//! Отладчик: точки останова, пошаговое выполнение, окно с регистрами, дизассемблером
//! и дампом памяти.
//!
//! Окно отладчика — отдельное окно ОС со своим контекстом виджетов; [`Debugger::frame`]
//! рисует его кадр в любом [`Ctx`] и не зависит от winit (в тестах — стенд без окна).

use ut88_core::Machine;

use crate::gfx::Color;
use crate::ui::{
    CentralPanel, ComboBox, Ctx, Grid, KeyCode, Label, RichText, ScrollArea, Sense, TextEdit,
    TextStyle, Ui,
};

use crate::loader::parse_hex_u16;

/// Направление обращения к порту для точки останова.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDir {
    /// Команда IN.
    In,
    /// Команда OUT.
    Out,
    /// Любая.
    Any,
}

/// Состояние отладчика.
pub struct Debugger {
    /// Окно открыто.
    pub open: bool,
    /// Точки останова по адресу команды.
    pub breakpoints: Vec<u16>,
    /// Точки останова по порту.
    pub port_breaks: Vec<(u8, PortDir)>,
    /// Временная точка «выполнить до адреса» (и шаг с обходом CALL).
    pub run_to: Option<u16>,
    /// Пропустить проверку на первой команде (чтобы уйти с точки останова).
    pub skip_once: bool,
    /// Причина последнего останова.
    pub stop_reason: Option<String>,
    /// Счётчик тактов на момент последнего останова/пуска.
    pub mark_cycles: u64,
    // Поля ввода.
    bp_input: String,
    port_input: String,
    port_dir: PortDir,
    run_to_input: String,
    mem_addr: u16,
    mem_addr_input: String,
    edit_addr_input: String,
    edit_bytes_input: String,
    reg_inputs: [String; 6],
    regs_seen_pc: Option<u16>,
    follow_pc: bool,
    message: String,
}

/// Что попросило окно отладчика.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbgAction {
    /// Ничего.
    None,
    /// Пауза.
    Pause,
    /// Продолжить.
    Continue,
    /// Шаг на команду.
    Step,
    /// Шаг с обходом CALL.
    StepOver,
    /// Выполнить до адреса.
    RunTo(u16),
}

impl Default for Debugger {
    fn default() -> Self {
        Self {
            open: false,
            breakpoints: Vec::new(),
            port_breaks: Vec::new(),
            run_to: None,
            skip_once: false,
            stop_reason: None,
            mark_cycles: 0,
            bp_input: String::new(),
            port_input: String::new(),
            port_dir: PortDir::Any,
            run_to_input: String::new(),
            mem_addr: 0xC000,
            mem_addr_input: "C000".into(),
            edit_addr_input: String::new(),
            edit_bytes_input: String::new(),
            reg_inputs: Default::default(),
            regs_seen_pc: None,
            follow_pc: false,
            message: String::new(),
        }
    }
}

/// Команда вызова подпрограммы (CALL, Cусл, RST) — для шага с обходом.
pub fn is_call(op: u8) -> bool {
    op & 0xC7 == 0xC4 || op & 0xCF == 0xCD || op & 0xC7 == 0xC7
}

impl Debugger {
    /// Нужна ли покомандная проверка (есть точки останова).
    pub fn active(&self) -> bool {
        !self.breakpoints.is_empty() || !self.port_breaks.is_empty() || self.run_to.is_some()
    }

    /// Проверить, не пора ли остановиться перед командой по текущему PC.
    fn check(&mut self, m: &Machine) -> Option<String> {
        let pc = m.cpu.pc;
        if self.run_to == Some(pc) {
            self.run_to = None;
            return Some(format!("достигнут адрес {pc:04X}"));
        }
        if self.breakpoints.contains(&pc) {
            return Some(format!("точка останова {pc:04X}"));
        }
        if !self.port_breaks.is_empty() {
            let op = m.peek(pc);
            let dir = match op {
                0xDB => Some(PortDir::In),
                0xD3 => Some(PortDir::Out),
                _ => None,
            };
            if let Some(dir) = dir {
                let port = m.peek(pc.wrapping_add(1));
                for &(bp, bd) in &self.port_breaks {
                    if bp == port && (bd == PortDir::Any || bd == dir) {
                        let what = if dir == PortDir::In { "IN" } else { "OUT" };
                        return Some(format!("{what} {port:02X}H по адресу {pc:04X}"));
                    }
                }
            }
        }
        None
    }

