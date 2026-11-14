//! Текст виджетов: измерение, рисование с подстановкой значков ([`Icon`]) и перенос
//! строк по словам.

use crate::gfx::{Align2, Canvas, Color, FontKind, Pos2, Rect, Vec2, measure_text_scaled, vec2};

use super::icon::Icon;

/// Кусок строки: текст шрифтом или значок.
enum Seg<'a> {
    Text(&'a str),
    Icon(Icon),
}

fn has_icons(s: &str) -> bool {
    s.chars().any(|c| Icon::from_char(c).is_some())
}

/// Разбить строку на куски текста и значки.
fn segments(s: &str) -> Vec<Seg<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if let Some(icon) = Icon::from_char(c) {
            if start < i {
                out.push(Seg::Text(&s[start..i]));
            }
            out.push(Seg::Icon(icon));
            start = i + c.len_utf8();
        }
    }
    if start < s.len() {
        out.push(Seg::Text(&s[start..]));
    }
    out
}

/// Размер строки (без переносов) в логических точках.
pub(crate) fn measure(text: &str, size: f32, font: FontKind, ppp: f32) -> Vec2 {
    if !has_icons(text) {
        return measure_text_scaled(text, size, font, ppp);
    }
    let h = measure_text_scaled("", size, font, ppp).y;
    let w = segments(text)
        .iter()
        .map(|s| match s {
            Seg::Text(t) => measure_text_scaled(t, size, font, ppp).x,
            Seg::Icon(i) => i.width_em() * size,
        })
        .sum();
    vec2(w, h)
}

/// Высота строки текста.
pub(crate) fn line_height(size: f32, font: FontKind, ppp: f32) -> f32 {
    measure_text_scaled("", size, font, ppp).y
}

/// Нарисовать строку, левый верхний угол — `pos`. Возвращает ширину.
pub(crate) fn paint(
    canvas: &mut Canvas,
    pos: Pos2,
    text: &str,
    size: f32,
    font: FontKind,
    color: Color,
) -> f32 {
    if !has_icons(text) {
        return canvas
            .text(pos, Align2::LEFT_TOP, text, size, font, color)
            .width();
    }
    let h = canvas.measure_text("", size, font).y;
    let mut x = pos.x;
    for s in segments(text) {
        match s {
            Seg::Text(t) => {
                x += canvas
                    .text(Pos2::new(x, pos.y), Align2::LEFT_TOP, t, size, font, color)
                    .width();
            }
            Seg::Icon(i) => {
                let w = i.width_em() * size;
                i.paint(
                    canvas,
                    Rect::from_min_size(Pos2::new(x, pos.y), vec2(w, h)),
                    size,
                    color,
                );
                x += w;
            }
        }
    }
    x - pos.x
}

/// Разбить текст на строки: по `'\n'` и по словам так, чтобы строка была не шире
/// `max_w` (слово длиннее `max_w` не разрывается).
pub(crate) fn wrap(text: &str, size: f32, font: FontKind, ppp: f32, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        if measure(para, size, font, ppp).x <= max_w {
            lines.push(para.to_owned());
            continue;
        }
        let mut line = String::new();
        for word in para.split(' ') {
            if line.is_empty() {
                line.push_str(word);
                continue;
            }
            let cand = format!("{line} {word}");
            if measure(&cand, size, font, ppp).x > max_w {
                lines.push(std::mem::replace(&mut line, word.to_owned()));
            } else {
                line = cand;
            }
        }
        lines.push(line);
    }
    lines
}

/// Обрезать строку до ширины `max_w`, заменив хвост многоточием «…» (строка, которая
/// помещается, возвращается как есть).
pub(crate) fn truncate(text: &str, size: f32, font: FontKind, ppp: f32, max_w: f32) -> String {
    if measure(text, size, font, ppp).x <= max_w {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    // Двоичный поиск наибольшего числа символов, которое помещается вместе с «…».
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let cand: String = chars[..mid].iter().collect::<String>() + "…";
        if measure(&cand, size, font, ppp).x <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let head: String = chars[..lo].iter().collect();
    format!("{}…", head.trim_end())
}

/// Байтовое смещение символа номер `ci` (или длина строки).
pub(crate) fn byte_of(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map_or(s.len(), |(b, _)| b)
}

/// X символа номер `ci` от начала строки.
pub(crate) fn char_x(s: &str, ci: usize, size: f32, font: FontKind, ppp: f32) -> f32 {
    measure(&s[..byte_of(s, ci)], size, font, ppp).x
}

/// Номер символа, ближайшего к `x` (от начала строки).
pub(crate) fn char_at(s: &str, x: f32, size: f32, font: FontKind, ppp: f32) -> usize {
    let n = s.chars().count();
    let mut prev = 0.0;
    for ci in 1..=n {
        let cx = char_x(s, ci, size, font, ppp);
        if x < (prev + cx) * 0.5 {
            return ci - 1;
        }
        prev = cx;
    }
    n
}
