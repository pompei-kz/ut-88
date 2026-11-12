//! Вывод звука ЮТ-88: перепады уровня выхода на магнитофон (бит 0 порта A1H) превращаются
//! в отсчёты и уходят в звуковую карту через `cpal`.
//!
//! Эмулятор забирает перепады у машины (`Machine::drain_sound_edges`) и передаёт их в
//! [`Audio::push_edges`] вместе с текущим временем в тактах кварца 16 МГц. Генератор
//! [`SampleGen`] переводит уровень в отсчёты (среднее значение уровня за период отсчёта —
//! простейший фильтр против наложения спектров) и убирает постоянную составляющую.
//!
//! Скорость эмуляции и звуковой карты немного различаются, поэтому шаг генератора
//! подстраивается по заполнению буфера (±2 %): буфер держится около [`TARGET_FILL`]
//! секунды. Если эмуляция идёт быстрее реального времени (скорость ×2 и выше) — лишние
//! отсчёты отбрасываются, если стоит — звук затухает до тишины.
//!
//! Если звукового устройства нет или его не удалось открыть, [`Audio`] работает «вхолостую»
//! и ничего не выводит.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

/// Частота кварца 16 МГц — единица времени перепадов.
pub const MASTER_HZ: f64 = 16_000_000.0;

/// Желаемое заполнение буфера, с.
pub const TARGET_FILL: f64 = 0.08;

/// Предельное заполнение буфера, с (больше — отсчёты отбрасываются).
pub const MAX_FILL: f64 = 0.25;

/// Амплитуда сигнала (из 1,0) при громкости 100 %.
const AMPLITUDE: f32 = 0.5;

/// Генератор отсчётов из перепадов уровня.
#[derive(Debug, Clone)]
pub struct SampleGen {
    /// Частота отсчётов, Гц.
    rate: f64,
    /// Время (такты 16 МГц), до которого отсчёты уже выданы.
    pos: f64,
    /// Текущий уровень.
    level: bool,
    /// Доля «единицы» в текущем недостроенном отсчёте, такты.
    acc_high: f64,
    /// Набранное время текущего отсчёта, такты.
    acc_total: f64,
    /// Состояние фильтра постоянной составляющей.
    dc_prev_in: f32,
    dc_prev_out: f32,
    /// Генератор начал работу (первый вызов задаёт начало отсчёта).
    started: bool,
    /// Поправка шага (1,0 — номинал).
    pub ratio: f64,
    /// Масштаб времени эмуляции для звука: на скоростях ниже ×1 равен скорости
    /// (звук растягивается и понижается, как у замедленной машины), иначе 1,0.
    pub time_scale: f64,
}

impl SampleGen {
    /// Новый генератор для частоты `rate` Гц.
    pub fn new(rate: u32) -> Self {
        Self {
            rate: f64::from(rate.max(1)),
            pos: 0.0,
            level: false,
            acc_high: 0.0,
            acc_total: 0.0,
            dc_prev_in: 0.0,
            dc_prev_out: 0.0,
            started: false,
            ratio: 1.0,
            time_scale: 1.0,
        }
    }

    /// Частота отсчётов.
    pub fn rate(&self) -> u32 {
        self.rate as u32
    }

    /// Тактов 16 МГц на отсчёт (с учётом поправки).
    fn step(&self) -> f64 {
        MASTER_HZ / self.rate * self.ratio * self.time_scale
    }

    /// Начать заново с момента `now` (после загрузки снимка, холодного старта).
    pub fn resync(&mut self, now: u64, level: bool) {
        self.pos = now as f64;
        self.level = level;
        self.acc_high = 0.0;
        self.acc_total = 0.0;
        self.started = true;
    }