    /// Выполнить не меньше `n` тактов с проверкой точек останова. Возвращает причину
    /// останова, если он случился.
    pub fn run(&mut self, m: &mut Machine, n: u64) -> Option<String> {
        if !self.active() {
            self.skip_once = false;
            m.run_cycles(n);
            return None;
        }
        let end = m.cycles() + n;
        while m.cycles() < end {
            if !self.skip_once
                && let Some(r) = self.check(m)
            {
                self.stop_reason = Some(r.clone());
                return Some(r);
            }
            self.skip_once = false;
            m.step();
        }
        None
    }

    /// Шаг с обходом CALL: если на PC вызов — поставить временную точку за ним.
    /// Возвращает `true`, если нужно продолжить выполнение (до временной точки).
    pub fn step_over(&mut self, m: &mut Machine) -> bool {
        let pc = m.cpu.pc;
        let op = m.peek(pc);
        if is_call(op) {
            self.run_to = Some(pc.wrapping_add(i8080::instruction_len(op) as u16));
            self.skip_once = true;
            true
        } else {
            m.step();
            false
        }
    }

    /// Кадр окна отладчика в контексте `ctx` (окно ОС «ЮТ-88 — отладчик» или стенд
    /// тестов). `paused` — эмуляция стоит.
    pub fn frame(&mut self, ctx: &Ctx, m: &mut Machine, paused: bool) -> DbgAction {
        CentralPanel::default()
            .show(ctx, |ui| {
                ScrollArea::vertical()
                    .id_salt("отладчик")
                    .show(ui, |ui| self.ui(ui, m, paused))
            })
            .inner
    }

    fn ui(&mut self, ui: &mut Ui, m: &mut Machine, paused: bool) -> DbgAction {
        let mut action = DbgAction::None;
        ui.horizontal(|ui| {
            if paused {
                if ui.button("▶ Продолжить").clicked() {
                    action = DbgAction::Continue;
                }
            } else if ui.button("⏸ Пауза").clicked() {
                action = DbgAction::Pause;
            }
            if ui
                .button("Шаг")
                .on_hover_text("выполнить одну команду")
                .clicked()
            {
                action = DbgAction::Step;
            }
            if ui
                .button("Шаг с обходом")
                .on_hover_text("CALL/RST выполняется целиком")
                .clicked()
            {
                action = DbgAction::StepOver;
            }
            ui.separator();
            ui.label("до адреса:");
            let r = ui.add(TextEdit::singleline(&mut self.run_to_input).desired_width(50.0));
            let go = ui.button("Пуск до").clicked()
                || (r.lost_focus() && ui.input(|i| i.key_pressed(KeyCode::Enter)));
            if go {
                match parse_hex_u16(&self.run_to_input) {
                    Ok(a) => action = DbgAction::RunTo(a),
                    Err(e) => self.message = e.to_string(),
                }
            }
        });
        if let Some(r) = &self.stop_reason {
            ui.label(RichText::new(format!("Останов: {r}")).color(Color::LIGHT_RED));
        }
        if !self.message.is_empty() {
            ui.label(RichText::new(&self.message).color(Color::YELLOW));
        }
        ui.separator();

        ui.columns(2, |cols| {
            self.registers(&mut cols[0], m, paused);
            cols[0].separator();
            self.breakpoints_ui(&mut cols[0]);
            self.disassembly(&mut cols[1], m);
        });
        ui.separator();
        self.memory(ui, m);
        action
    }

