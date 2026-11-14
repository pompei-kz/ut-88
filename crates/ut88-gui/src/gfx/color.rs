//! Цвет.

/// Цвет sRGB с прозрачностью **без** предварительного умножения на альфу.
///
/// Аналог `egui::Color32`; конструкторы названы так же (`from_rgb`, `from_gray`,
/// `from_black_alpha`, …), значения констант совпадают с egui. В отличие от egui цвет
/// хранится без умножения на альфу — умножение делается при рисовании.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Color {
    /// Красный.
    pub r: u8,
    /// Зелёный.
    pub g: u8,
    /// Синий.
    pub b: u8,
    /// Непрозрачность: 0 — прозрачный, 255 — непрозрачный.
    pub a: u8,
}

impl Color {
    /// Полностью прозрачный.
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);
    /// Чёрный.
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    /// Белый.
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    /// Тёмно-серый.
    pub const DARK_GRAY: Color = Color::rgb(96, 96, 96);
    /// Серый.
    pub const GRAY: Color = Color::rgb(160, 160, 160);
    /// Светло-серый.
    pub const LIGHT_GRAY: Color = Color::rgb(220, 220, 220);
    /// Красный.
    pub const RED: Color = Color::rgb(255, 0, 0);
    /// Тёмно-красный.
    pub const DARK_RED: Color = Color::rgb(0x8B, 0, 0);
    /// Светло-красный.
    pub const LIGHT_RED: Color = Color::rgb(255, 128, 128);
    /// Жёлтый.
    pub const YELLOW: Color = Color::rgb(255, 255, 0);
    /// Светло-жёлтый.
    pub const LIGHT_YELLOW: Color = Color::rgb(255, 255, 0xE0);
    /// Оранжевый.
    pub const ORANGE: Color = Color::rgb(255, 165, 0);
    /// Зелёный.
    pub const GREEN: Color = Color::rgb(0, 255, 0);
    /// Тёмно-зелёный.
    pub const DARK_GREEN: Color = Color::rgb(0, 0x64, 0);
    /// Светло-зелёный.
    pub const LIGHT_GREEN: Color = Color::rgb(0x90, 0xEE, 0x90);
    /// Синий.
    pub const BLUE: Color = Color::rgb(0, 0, 255);
    /// Тёмно-синий.
    pub const DARK_BLUE: Color = Color::rgb(0, 0, 0x8B);
    /// Светло-синий.
    pub const LIGHT_BLUE: Color = Color::rgb(0xAD, 0xD8, 0xE6);

    /// Непрозрачный цвет.
    #[inline]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    /// Цвет с прозрачностью (без умножения на альфу).
    #[inline]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    /// То же, что [`Color::rgb`] (имя из egui).
    #[inline]
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Color::rgb(r, g, b)
    }

    /// То же, что [`Color::rgba`] (имя из egui).
    #[inline]
    pub const fn from_rgba_unmultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color::rgba(r, g, b, a)
    }

    /// Непрозрачный серый.
    #[inline]
    pub const fn from_gray(l: u8) -> Self {
        Color::rgb(l, l, l)
    }

    /// Полупрозрачный чёрный.
    #[inline]
    pub const fn from_black_alpha(a: u8) -> Self {
        Color::rgba(0, 0, 0, a)
    }

    /// Полупрозрачный белый.
    #[inline]
    pub const fn from_white_alpha(a: u8) -> Self {
        Color::rgba(255, 255, 255, a)
    }

    /// Красный.
    #[inline]
    pub const fn r(&self) -> u8 {
        self.r
    }

    /// Зелёный.
    #[inline]
    pub const fn g(&self) -> u8 {
        self.g
    }

    /// Синий.
    #[inline]
    pub const fn b(&self) -> u8 {
        self.b
    }

    /// Непрозрачность.
    #[inline]
    pub const fn a(&self) -> u8 {
        self.a
    }

    /// Тот же цвет с другой непрозрачностью.
    #[inline]
    pub const fn with_alpha(self, a: u8) -> Self {
        Color::rgba(self.r, self.g, self.b, a)
    }

    /// Умножить непрозрачность на `k` (0..=1); аналог `egui::Color32::gamma_multiply`.
    #[inline]
    pub fn gamma_multiply(self, k: f32) -> Self {
        let a = (self.a as f32 * k.clamp(0.0, 1.0)).round() as u8;
        self.with_alpha(a)
    }

    /// Полностью прозрачный.
    #[inline]
    pub const fn is_transparent(&self) -> bool {
        self.a == 0
    }

    /// Цвет для tiny-skia.
    #[inline]
    pub(crate) fn to_skia(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.r, self.g, self.b, self.a)
    }
}
