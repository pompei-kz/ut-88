//! Очередь нажатий: перевод событий терминала в нажатия и отпускания клавиш машины.
//!
//! Терминал обычно не сообщает об отпускании клавиши, а программы ЮТ-88 опрашивают
//! клавиатуру по-разному, поэтому каждое нажатие удерживается в машине само:
//!
//! - кнопка модуля 1 держится, пока программа не прочитает порт клавиатуры (A0H), и
//!   ещё 20 мс (МОНИТОР между цифрами делает паузу ≈ 0,35 с и только потом опрашивает
//!   кнопки, а удержание дольше паузы дало бы повтор); если порт не читают — до 1 с;
//! - клавиша полной клавиатуры держится 0,4 с (МОНИТОР-«F» повторяет символ при
//!   удержании ≥ 0,5 с, а «Тетрис» опрашивает клавиатуру редко); если в очереди уже
//!   ждут следующие клавиши — 0,15 с и пауза 0,1 с (быстрый набор);
//! - автоповтор терминала (то же событие чаще 70 мс) продлевает удержание;
//! - если терминал сообщает об отпускании (протокол клавиатуры kitty), клавиша
//!   держится, пока её не отпустят (но не меньше указанного выше минимума).
//!
//! Время удержания отсчитывается в тактах машины (то есть в эмулированном времени).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ut88_core::{CPU_HZ, KbKey, Machine, PadKey};

/// Что нажать в машине.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Кнопка модуля 1.
    Pad(PadKey),
    /// Клавиша полной клавиатуры и модификатор (РУС, УС, СС).
    Kb(KbKey, Option<KbKey>),
}

impl Target {
    fn down(self, m: &mut Machine) {
        match self {
            Target::Pad(k) => m.pad_down(k),
            Target::Kb(k, md) => {
                if let Some(md) = md {
                    m.kb_down(md);
                }
                m.kb_down(k);
            }
        }
    }

    fn up(self, m: &mut Machine) {
        match self {
            Target::Pad(k) => m.pad_up(k),
            Target::Kb(k, md) => {
                m.kb_up(k);
                if let Some(md) = md {
                    m.kb_up(md);
                }
            }
        }
    }

    fn is_pad(self) -> bool {
        matches!(self, Target::Pad(_))
    }
}

fn secs(s: f64) -> u64 {
    (s * CPU_HZ) as u64
}

/// Удержание клавиши полной клавиатуры, когда очередь пуста, с.
pub const KB_HOLD: f64 = 0.4;
/// Удержание клавиши полной клавиатуры при быстром наборе, с.
pub const KB_SHORT: f64 = 0.15;
/// Пауза между клавишами полной клавиатуры, с.
pub const KB_GAP: f64 = 0.1;
/// Удержание кнопки модуля 1 после чтения порта, с.
pub const PAD_AFTER_READ: f64 = 0.02;
/// Наибольшее удержание кнопки модуля 1, если порт не читают, с.
pub const PAD_MAX: f64 = 1.0;
/// Пауза между кнопками модуля 1, с.
pub const PAD_GAP: f64 = 0.02;
/// Продление удержания автоповтором терминала, с.
pub const REPEAT_EXTEND: f64 = 0.15;
/// События одной клавиши чаще этого интервала считаются автоповтором.
pub const REPEAT_WINDOW: Duration = Duration::from_millis(70);
/// Наибольшая длина очереди.
const QUEUE_MAX: usize = 64;

#[derive(Debug, Clone)]
struct Req {
    target: Target,
    /// Клавиша ПК (для сопоставления отпускания).
    ident: String,
    /// Клавиша ПК ещё держится (известно только с протоколом kitty).
    user_down: bool,
}

#[derive(Debug, Clone)]
struct Active {
    req: Req,
    since: u64,
    /// Продление удержания (автоповтор).
    ext_until: u64,
    /// Такт, когда программа прочитала порт кнопок модуля 1.
    read_at: Option<u64>,
}

