//! Магнитофон: запись выхода порта A1H в WAV, воспроизведение WAV на вход, формирование
//! сигнала блоков МОНИТОРА-«0» и МОНИТОРА-«F» из двоичных данных.
//!
//! Время везде — в тактах задающего генератора 16 МГц ([`MASTER_HZ`]); один такт
//! процессора равен [`CPU_DIV`] = 9 таким тактам.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Частота кварца КР580ГФ24, Гц.
pub const MASTER_HZ: u64 = 16_000_000;
/// Делитель КР580ГФ24: такт процессора = 9 тактов кварца (≈ 1,78 МГц).
pub const CPU_DIV: u64 = 9;
/// Частота дискретизации WAV, которую пишет эмулятор.
pub const WAV_RATE: u32 = 44_100;

/// Амплитуда записываемого сигнала (из 32767).
const AMPLITUDE: i16 = 16_000;

/// Звуковой сигнал (моно, 16 бит).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TapeSignal {
    /// Частота дискретизации, Гц.
    pub rate: u32,
    /// Отсчёты.
    pub samples: Vec<i16>,
}

impl TapeSignal {
    /// Длительность, с.
    pub fn duration(&self) -> f64 {
        if self.rate == 0 {
            0.0
        } else {
            self.samples.len() as f64 / f64::from(self.rate)
        }
    }

    /// Прочитать WAV (любая разрядность, PCM или float; из многоканального берётся
    /// первый канал).
    pub fn load_wav(path: impl AsRef<Path>) -> io::Result<Self> {
        let reader = hound::WavReader::open(path).map_err(wav_err)?;
        Self::from_wav_reader(reader)
    }

    /// Прочитать WAV из потока.
    pub fn read_wav<R: io::Read>(r: R) -> io::Result<Self> {
        let reader = hound::WavReader::new(r).map_err(wav_err)?;
        Self::from_wav_reader(reader)
    }

    fn from_wav_reader<R: io::Read>(mut reader: hound::WavReader<R>) -> io::Result<Self> {
        let spec = reader.spec();
        let ch = usize::from(spec.channels.max(1));
        let samples: Vec<i16> = match spec.sample_format {
            hound::SampleFormat::Float => reader
                .samples::<f32>()
                .step_by(ch)
                .map(|s| s.map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16))
                .collect::<Result<_, _>>()
                .map_err(wav_err)?,
            hound::SampleFormat::Int => {
                let bits = u32::from(spec.bits_per_sample);
                reader
                    .samples::<i32>()
                    .step_by(ch)
                    .map(|s| {
                        s.map(|v| {
                            if bits > 16 {
                                (v >> (bits - 16)) as i16
                            } else {
                                (v << (16 - bits)) as i16
                            }
                        })
                    })
                    .collect::<Result<_, _>>()
                    .map_err(wav_err)?
            }
        };
        Ok(Self {
            rate: spec.sample_rate,
            samples,
        })
    }

    /// Записать WAV (моно, 16 бит).
    pub fn save_wav(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let file = std::fs::File::create(path)?;
        self.write_wav(io::BufWriter::new(file))
    }

    /// Записать WAV в поток.
    pub fn write_wav<W: io::Write + io::Seek>(&self, w: W) -> io::Result<()> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::new(w, spec).map_err(wav_err)?;
        for &s in &self.samples {
            writer.write_sample(s).map_err(wav_err)?;
        }
        writer.finalize().map_err(wav_err)
    }

    /// Построить сигнал по списку перепадов уровня `(такт 16 МГц, уровень)`.
    ///
    /// `start`/`end` — границы записи, `start_level` — уровень до первого перепада.
    pub fn from_edges(
        start: u64,
        end: u64,
        start_level: bool,
        edges: &[(u64, bool)],
        rate: u32,
    ) -> Self {
        let n = ticks_to_samples(end.saturating_sub(start), rate);
        let mut samples = Vec::with_capacity(n as usize);
        let mut level = start_level;
        let mut i = 0;
        for k in 0..n {
            let t = start + samples_to_ticks(k, rate);
            while i < edges.len() && edges[i].0 <= t {
                level = edges[i].1;
                i += 1;
            }
            samples.push(if level { AMPLITUDE } else { -AMPLITUDE });
        }
        Self { rate, samples }
    }
}

fn wav_err(e: hound::Error) -> io::Error {
    match e {
        hound::Error::IoError(e) => e,
        e => io::Error::new(io::ErrorKind::InvalidData, e.to_string()),
    }
}