    fn registers(&mut self, ui: &mut Ui, m: &mut Machine, paused: bool) {
        let c = &m.cpu;
        ui.heading("Регистры");
        // Поля ввода обновляются, когда процессор сдвинулся.
        if self.regs_seen_pc != Some(c.pc) || !paused {
            self.reg_inputs = [
                format!("{:04X}", c.psw()),
                format!("{:04X}", c.bc()),
                format!("{:04X}", c.de()),
                format!("{:04X}", c.hl()),
                format!("{:04X}", c.sp),
                format!("{:04X}", c.pc),
            ];
            self.regs_seen_pc = Some(c.pc);
        }
        const NAMES: [&str; 6] = ["PSW", "BC", "DE", "HL", "SP", "PC"];
        let mut changed: Option<(usize, u16)> = None;
        Grid::new("regs").num_columns(2).show(ui, |ui| {
            for (i, name) in NAMES.iter().enumerate() {
                ui.monospace(*name);
                let r = ui.add_enabled(
                    paused,
                    TextEdit::singleline(&mut self.reg_inputs[i])
                        .desired_width(50.0)
                        .font(TextStyle::Monospace),
                );
                if r.lost_focus()
                    && let Ok(v) = parse_hex_u16(&self.reg_inputs[i])
                {
                    changed = Some((i, v));
                }
                ui.end_row();
            }
        });
        if let Some((i, v)) = changed {
            let c = &mut m.cpu;
            match i {
                0 => c.set_psw(v),
                1 => c.set_bc(v),
                2 => c.set_de(v),
                3 => c.set_hl(v),
                4 => c.sp = v,
                _ => {
                    c.pc = v;
                    c.halted = false;
                }
            }
            self.regs_seen_pc = None;
        }
        let c = &m.cpu;
        let f = |b: bool, s: &'static str| if b { s } else { "-" };
        ui.monospace(format!(
            "Флаги: S={} Z={} AC={} P={} C={}   {}{}{}{}{}",
            u8::from(c.flag_s),
            u8::from(c.flag_z),
            u8::from(c.flag_ac),
            u8::from(c.flag_p),
            u8::from(c.flag_c),
            f(c.flag_s, "S"),
            f(c.flag_z, "Z"),
            f(c.flag_ac, "A"),
            f(c.flag_p, "P"),
            f(c.flag_c, "C"),
        ));
        ui.monospace(format!(
            "INTE={}  HLT={}  (SP): {:02X}{:02X}",
            u8::from(c.inte),
            u8::from(c.halted),
            m.peek(c.sp.wrapping_add(1)),
            m.peek(c.sp)
        ));
        ui.monospace(format!("Тактов всего: {}", m.cycles()));
        ui.horizontal(|ui| {
            ui.monospace(format!(
                "С отметки:    {}",
                m.cycles().saturating_sub(self.mark_cycles)
            ));
            if ui.small_button("отметка").clicked() {
                self.mark_cycles = m.cycles();
            }
        });
    }