    /// Обработать перепады `(такт, уровень)` и выдать отсчёты до момента `now`.
    pub fn feed(&mut self, edges: &[(u64, bool)], now: u64, out: &mut Vec<f32>) {
        if !self.started {
            let t0 = edges.first().map_or(now, |e| e.0).min(now);
            self.resync(t0, edges.first().is_some_and(|e| !e.1));
        }
        // Время пошло назад (новая машина) — начинаем заново.
        if (now as f64) + 1.0 < self.pos {
            self.resync(now, self.level);
        }
        for &(t, lv) in edges {
            self.advance(t as f64, out);
            self.level = lv;
        }
        self.advance(now as f64, out);
    }

    /// Довести генерацию до момента `t` при текущем уровне.
    fn advance(&mut self, t: f64, out: &mut Vec<f32>) {
        if t <= self.pos {
            return;
        }
        let step = self.step();
        // Начало текущего отсчёта.
        let mut cur = self.pos;
        loop {
            // Время, уже набранное в текущем отсчёте, хранится в `acc_*`; конец отсчёта —
            // когда суммарно набрано `step` тактов.
            let filled = self.acc_total;
            let need = step - filled;
            let avail = t - cur;
            if avail < need {
                self.add(avail);
                self.pos = t;
                return;
            }
            self.add(need);
            cur += need;
            let v = (self.acc_high / step) as f32 * 2.0 - 1.0;
            self.acc_high = 0.0;
            self.acc_total = 0.0;
            out.push(self.dc_block(v));
        }
    }

    fn add(&mut self, dt: f64) {
        if self.level {
            self.acc_high += dt;
        }
        self.acc_total += dt;
    }

    /// Фильтр постоянной составляющей (ФВЧ первого порядка, ≈ 20 Гц).
    fn dc_block(&mut self, x: f32) -> f32 {
        let r = 1.0 - (2.0 * std::f32::consts::PI * 20.0 / self.rate as f32);
        let y = x - self.dc_prev_in + r * self.dc_prev_out;
        self.dc_prev_in = x;
        self.dc_prev_out = y;
        y
    }
}

/// Общие данные потока вывода и эмулятора.
struct Shared {
    buf: VecDeque<f32>,
    /// Последний выданный отсчёт (для плавного затухания при опустошении буфера).
    last: f32,
}

/// Вывод звука.
pub struct Audio {
    stream: Option<cpal::Stream>,
    shared: Arc<Mutex<Shared>>,
    enabled: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
    gen_: SampleGen,
    scratch: Vec<f32>,
    error: Option<String>,
}