fn ticks_to_samples(ticks: u64, rate: u32) -> u64 {
    (u128::from(ticks) * u128::from(rate) / u128::from(MASTER_HZ)) as u64
}

fn samples_to_ticks(n: u64, rate: u32) -> u64 {
    (u128::from(n) * u128::from(MASTER_HZ) / u128::from(rate)) as u64
}

/// «Кассета» в магнитофоне: сигнал подаётся на вход порта A1H (бит 0).
///
/// Вставленная кассета стоит, пока её не запустят: явной командой
/// ([`TapePlayer::start`]) или автоматически — при первом чтении программой порта
/// магнитофона после вставки (имитация: МОНИТОР начал читать — пользователь нажал
/// «Воспроизведение»). Остановленная лента подаёт на вход 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapePlayer {
    /// Сигнал.
    pub signal: TapeSignal,
    /// Позиция ленты (тактов 16 МГц от её начала) в момент последнего пуска или останова.
    pub offset: u64,
    /// Такт (16 МГц) последнего пуска; `None` — лента стоит.
    pub start_tick: Option<u64>,
    /// Запустить ленту при первом чтении порта магнитофона.
    pub auto_start: bool,
    /// Порог: отсчёт больше порога — логическая 1.
    pub threshold: i16,
    /// Инвертировать уровень (МОНИТОРЫ умеют сами, но вдруг).
    pub invert: bool,
}

impl TapePlayer {
    /// Вставленная кассета: лента стоит в начале, автопуск взведён.
    pub fn new(signal: TapeSignal) -> Self {
        Self {
            signal,
            offset: 0,
            start_tick: None,
            auto_start: true,
            threshold: 0,
            invert: false,
        }
    }

    /// Позиция ленты в тактах 16 МГц от её начала.
    fn pos_ticks(&self, tick: u64) -> u64 {
        match self.start_tick {
            Some(t0) => self.offset + tick.saturating_sub(t0),
            None => self.offset,
        }
    }

    /// Лента движется.
    pub fn playing(&self) -> bool {
        self.start_tick.is_some()
    }

    /// Пустить ленту с текущей позиции (с такта `tick`).
    pub fn start(&mut self, tick: u64) {
        if self.start_tick.is_none() {
            self.start_tick = Some(tick);
        }
        self.auto_start = false;
    }

    /// Остановить ленту в такте `tick` (позиция сохраняется, автопуск снимается).
    pub fn pause(&mut self, tick: u64) {
        self.offset = self.pos_ticks(tick);
        self.start_tick = None;
        self.auto_start = false;
    }

    /// Перемотать в начало и остановить; автопуск взводится снова.
    pub fn rewind(&mut self) {
        self.offset = 0;
        self.start_tick = None;
        self.auto_start = true;
    }

    /// Уровень на входе в момент `tick`; `None` — лента кончилась.
    pub fn level_at(&self, tick: u64) -> Option<bool> {
        let k = ticks_to_samples(self.pos_ticks(tick), self.signal.rate) as usize;
        let s = *self.signal.samples.get(k)?;
        if self.playing() {
            Some((s > self.threshold) != self.invert)
        } else {
            Some(false)
        }
    }

    /// Позиция воспроизведения в секундах (не дальше конца ленты).
    pub fn position(&self, tick: u64) -> f64 {
        (self.pos_ticks(tick) as f64 / MASTER_HZ as f64).min(self.signal.duration())
    }

    /// Лента кончилась.
    pub fn finished(&self, tick: u64) -> bool {
        self.level_at(tick).is_none()
    }
}

/// Запись выхода порта A1H.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TapeRecorder {
    /// Такт начала записи.
    pub start_tick: u64,
    /// Уровень выхода в момент начала.
    pub start_level: bool,
    /// Перепады `(такт 16 МГц, новый уровень)`.
    pub edges: Vec<(u64, bool)>,
}

impl TapeRecorder {
    /// Получить сигнал от начала записи до такта `end`.
    pub fn to_signal(&self, end: u64, rate: u32) -> TapeSignal {
        TapeSignal::from_edges(self.start_tick, end, self.start_level, &self.edges, rate)
    }
}

