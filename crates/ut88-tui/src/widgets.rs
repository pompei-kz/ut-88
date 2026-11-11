//! Виджеты: семисегментные индикаторы, подсказка клавиатуры модуля 1, экран 64×28.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use ut88_core::screen::{COLS, ROWS, koi7_to_char};
use ut88_core::{PadKey, Screen};

/// Ширина цифры, знакомест.
pub const DIGIT_W: u16 = 8;
/// Высота цифры, строк.
pub const DIGIT_H: u16 = 7;
/// Промежуток между цифрами.
pub const DIGIT_GAP: u16 = 2;
/// Дополнительный промежуток между адресом (4 цифры) и данными (2 цифры).
pub const GROUP_GAP: u16 = 3;
/// Ширина ряда индикаторов.
pub const INDICATORS_W: u16 = 6 * DIGIT_W + 5 * DIGIT_GAP + GROUP_GAP;

/// Цвет светящегося сегмента.
const LIT: Color = Color::Rgb(255, 40, 30);
/// Цвет погашенного сегмента.
const DARK: Color = Color::Rgb(60, 12, 10);

/// Горит ли точка `(x, y)` цифры с сегментами `seg` (бит 0 — a, …, бит 6 — g).
///
/// Цифра 8 × 7: строки 0, 3, 6 — горизонтальные сегменты a, g, d; столбцы 0 и 7 —
/// вертикальные f/e и b/c. Углы горят, если горит любой примыкающий сегмент.
/// Возвращает `None`, если точка не принадлежит ни одному сегменту.
pub fn segment_pixel(seg: u8, x: u16, y: u16) -> Option<bool> {
    let on = |i: u8| seg >> i & 1 != 0;
    let (a, b, c, d, e, f, g) = (on(0), on(1), on(2), on(3), on(4), on(5), on(6));
    let right = DIGIT_W - 1;
    let edge_x = x == 0 || x == right;
    let row = match y {
        0 => Some((a, f, b)),
        3 => Some((g, f || e, b || c)),
        6 => Some((d, e, c)),
        _ => None,
    };
    if let Some((h, left_v, right_v)) = row {
        return Some(match x {
            0 => h || left_v,
            x if x == right => h || right_v,
            _ => h,
        });
    }
    if !edge_x {
        return None;
    }
    let upper = y < 3;
    Some(match (x == 0, upper) {
        (true, true) => f,
        (true, false) => e,
        (false, true) => b,
        (false, false) => c,
    })
}

/// Шесть индикаторов модуля 1 крупными цифрами.
pub struct Indicators {
    /// Сегменты разрядов слева направо.
    pub segments: [u8; 6],
}

impl Widget for Indicators {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, &seg) in self.segments.iter().enumerate() {
            let i = i as u16;
            let x0 = area.x + i * (DIGIT_W + DIGIT_GAP) + if i >= 4 { GROUP_GAP } else { 0 };
            for y in 0..DIGIT_H {
                for x in 0..DIGIT_W {
                    let (px, py) = (x0 + x, area.y + y);
                    if px >= area.right() || py >= area.bottom() {
                        continue;
                    }
                    let Some(lit) = segment_pixel(seg, x, y) else {
                        continue;
                    };
                    if let Some(cell) = buf.cell_mut((px, py)) {
                        if lit {
                            cell.set_symbol("█").set_fg(LIT);
                        } else {
                            // Погашенный сегмент виден тусклым, как на настоящем АЛС.
                            cell.set_symbol(" ").set_fg(DARK);
                        }
                    }
                }
            }
        }
    }
}

/// Раскладка клавиатуры модуля 1 на подсказке (строки сверху вниз).
pub const PAD_ROWS: [[u8; 4]; 4] = [
    [0xC, 0xD, 0xE, 0xF],
    [8, 9, 0xA, 0xB],
    [4, 5, 6, 7],
    [0, 1, 2, 3],
];

/// Подсказка клавиатуры модуля 1 с соответствием клавишам ПК; `pressed` подсвечивается.
pub fn pad_help(pressed: Option<PadKey>) -> Paragraph<'static> {
    let hi = Style::default().fg(Color::Black).bg(LIT);
    let key = |label: String, k: PadKey| {
        let st = if pressed == Some(k) {
            hi
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        Span::styled(label, st)
    };
    let notes = [
        "Клавиши ПК:",
        "  0–9, A–F  — цифры",
        "  Backspace — ШАГ НАЗАД",
        "  F2        — УСТ",
    ];
    let mut lines = Vec::new();
    lines.push(Line::from("┌───┬───┬───┬───┐"));
    for (r, row) in PAD_ROWS.iter().enumerate() {
        let mut spans = vec![Span::raw("│")];
        for &d in row {
            spans.push(key(format!(" {d:X} "), PadKey::Hex(d)));
            spans.push(Span::raw("│"));
        }
        spans.push(Span::raw(format!("    {}", notes[r])));
        lines.push(Line::from(spans));
        if r < 3 {
            lines.push(Line::from("├───┼───┼───┼───┤"));
        }
    }
    lines.push(Line::from("└───┴───┴───┴───┘"));
    lines.push(Line::from(vec![
        key("[ШАГ НАЗАД]".into(), PadKey::StepBack),
        Span::raw(" "),
        key("[ УСТ ]".into(), PadKey::Reset),
    ]));
    Paragraph::new(lines)
}

/// Экран дисплейного модуля 64 × 28 текстом.
pub struct ScreenView<'a> {
    /// Снимок экрана.
    pub screen: &'a Screen,
}

impl Widget for ScreenView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let normal = Style::default().fg(Color::White).bg(Color::Black);
        let inverse = Style::default().fg(Color::Black).bg(Color::White);
        for row in 0..ROWS.min(area.height as usize) {
            for col in 0..COLS.min(area.width as usize) {
                let i = row * COLS + col;
                let ch = koi7_to_char(self.screen.codes[i]);
                let st = if self.screen.inverse[i] {
                    inverse
                } else {
                    normal
                };
                if let Some(cell) = buf.cell_mut((area.x + col as u16, area.y + row as u16)) {
                    let mut tmp = [0u8; 4];
                    cell.set_symbol(ch.encode_utf8(&mut tmp)).set_style(st);
                }
            }
        }
    }
}

/// Текст буфера построчно (для тестов и отладки).
pub fn buffer_text(buf: &Buffer) -> String {
    let a = buf.area;
    (a.top()..a.bottom())
        .map(|y| {
            (a.left()..a.right())
                .map(|x| buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digit(seg: u8) -> Vec<String> {
        (0..DIGIT_H)
            .map(|y| {
                (0..DIGIT_W)
                    .map(|x| match segment_pixel(seg, x, y) {
                        Some(true) => '#',
                        _ => '.',
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn seven_segment_shapes() {
        // «1» — сегменты b, c.
        assert_eq!(
            digit(0x06),
            [
                ".......#", ".......#", ".......#", ".......#", ".......#", ".......#", ".......#"
            ]
        );
        // «8» — все.
        let eight = digit(0x7F);
        assert_eq!(eight[0], "########");
        assert_eq!(eight[1], "#......#");
        assert_eq!(eight[3], "########");
        assert_eq!(eight[6], "########");
        // «7» — a, b, c.
        let seven = digit(0x07);
        assert_eq!(seven[0], "########");
        assert_eq!(seven[4], ".......#");
        assert_eq!(seven[6], ".......#");
        // Средний сегмент g не связан с a.
        assert_eq!(digit(0x40)[0], "........");
        assert_eq!(digit(0x40)[3], "########");
    }
}