    fn breakpoints_ui(&mut self, ui: &mut Ui) {
        ui.heading("Точки останова");
        ui.horizontal(|ui| {
            ui.label("адрес:");
            let r = ui.add(TextEdit::singleline(&mut self.bp_input).desired_width(50.0));
            if (ui.button("+").clicked()
                || (r.lost_focus() && ui.input(|i| i.key_pressed(KeyCode::Enter))))
                && let Ok(a) = parse_hex_u16(&self.bp_input)
                && !self.breakpoints.contains(&a)
            {
                self.breakpoints.push(a);
                self.breakpoints.sort();
            }
        });
        let mut remove = None;
        for (i, a) in self.breakpoints.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.monospace(format!("{a:04X}"));
                if ui.small_button("✖").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.breakpoints.remove(i);
        }
        ui.horizontal(|ui| {
            ui.label("порт:");
            ui.add(TextEdit::singleline(&mut self.port_input).desired_width(30.0));
            ComboBox::from_id_salt("portdir")
                .width(60.0)
                .selected_text(match self.port_dir {
                    PortDir::In => "IN",
                    PortDir::Out => "OUT",
                    PortDir::Any => "IN/OUT",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.port_dir, PortDir::Any, "IN/OUT");
                    ui.selectable_value(&mut self.port_dir, PortDir::In, "IN");
                    ui.selectable_value(&mut self.port_dir, PortDir::Out, "OUT");
                });
            if ui.button("+").clicked()
                && let Ok(p) = parse_hex_u16(&self.port_input)
                && p <= 0xFF
            {
                let e = (p as u8, self.port_dir);
                if !self.port_breaks.contains(&e) {
                    self.port_breaks.push(e);
                }
            }
        });
        let mut remove = None;
        for (i, (p, d)) in self.port_breaks.iter().enumerate() {
            ui.horizontal(|ui| {
                let d = match d {
                    PortDir::In => "IN",
                    PortDir::Out => "OUT",
                    PortDir::Any => "IN/OUT",
                };
                ui.monospace(format!("{d} {p:02X}H"));
                if ui.small_button("✖").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.port_breaks.remove(i);
        }
        ui.small("Порт проверяется у команд IN/OUT перед выполнением.");
    }

    fn disassembly(&mut self, ui: &mut Ui, m: &Machine) {
        ui.heading("Дизассемблер");
        let pc = m.cpu.pc;
        for (addr, len, text) in disasm_around(m, pc, 8, 18) {
            let bytes: String = (0..len)
                .map(|i| format!("{:02X} ", m.peek(addr.wrapping_add(i as u16))))
                .collect();
            let mark = if addr == pc { "►" } else { " " };
            let bp = if self.breakpoints.contains(&addr) {
                "●"
            } else {
                " "
            };
            let line = format!("{bp}{mark}{addr:04X}  {bytes:<9} {text}");
            let mut rt = RichText::new(line).monospace();
            if addr == pc {
                rt = rt.color(Color::from_rgb(0x60, 0xFF, 0x60)).strong();
            } else if self.breakpoints.contains(&addr) {
                rt = rt.color(Color::LIGHT_RED);
            }
            let r = ui.add(Label::new(rt).sense(Sense::click()));
            if r.on_hover_text("щелчок — точка останова").clicked() {
                if let Some(i) = self.breakpoints.iter().position(|&b| b == addr) {
                    self.breakpoints.remove(i);
                } else {
                    self.breakpoints.push(addr);
                    self.breakpoints.sort();
                }
            }
        }
    }

    fn memory(&mut self, ui: &mut Ui, m: &mut Machine) {
        ui.heading("Память");
        ui.horizontal(|ui| {
            ui.label("адрес:");
            let r = ui.add(TextEdit::singleline(&mut self.mem_addr_input).desired_width(50.0));
            if (r.lost_focus() || ui.button("Показать").clicked())
                && let Ok(a) = parse_hex_u16(&self.mem_addr_input)
            {
                self.mem_addr = a & 0xFFF0;
                self.follow_pc = false;
            }
            for (label, delta) in [("◀◀", -0x100i32), ("◀", -0x10), ("▶", 0x10), ("▶▶", 0x100)]
            {
                if ui.small_button(label).clicked() {
                    self.mem_addr = (i32::from(self.mem_addr) + delta) as u16 & 0xFFF0;
                    self.mem_addr_input = format!("{:04X}", self.mem_addr);
                }
            }
            for (label, a) in [("PC", m.cpu.pc), ("SP", m.cpu.sp), ("HL", m.cpu.hl())] {
                if ui.small_button(label).clicked() {
                    self.mem_addr = a & 0xFFF0;
                    self.mem_addr_input = format!("{:04X}", self.mem_addr);
                }
            }
        });
        let mut text = String::new();
        for row in 0..16u16 {
            let a = self.mem_addr.wrapping_add(row * 16);
            text.push_str(&format!("{a:04X}  "));
            for i in 0..16u16 {
                text.push_str(&format!("{:02X} ", m.peek(a.wrapping_add(i))));
                if i == 7 {
                    text.push(' ');
                }
            }
            text.push(' ');
            for i in 0..16u16 {
                let b = m.peek(a.wrapping_add(i)) & 0x7F;
                text.push(if b < 0x20 {
                    '.'
                } else {
                    ut88_core::screen::koi7_to_char(b)
                });
            }
            text.push('\n');
        }
        ui.label(RichText::new(text).monospace());
        ui.horizontal(|ui| {
            ui.label("Записать с адреса");
            ui.add(TextEdit::singleline(&mut self.edit_addr_input).desired_width(50.0));
            ui.label("байты:");
            ui.add(
                TextEdit::singleline(&mut self.edit_bytes_input)
                    .desired_width(260.0)
                    .hint_text("3E 55 C9"),
            );
            if ui.button("Записать").clicked() {
                match parse_hex_u16(&self.edit_addr_input).and_then(|a| {
                    let bytes = self
                        .edit_bytes_input
                        .split_whitespace()
                        .map(|b| {
                            u8::from_str_radix(b, 16)
                                .map_err(|_| anyhow::anyhow!("неверный байт «{b}»"))
                        })
                        .collect::<anyhow::Result<Vec<u8>>>()?;
                    Ok((a, bytes))
                }) {
                    Ok((a, bytes)) => {
                        m.load_bin(a, &bytes);
                        self.message = format!("записано {} байт с {a:04X}", bytes.len());
                    }
                    Err(e) => self.message = e.to_string(),
                }
            }
        });
    }
}