/// Формат блока на ленте.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TapeFormat {
    /// МОНИТОР модуля 1 (директивы 9/A, «Микро-80»): 256 × 00H, E6H, начало (ст., мл.),
    /// конец (ст., мл.), данные.
    Monitor0,
    /// МОНИТОР-«F» (директивы O/I, «РК-86»): то же + 00H, 00H, E6H, контрольная сумма
    /// (ст., мл.).
    MonitorF,
}

impl TapeFormat {
    /// Длительность половины бита в тактах процессора — по циклам задержки в драйверах
    /// записи МОНИТОРОВ (0100H и FBEEH при константе 1DH), усреднённо.
    pub fn half_bit_cycles(self) -> u64 {
        match self {
            TapeFormat::Monitor0 => 520,
            TapeFormat::MonitorF => 797,
        }
    }

    /// Удлинение второй половины последнего бита байта, тактов процессора: время, которое
    /// драйвер записи тратит между байтами (у МОНИТОРА-«0» ≈ 140–190 тактов; у
    /// МОНИТОРА-«F» последний бит специально укорочен и пауза почти не меняется).
    ///
    /// Без этой паузы драйвер чтения МОНИТОРА-«0» не успевает между байтами и теряет фазу.
    pub fn byte_gap_cycles(self) -> u64 {
        match self {
            TapeFormat::Monitor0 => 160,
            TapeFormat::MonitorF => 0,
        }
    }
}

/// Контрольная сумма МОНИТОРОВ: 16-разрядная сумма байтов (директива 8, подпрограмма
/// F82AH).
pub fn checksum(data: &[u8]) -> u16 {
    data.iter().fold(0u16, |s, &b| s.wrapping_add(u16::from(b)))
}

/// Байты блока, которые МОНИТОР выводит на ленту (с пилот-тоном и синхробайтом).
///
/// Порядок байтов адресов — старший, затем младший (так в коде обоих МОНИТОРОВ).
pub fn encode_block(format: TapeFormat, start: u16, data: &[u8]) -> Vec<u8> {
    assert!(!data.is_empty(), "пустой блок");
    let end = start.wrapping_add((data.len() - 1) as u16);
    let mut out = vec![0u8; 256];
    out.push(0xE6);
    out.extend_from_slice(&start.to_be_bytes());
    out.extend_from_slice(&end.to_be_bytes());
    out.extend_from_slice(data);
    if format == TapeFormat::MonitorF {
        out.extend_from_slice(&[0x00, 0x00, 0xE6]);
        out.extend_from_slice(&checksum(data).to_be_bytes());
    }
    out
}

/// Сигнал (WAV) блока: двухфазный код, старший бит вперёд; в первой половине бита —
/// инверсное значение, во второй — прямое (как выводят оба МОНИТОРА). До и после блока —
/// по 0,1 с тишины (уровень 0).
pub fn block_signal(format: TapeFormat, start: u16, data: &[u8], rate: u32) -> TapeSignal {
    let bytes = encode_block(format, start, data);
    let half = format.half_bit_cycles() * CPU_DIV;
    let gap = format.byte_gap_cycles() * CPU_DIV;
    let lead = MASTER_HZ / 10;
    let mut t = lead;
    let mut edges = Vec::with_capacity(bytes.len() * 16);
    for b in bytes {
        for i in (0..8).rev() {
            let bit = (b >> i) & 1 != 0;
            edges.push((t, !bit));
            t += half;
            edges.push((t, bit));
            t += half;
        }
        t += gap;
    }
    edges.push((t, false));
    TapeSignal::from_edges(0, t + lead, false, &edges, rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_bytes() {
        let b = encode_block(TapeFormat::MonitorF, 0xC000, &[1, 2, 3]);
        assert_eq!(b.len(), 256 + 5 + 3 + 5);
        assert_eq!(&b[256..261], &[0xE6, 0xC0, 0x00, 0xC0, 0x02]);
        assert_eq!(&b[264..], &[0, 0, 0xE6, 0, 6]);
        let b = encode_block(TapeFormat::Monitor0, 0xC000, &[1, 2, 3]);
        assert_eq!(b.len(), 256 + 5 + 3);
    }

    #[test]
    fn wav_roundtrip() {
        let s = block_signal(TapeFormat::Monitor0, 0xC000, &[0x55], WAV_RATE);
        let mut buf = io::Cursor::new(Vec::new());
        s.write_wav(&mut buf).unwrap();
        buf.set_position(0);
        let r = TapeSignal::read_wav(buf).unwrap();
        assert_eq!(r, s);
    }
}