/// Очередь нажатий.
#[derive(Debug, Default)]
pub struct InputQueue {
    queue: VecDeque<Req>,
    cur: Option<Active>,
    gap_until: u64,
    last_event: Option<(String, Instant)>,
    pending_extend: bool,
}

impl InputQueue {
    /// Пустая очередь.
    pub fn new() -> Self {
        Self::default()
    }

    /// Нажатие клавиши ПК `ident`. `user_down` — терминал сообщит об отпускании.
    pub fn press(&mut self, target: Target, ident: &str, user_down: bool, now: Instant) {
        let repeat = matches!(&self.last_event, Some((id, t)) if id == ident
            && now.duration_since(*t) < REPEAT_WINDOW);
        self.last_event = Some((ident.to_string(), now));
        if repeat {
            self.repeat(ident);
            return;
        }
        if self.queue.len() < QUEUE_MAX {
            self.queue.push_back(Req {
                target,
                ident: ident.to_string(),
                user_down,
            });
        }
    }

    /// Автоповтор клавиши `ident`: продлить её удержание.
    pub fn repeat(&mut self, ident: &str) {
        if let Some(a) = &mut self.cur
            && a.req.ident == ident
        {
            // Точное время продления выставит tick() по текущему такту.
            a.ext_until = u64::MAX;
            self.pending_extend = true;
        }
    }

    /// Отпускание клавиши ПК (протокол kitty).
    pub fn release(&mut self, ident: &str, now_cycles: u64) {
        if let Some(a) = &mut self.cur
            && a.req.ident == ident
            && a.req.user_down
        {
            a.req.user_down = false;
            a.ext_until = now_cycles;
            return;
        }
        if let Some(r) = self
            .queue
            .iter_mut()
            .find(|r| r.ident == ident && r.user_down)
        {
            r.user_down = false;
        }
    }

    /// Программа читает порт кнопок модуля 1.
    pub fn on_pad_read(&mut self, now: u64) {
        if let Some(a) = &mut self.cur
            && a.req.target.is_pad()
            && a.read_at.is_none()
        {
            a.read_at = Some(now);
        }
    }

    /// Что сейчас нажато.
    pub fn current(&self) -> Option<Target> {
        self.cur.as_ref().map(|a| a.req.target)
    }

    /// Нажатий в очереди (без текущего).
    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    /// Отпустить всё и очистить очередь.
    pub fn clear(&mut self, m: &mut Machine) {
        self.queue.clear();
        self.cur = None;
        self.gap_until = 0;
        m.release_all_keys();
    }

    /// Обновить нажатия по текущему такту машины.
    pub fn tick(&mut self, m: &mut Machine) {
        let now = m.cycles();
        if std::mem::take(&mut self.pending_extend)
            && let Some(a) = &mut self.cur
        {
            a.ext_until = now + secs(REPEAT_EXTEND);
        }
        if let Some(a) = &self.cur
            && self.should_release(a, now)
        {
            let a = self.cur.take().expect("есть нажатие");
            a.req.target.up(m);
            let gap = if a.req.target.is_pad() {
                PAD_GAP
            } else {
                KB_GAP
            };
            self.gap_until = now + secs(gap);
        }
        if self.cur.is_none()
            && now >= self.gap_until
            && let Some(req) = self.queue.pop_front()
        {
            req.target.down(m);
            self.cur = Some(Active {
                req,
                since: now,
                ext_until: now,
                read_at: None,
            });
        }
    }

    fn should_release(&self, a: &Active, now: u64) -> bool {
        if a.req.user_down || now < a.ext_until {
            return false;
        }
        if a.req.target.is_pad() {
            match a.read_at {
                Some(t) => now >= t + secs(PAD_AFTER_READ),
                None => now >= a.since + secs(PAD_MAX),
            }
        } else {
            let hold = if self.queue.is_empty() {
                KB_HOLD
            } else {
                KB_SHORT
            };
            now >= a.since + secs(hold)
        }
    }
}