/// Строки дизассемблера вокруг `pc`: до `before` команд перед ним и `after` после.
/// Начало ищется так, чтобы цепочка команд попала точно на `pc`.
pub fn disasm_around(
    m: &Machine,
    pc: u16,
    before: usize,
    after: usize,
) -> Vec<(u16, usize, String)> {
    let one = |a: u16| {
        let bytes = [
            m.peek(a),
            m.peek(a.wrapping_add(1)),
            m.peek(a.wrapping_add(2)),
        ];
        let (t, l) = i8080::disasm(&bytes, a);
        (a, l, t)
    };
    // Подобрать начало: максимально далёкое, с которого цепочка приходит на pc.
    let mut start = pc;
    for back in (1..=(before as u16 * 3)).rev() {
        let s = pc.wrapping_sub(back);
        let mut a = s;
        let mut n = 0;
        while a != pc && n < before * 3 {
            let (_, l, _) = one(a);
            a = a.wrapping_add(l as u16);
            n += 1;
            if a.wrapping_sub(s) > back {
                break;
            }
        }
        if a == pc {
            start = s;
            break;
        }
    }
    let mut lines = Vec::new();
    let mut a = start;
    while a != pc {
        let l = one(a);
        a = a.wrapping_add(l.1 as u16);
        lines.push(l);
    }
    let skip = lines.len().saturating_sub(before);
    let mut out: Vec<_> = lines.into_iter().skip(skip).collect();
    let mut a = pc;
    for _ in 0..after {
        let l = one(a);
        a = a.wrapping_add(l.1 as u16);
        out.push(l);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ut88_core::Config;

    #[test]
    fn call_detection() {
        assert!(is_call(0xCD));
        assert!(is_call(0xC4));
        assert!(is_call(0xFF)); // RST 7
        assert!(is_call(0xDD)); // *CALL
        assert!(!is_call(0xC3));
        assert!(!is_call(0xC9));
    }

    #[test]
    fn disasm_lands_on_pc() {
        let m = Machine::new(Config::minimal());
        let lines = disasm_around(&m, 0x0005, 4, 3);
        let addrs: Vec<u16> = lines.iter().map(|l| l.0).collect();
        let i = addrs.iter().position(|&a| a == 0x0005).unwrap();
        assert_eq!(&addrs[i - 2..i], &[0x0000, 0x0003]);
        assert_eq!(addrs.len(), i + 3);
    }

    #[test]
    fn breakpoint_stops_and_resumes() {
        let mut m = Machine::new(Config::minimal());
        m.run_seconds(0.1);
        let mut d = Debugger::default();
        // Цикл ожидания клавиши МОНИТОРА (005FH–0065H).
        d.breakpoints.push(0x005F);
        let r = d.run(&mut m, 1_000_000);
        assert!(r.is_some());
        assert_eq!(m.cpu.pc, 0x005F);
        d.skip_once = true;
        let c = m.cycles();
        d.run(&mut m, 4);
        assert!(m.cycles() > c);
    }

    #[test]
    fn port_breakpoint() {
        let mut m = Machine::new(Config::minimal());
        let mut d = Debugger::default();
        d.port_breaks.push((0xA0, PortDir::In));
        let r = d.run(&mut m, 2_000_000).expect("останов на IN A0");
        assert!(r.contains("IN A0H"), "{r}");
        assert_eq!(m.peek(m.cpu.pc), 0xDB);
    }

    #[test]
    fn step_over_call() {
        let mut m = Machine::new(Config::minimal());
        m.load_bin(0xC000, &[0xCD, 0x10, 0xC0, 0x00]);
        m.load_bin(0xC010, &[0xC9]);
        m.cpu.sp = 0xC3F0;
        m.jump(0xC000);
        let mut d = Debugger::default();
        assert!(d.step_over(&mut m));
        d.run(&mut m, 1000);
        assert_eq!(m.cpu.pc, 0xC003);
        assert!(d.run_to.is_none());
    }
}
