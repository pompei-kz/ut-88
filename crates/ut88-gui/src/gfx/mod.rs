//! Программная графика окна: холст [`Canvas`] поверх tiny-skia, геометрия
//! ([`Pos2`], [`Vec2`], [`Rect`], [`Align2`]), цвет [`Color`] и текст вшитыми шрифтами
//! DejaVu ([`FontKind`]).
//!
//! Кадр рисуется в памяти и переводится в формат softbuffer ([`Canvas::to_0rgb`]).
//! Все координаты и размеры в операциях рисования — логические точки; масштаб HiDPI
//! задаёт [`Canvas::set_scale`].

mod canvas;
mod color;
mod geom;
mod text;

#[cfg(test)]
mod tests;

pub use canvas::{Canvas, measure_text_scaled};
pub use color::Color;
pub use geom::{Align, Align2, Pos2, Rect, Vec2, pos2, vec2};
pub use text::{FontKind, missing_glyphs};