impl Audio {
    /// Открыть звуковое устройство по умолчанию. Не падает: если устройства нет, звук
    /// просто не выводится ([`Audio::is_available`] = `false`, причина — [`Audio::error`]).
    pub fn new() -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            buf: VecDeque::new(),
            last: 0.0,
        }));
        let enabled = Arc::new(AtomicBool::new(true));
        let volume = Arc::new(AtomicU32::new(0.7f32.to_bits()));
        let (stream, rate, error) = match open_stream(&shared, &enabled, &volume) {
            Ok((s, r)) => (Some(s), r, None),
            Err(e) => (None, 44_100, Some(e)),
        };
        Self {
            stream,
            shared,
            enabled,
            volume,
            gen_: SampleGen::new(rate),
            scratch: Vec::new(),
            error,
        }
    }

    /// Беззвучный вывод (для тестов и ключа `--no-sound`).
    pub fn silent() -> Self {
        Self {
            stream: None,
            shared: Arc::new(Mutex::new(Shared {
                buf: VecDeque::new(),
                last: 0.0,
            })),
            enabled: Arc::new(AtomicBool::new(false)),
            volume: Arc::new(AtomicU32::new(0)),
            gen_: SampleGen::new(44_100),
            scratch: Vec::new(),
            error: Some("звук выключен".into()),
        }
    }

    /// Устройство открыто.
    pub fn is_available(&self) -> bool {
        self.stream.is_some()
    }

    /// Почему устройство не открылось.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Частота отсчётов устройства.
    pub fn sample_rate(&self) -> u32 {
        self.gen_.rate()
    }

    /// Звук включён.
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Включить/выключить звук.
    pub fn set_enabled(&mut self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
        if !on && let Ok(mut s) = self.shared.lock() {
            s.buf.clear();
        }
    }

    /// Громкость 0,0–1,0.
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    /// Задать громкость 0,0–1,0.
    pub fn set_volume(&mut self, v: f32) {
        self.volume
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Начать отсчёт заново с момента `now` (такты 16 МГц) — после замены машины.
    pub fn resync(&mut self, now: u64, level: bool) {
        self.gen_.resync(now, level);
        if let Ok(mut s) = self.shared.lock() {
            s.buf.clear();
        }
    }

    /// Передать перепады `(такт 16 МГц, уровень)` и текущее время `now`.
    pub fn push_edges(&mut self, edges: &[(u64, bool)], now: u64) {
        if self.stream.is_none() || !self.enabled() {
            // Держим генератор в курсе времени, чтобы при включении не было скачка.
            if let Some(&(_, lv)) = edges.last() {
                self.gen_.level = lv;
            }
            self.gen_.resync(now, self.gen_.level);
            return;
        }
        let rate = f64::from(self.gen_.rate());
        let fill = self.shared.lock().map(|s| s.buf.len()).unwrap_or(0) as f64 / rate;
        // Подстройка шага: буфер полнее нормы — отсчёты реже, и наоборот.
        let err = (fill - TARGET_FILL) / TARGET_FILL;
        self.gen_.ratio = (1.0 + 0.02 * err).clamp(0.98, 1.02);
        self.scratch.clear();
        self.gen_.feed(edges, now, &mut self.scratch);
        if let Ok(mut s) = self.shared.lock() {
            s.buf.extend(self.scratch.iter().copied());
            let max = (MAX_FILL * rate) as usize;
            if s.buf.len() > max {
                let target = (TARGET_FILL * rate) as usize;
                let drop = s.buf.len() - target;
                s.buf.drain(..drop);
            }
        }
    }

    /// Сообщить скорость эмуляции (`None` — максимум). На скоростях ниже ×1 звук
    /// растягивается во времени: иначе отсчётов не хватает, буфер пустеет и звук хрипит.
    /// На ×1 и выше масштаб 1,0 (лишние отсчёты отбрасываются при переполнении буфера).
    pub fn set_speed(&mut self, speed: Option<f64>) {
        self.gen_.time_scale = match speed {
            Some(k) if k > 0.0 && k < 1.0 => k,
            _ => 1.0,
        };
    }

    /// Заполнение буфера, с.
    pub fn buffered(&self) -> f64 {
        self.shared.lock().map(|s| s.buf.len()).unwrap_or(0) as f64 / f64::from(self.gen_.rate())
    }

    /// Приостановить вывод (поток продолжает работать, выдаёт тишину).
    pub fn pause(&self) {
        if let Some(s) = &self.stream {
            let _ = s.pause();
        }
    }

    /// Возобновить вывод.
    pub fn play(&self) {
        if let Some(s) = &self.stream {
            let _ = s.play();
        }
    }
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

fn open_stream(
    shared: &Arc<Mutex<Shared>>,
    enabled: &Arc<AtomicBool>,
    volume: &Arc<AtomicU32>,
) -> Result<(cpal::Stream, u32), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "нет звукового устройства".to_string())?;
    let supported = device
        .default_output_config()
        .map_err(|e| format!("нет конфигурации вывода: {e}"))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let rate = config.sample_rate;
    let s = match format {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, shared, enabled, volume),
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, shared, enabled, volume),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, shared, enabled, volume),
        cpal::SampleFormat::I32 => build::<i32>(&device, &config, shared, enabled, volume),
        cpal::SampleFormat::U8 => build::<u8>(&device, &config, shared, enabled, volume),
        f => return Err(format!("формат {f:?} не поддерживается")),
    }?;
    s.play().map_err(|e| format!("не запускается поток: {e}"))?;
    Ok((s, rate))
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: &Arc<Mutex<Shared>>,
    enabled: &Arc<AtomicBool>,
    volume: &Arc<AtomicU32>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels.max(1));
    let shared = Arc::clone(shared);
    let enabled = Arc::clone(enabled);
    let volume = Arc::clone(volume);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let on = enabled.load(Ordering::Relaxed);
                let vol = f32::from_bits(volume.load(Ordering::Relaxed)) * AMPLITUDE;
                let Ok(mut s) = shared.lock() else {
                    data.fill(T::from_sample(0.0));
                    return;
                };
                for frame in data.chunks_mut(channels) {
                    let v = match s.buf.pop_front() {
                        Some(v) if on => {
                            s.last = v;
                            v
                        }
                        _ => {
                            // Буфер пуст: плавно уходим в тишину без щелчка.
                            s.last *= 0.995;
                            s.last
                        }
                    };
                    let out = T::from_sample(v * vol);
                    frame.fill(out);
                }
            },
            |e| eprintln!("ut88-audio: {e}"),
            None,
        )
        .map_err(|e| format!("не открывается поток: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_wave_gives_samples() {
        let mut g = SampleGen::new(1000);
        let mut out = Vec::new();
        // Меандр 100 Гц: перепад каждые 80 000 тактов 16 МГц (5 мс).
        let edges: Vec<(u64, bool)> = (1..=20).map(|i| (i * 80_000, i % 2 == 1)).collect();
        g.feed(&edges, 21 * 80_000, &mut out);
        // 21 × 5 мс = 105 мс → около 105 отсчётов.
        assert!((100..=106).contains(&out.len()), "{}", out.len());
        // Сигнал меняет знак.
        assert!(out.iter().any(|&v| v > 0.5));
        assert!(out.iter().any(|&v| v < -0.5));
    }

    #[test]
    fn slow_speed_stretches_sound() {
        // На ×0,5 одна эмулированная секунда должна дать две секунды звука.
        let mut g = SampleGen::new(1000);
        g.time_scale = 0.5;
        let mut out = Vec::new();
        g.feed(&[(0, true)], 16_000_000, &mut out);
        assert_eq!(out.len(), 2000);
    }

    #[test]
    fn constant_level_decays_to_zero() {
        let mut g = SampleGen::new(1000);
        let mut out = Vec::new();
        g.feed(&[(0, true)], 16_000_000, &mut out);
        assert_eq!(out.len(), 1000);
        assert!(out.last().unwrap().abs() < 0.01);
    }

    #[test]
    fn time_going_back_resyncs() {
        let mut g = SampleGen::new(1000);
        let mut out = Vec::new();
        g.feed(&[], 16_000_000, &mut out);
        out.clear();
        g.feed(&[], 1_000, &mut out);
        assert!(out.is_empty());
        g.feed(&[], 1_000 + 160_000, &mut out);
        assert_eq!(out.len(), 10);
    }

    /// Проверка на настоящей звуковой карте: `cargo test -p ut88-audio -- --ignored`.
    #[test]
    #[ignore]
    fn real_device_plays_tone() {
        let mut a = Audio::new();
        if !a.is_available() {
            eprintln!("нет устройства: {:?}", a.error());
            return;
        }
        // 440 Гц, 0,3 с, порциями по 20 мс — как кадры эмулятора.
        let half = (MASTER_HZ / 880.0) as u64;
        let mut t = 0u64;
        let mut level = false;
        for frame in 1..=15u64 {
            let end = frame * 320_000;
            let mut edges = Vec::new();
            while t + half <= end {
                t += half;
                level = !level;
                edges.push((t, level));
            }
            a.push_edges(&edges, end);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let filled = a.buffered();
        assert!(filled > 0.0 && filled <= MAX_FILL, "{filled}");
        std::thread::sleep(std::time::Duration::from_millis(400));
        assert!(a.buffered() < 0.01, "буфер не опустел: {}", a.buffered());
    }

    #[test]
    fn silent_audio_does_not_panic() {
        let mut a = Audio::silent();
        a.push_edges(&[(10, true)], 1000);
        assert!(!a.is_available());
    }
}
