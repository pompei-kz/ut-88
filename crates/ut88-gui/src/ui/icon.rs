//! Значки, которых нет в шрифтах DejaVu (`⏹ ⏮ ⏭ ⏸ ⏵ ⏴ 🔊 🔇`), — рисуются фигурами.
//!
//! Текст виджетов ([`super::text`]) сам подставляет эти значки вместо символов, поэтому
//! подписи вроде `"⏸ пауза"` можно писать как есть.

use crate::gfx::{Canvas, Color, Pos2, Rect, pos2};

/// Значок, рисуемый фигурами.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    /// `⏹` — стоп (квадрат).
    Stop,
    /// `⏮` — в начало.
    SkipBack,
    /// `⏭` — в конец.
    SkipForward,
    /// `⏸` — пауза.
    Pause,
    /// `⏵` — треугольник вправо.
    Right,
    /// `⏴` — треугольник влево.
    Left,
    /// `🔊` — громкоговоритель.
    Speaker,
    /// `🔇` — звук выключен.
    Mute,
}

impl Icon {
    /// Значок для символа, которого нет в шрифте.
    pub fn from_char(c: char) -> Option<Icon> {
        Some(match c {
            '⏹' => Icon::Stop,
            '⏮' => Icon::SkipBack,
            '⏭' => Icon::SkipForward,
            '⏸' => Icon::Pause,
            '⏵' => Icon::Right,
            '⏴' => Icon::Left,
            '🔊' => Icon::Speaker,
            '🔇' => Icon::Mute,
            _ => return None,
        })
    }

    /// Ширина значка в долях кегля.
    pub fn width_em(self) -> f32 {
        match self {
            Icon::Speaker | Icon::Mute => 1.15,
            _ => 0.9,
        }
    }

    /// Нарисовать значок в прямоугольнике `rect` (ширина — [`Icon::width_em`] × кегль,
    /// высота — строка текста) кеглем `size`.
    pub fn paint(self, canvas: &mut Canvas, rect: Rect, size: f32, color: Color) {
        let c = rect.center();
        let s = size;
        let p = |x: f32, y: f32| pos2(c.x + x * s, c.y + y * s);
        match self {
            Icon::Stop => {
                let r = Rect::from_center_size(c, crate::gfx::vec2(0.56 * s, 0.56 * s));
                canvas.fill_rect(r, 0.06 * s, color);
            }
            Icon::Pause => {
                for x in [-0.17, 0.17] {
                    let r = Rect::from_center_size(p(x, 0.0), crate::gfx::vec2(0.18 * s, 0.6 * s));
                    canvas.fill_rect(r, 0.0, color);
                }
            }
            Icon::Right => triangle(canvas, p(-0.22, -0.32), p(-0.22, 0.32), p(0.3, 0.0), color),
            Icon::Left => triangle(canvas, p(0.22, -0.32), p(0.22, 0.32), p(-0.3, 0.0), color),
            Icon::SkipBack => {
                let r = Rect::from_center_size(p(-0.3, 0.0), crate::gfx::vec2(0.12 * s, 0.6 * s));
                canvas.fill_rect(r, 0.0, color);
                triangle(canvas, p(0.32, -0.3), p(0.32, 0.3), p(-0.22, 0.0), color);
            }
            Icon::SkipForward => {
                let r = Rect::from_center_size(p(0.3, 0.0), crate::gfx::vec2(0.12 * s, 0.6 * s));
                canvas.fill_rect(r, 0.0, color);
                triangle(canvas, p(-0.32, -0.3), p(-0.32, 0.3), p(0.22, 0.0), color);
            }
            Icon::Speaker | Icon::Mute => {
                // Корпус громкоговорителя и раструб.
                let body = [
                    p(-0.48, -0.13),
                    p(-0.3, -0.13),
                    p(-0.06, -0.34),
                    p(-0.06, 0.34),
                    p(-0.3, 0.13),
                    p(-0.48, 0.13),
                ];
                canvas.fill_polygon(&body, color);
                let w = 0.08 * s;
                if self == Icon::Speaker {
                    for r in [0.2, 0.36] {
                        let pts: Vec<Pos2> = (0..=8)
                            .map(|i| {
                                let a = (-45.0 + 90.0 * i as f32 / 8.0f32).to_radians();
                                p(-0.04 + r * a.cos(), r * a.sin())
                            })
                            .collect();
                        canvas.polyline(&pts, w, color);
                    }
                } else {
                    canvas.line(p(0.08, -0.18), p(0.44, 0.18), w * 1.2, color);
                    canvas.line(p(0.08, 0.18), p(0.44, -0.18), w * 1.2, color);
                }
            }
        }
    }
}

fn triangle(canvas: &mut Canvas, a: Pos2, b: Pos2, c: Pos2, color: Color) {
    canvas.fill_polygon(&[a, b, c], color);
}
