//! Геометрия: точки, векторы, прямоугольники и выравнивание.
//!
//! Имена и смысл повторяют `egui::{Pos2, Vec2, Rect, Align2}`, чтобы перенос рисунка и
//! интерфейса с egui был механическим. Все величины — в логических точках (до умножения
//! на масштаб HiDPI).

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

/// Точка на плоскости.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Pos2 {
    /// Координата X (вправо).
    pub x: f32,
    /// Координата Y (вниз).
    pub y: f32,
}

/// Вектор (смещение или размер).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec2 {
    /// Составляющая X.
    pub x: f32,
    /// Составляющая Y.
    pub y: f32,
}

/// Точка `(x, y)`.
#[inline]
pub const fn pos2(x: f32, y: f32) -> Pos2 {
    Pos2 { x, y }
}

/// Вектор `(x, y)`.
#[inline]
pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

impl Pos2 {
    /// Начало координат.
    pub const ZERO: Pos2 = pos2(0.0, 0.0);

    /// Точка `(x, y)`.
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        pos2(x, y)
    }

    /// Радиус-вектор точки.
    #[inline]
    pub fn to_vec2(self) -> Vec2 {
        vec2(self.x, self.y)
    }

    /// Расстояние до другой точки.
    #[inline]
    pub fn distance(self, other: Pos2) -> f32 {
        (self - other).length()
    }

    /// Покомпонентный минимум.
    #[inline]
    pub fn min(self, other: Pos2) -> Pos2 {
        pos2(self.x.min(other.x), self.y.min(other.y))
    }

    /// Покомпонентный максимум.
    #[inline]
    pub fn max(self, other: Pos2) -> Pos2 {
        pos2(self.x.max(other.x), self.y.max(other.y))
    }

    /// Покомпонентное ограничение диапазоном `[min, max]`.
    #[inline]
    pub fn clamp(self, min: Pos2, max: Pos2) -> Pos2 {
        pos2(self.x.clamp(min.x, max.x), self.y.clamp(min.y, max.y))
    }

    /// Линейная интерполяция: `t = 0` — `self`, `t = 1` — `other`.
    #[inline]
    pub fn lerp(self, other: Pos2, t: f32) -> Pos2 {
        pos2(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }

    /// Округление до целых.
    #[inline]
    pub fn round(self) -> Pos2 {
        pos2(self.x.round(), self.y.round())
    }
}

impl Vec2 {
    /// Нулевой вектор.
    pub const ZERO: Vec2 = vec2(0.0, 0.0);
    /// Единичный вектор по X.
    pub const X: Vec2 = vec2(1.0, 0.0);
    /// Единичный вектор по Y.
    pub const Y: Vec2 = vec2(0.0, 1.0);

    /// Вектор `(x, y)`.
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        vec2(x, y)
    }

    /// Вектор `(v, v)`.
    #[inline]
    pub const fn splat(v: f32) -> Self {
        vec2(v, v)
    }

    /// Единичный вектор под углом `angle` радиан от оси X (по часовой стрелке, ось Y вниз).
    #[inline]
    pub fn angled(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        vec2(c, s)
    }

    /// Точка с такими координатами.
    #[inline]
    pub fn to_pos2(self) -> Pos2 {
        pos2(self.x, self.y)
    }

    /// Длина.
    #[inline]
    pub fn length(self) -> f32 {
        self.x.hypot(self.y)
    }

    /// Квадрат длины.
    #[inline]
    pub fn length_sq(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    /// Единичный вектор того же направления (нулевой остаётся нулевым).
    #[inline]
    pub fn normalized(self) -> Vec2 {
        let len = self.length();
        if len > 0.0 { self / len } else { self }
    }

    /// Поворот на 90° (в экранных координатах — по часовой стрелке).
    #[inline]
    pub fn rot90(self) -> Vec2 {
        vec2(-self.y, self.x)
    }

    /// Скалярное произведение.
    #[inline]
    pub fn dot(self, other: Vec2) -> f32 {
        self.x * other.x + self.y * other.y
    }

    /// Покомпонентный минимум.
    #[inline]
    pub fn min(self, other: Vec2) -> Vec2 {
        vec2(self.x.min(other.x), self.y.min(other.y))
    }

    /// Покомпонентный максимум.
    #[inline]
    pub fn max(self, other: Vec2) -> Vec2 {
        vec2(self.x.max(other.x), self.y.max(other.y))
    }

    /// Меньшая из составляющих.
    #[inline]
    pub fn min_elem(self) -> f32 {
        self.x.min(self.y)
    }

    /// Большая из составляющих.
    #[inline]
    pub fn max_elem(self) -> f32 {
        self.x.max(self.y)
    }
}

impl Add<Vec2> for Pos2 {
    type Output = Pos2;
    #[inline]
    fn add(self, v: Vec2) -> Pos2 {
        pos2(self.x + v.x, self.y + v.y)
    }
}

impl Sub<Vec2> for Pos2 {
    type Output = Pos2;
    #[inline]
    fn sub(self, v: Vec2) -> Pos2 {
        pos2(self.x - v.x, self.y - v.y)
    }
}

impl Sub for Pos2 {
    type Output = Vec2;
    #[inline]
    fn sub(self, p: Pos2) -> Vec2 {
        vec2(self.x - p.x, self.y - p.y)
    }
}

impl AddAssign<Vec2> for Pos2 {
    #[inline]
    fn add_assign(&mut self, v: Vec2) {
        *self = *self + v;
    }
}

impl SubAssign<Vec2> for Pos2 {
    #[inline]
    fn sub_assign(&mut self, v: Vec2) {
        *self = *self - v;
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    #[inline]
    fn add(self, v: Vec2) -> Vec2 {
        vec2(self.x + v.x, self.y + v.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    #[inline]
    fn sub(self, v: Vec2) -> Vec2 {
        vec2(self.x - v.x, self.y - v.y)
    }
}

impl AddAssign for Vec2 {
    #[inline]
    fn add_assign(&mut self, v: Vec2) {
        *self = *self + v;
    }
}

impl SubAssign for Vec2 {
    #[inline]
    fn sub_assign(&mut self, v: Vec2) {
        *self = *self - v;
    }
}

impl Neg for Vec2 {
    type Output = Vec2;
    #[inline]
    fn neg(self) -> Vec2 {
        vec2(-self.x, -self.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Vec2;
    #[inline]
    fn mul(self, k: f32) -> Vec2 {
        vec2(self.x * k, self.y * k)
    }
}

impl Mul<Vec2> for f32 {
    type Output = Vec2;
    #[inline]
    fn mul(self, v: Vec2) -> Vec2 {
        v * self
    }
}

/// Покомпонентное произведение.
impl Mul for Vec2 {
    type Output = Vec2;
    #[inline]
    fn mul(self, v: Vec2) -> Vec2 {
        vec2(self.x * v.x, self.y * v.y)
    }
}

impl MulAssign<f32> for Vec2 {
    #[inline]
    fn mul_assign(&mut self, k: f32) {
        *self = *self * k;
    }
}

impl Div<f32> for Vec2 {
    type Output = Vec2;
    #[inline]
    fn div(self, k: f32) -> Vec2 {
        vec2(self.x / k, self.y / k)
    }
}

impl DivAssign<f32> for Vec2 {
    #[inline]
    fn div_assign(&mut self, k: f32) {
        *self = *self / k;
    }
}

/// Прямоугольник со сторонами вдоль осей: `min` — левый верхний угол, `max` — правый
/// нижний. Правая и нижняя границы не входят в прямоугольник (`contains`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    /// Левый верхний угол.
    pub min: Pos2,
    /// Правый нижний угол.
    pub max: Pos2,
}

impl Rect {
    /// Пустой «вывернутый» прямоугольник: объединение с ним даёт второй операнд.
    pub const NOTHING: Rect = Rect {
        min: pos2(f32::INFINITY, f32::INFINITY),
        max: pos2(f32::NEG_INFINITY, f32::NEG_INFINITY),
    };
    /// Вся плоскость.
    pub const EVERYTHING: Rect = Rect {
        min: pos2(f32::NEG_INFINITY, f32::NEG_INFINITY),
        max: pos2(f32::INFINITY, f32::INFINITY),
    };

    /// По двум углам.
    #[inline]
    pub const fn from_min_max(min: Pos2, max: Pos2) -> Self {
        Rect { min, max }
    }

    /// По левому верхнему углу и размеру.
    #[inline]
    pub fn from_min_size(min: Pos2, size: Vec2) -> Self {
        Rect {
            min,
            max: min + size,
        }
    }

    /// По центру и размеру.
    #[inline]
    pub fn from_center_size(center: Pos2, size: Vec2) -> Self {
        Rect {
            min: center - size * 0.5,
            max: center + size * 0.5,
        }
    }

    /// По двум любым противоположным углам.
    #[inline]
    pub fn from_two_pos(a: Pos2, b: Pos2) -> Self {
        Rect {
            min: a.min(b),
            max: a.max(b),
        }
    }

    /// Ширина.
    #[inline]
    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    /// Высота.
    #[inline]
    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    /// Размер.
    #[inline]
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    /// Площадь.
    #[inline]
    pub fn area(&self) -> f32 {
        self.width() * self.height()
    }

    /// Центр.
    #[inline]
    pub fn center(&self) -> Pos2 {
        pos2(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
        )
    }

    /// Левая граница.
    #[inline]
    pub fn left(&self) -> f32 {
        self.min.x
    }

    /// Правая граница.
    #[inline]
    pub fn right(&self) -> f32 {
        self.max.x
    }

    /// Верхняя граница.
    #[inline]
    pub fn top(&self) -> f32 {
        self.min.y
    }

    /// Нижняя граница.
    #[inline]
    pub fn bottom(&self) -> f32 {
        self.max.y
    }

    /// Левый верхний угол.
    #[inline]
    pub fn left_top(&self) -> Pos2 {
        self.min
    }

    /// Правый верхний угол.
    #[inline]
    pub fn right_top(&self) -> Pos2 {
        pos2(self.max.x, self.min.y)
    }

    /// Левый нижний угол.
    #[inline]
    pub fn left_bottom(&self) -> Pos2 {
        pos2(self.min.x, self.max.y)
    }

    /// Правый нижний угол.
    #[inline]
    pub fn right_bottom(&self) -> Pos2 {
        self.max
    }

    /// Середина верхней стороны.
    #[inline]
    pub fn center_top(&self) -> Pos2 {
        pos2(self.center().x, self.min.y)
    }

    /// Середина нижней стороны.
    #[inline]
    pub fn center_bottom(&self) -> Pos2 {
        pos2(self.center().x, self.max.y)
    }

    /// Середина левой стороны.
    #[inline]
    pub fn left_center(&self) -> Pos2 {
        pos2(self.min.x, self.center().y)
    }

    /// Середина правой стороны.
    #[inline]
    pub fn right_center(&self) -> Pos2 {
        pos2(self.max.x, self.center().y)
    }

    /// Точка внутри: `min <= p < max`.
    #[inline]
    pub fn contains(&self, p: Pos2) -> bool {
        self.min.x <= p.x && p.x < self.max.x && self.min.y <= p.y && p.y < self.max.y
    }

    /// Прямоугольник `other` целиком внутри этого.
    #[inline]
    pub fn contains_rect(&self, other: Rect) -> bool {
        self.min.x <= other.min.x
            && self.min.y <= other.min.y
            && other.max.x <= self.max.x
            && other.max.y <= self.max.y
    }

    /// Ширина и высота положительны.
    #[inline]
    pub fn is_positive(&self) -> bool {
        self.min.x < self.max.x && self.min.y < self.max.y
    }

    /// Прямоугольники пересекаются (с ненулевой площадью).
    #[inline]
    pub fn intersects(&self, other: Rect) -> bool {
        self.intersect(other).is_positive()
    }

    /// Уменьшить на `d` с каждой стороны.
    #[inline]
    pub fn shrink(&self, d: f32) -> Rect {
        self.shrink2(Vec2::splat(d))
    }

    /// Уменьшить на `d.x` слева и справа и на `d.y` сверху и снизу.
    #[inline]
    pub fn shrink2(&self, d: Vec2) -> Rect {
        Rect {
            min: self.min + d,
            max: self.max - d,
        }
    }

    /// Увеличить на `d` с каждой стороны.
    #[inline]
    pub fn expand(&self, d: f32) -> Rect {
        self.shrink(-d)
    }

    /// Увеличить на `d.x` слева и справа и на `d.y` сверху и снизу.
    #[inline]
    pub fn expand2(&self, d: Vec2) -> Rect {
        self.shrink2(-d)
    }

    /// Сдвинуть на `v`.
    #[inline]
    pub fn translate(&self, v: Vec2) -> Rect {
        Rect {
            min: self.min + v,
            max: self.max + v,
        }
    }

    /// Пересечение (может оказаться не положительным — см. [`Rect::is_positive`]).
    #[inline]
    pub fn intersect(&self, other: Rect) -> Rect {
        Rect {
            min: self.min.max(other.min),
            max: self.max.min(other.max),
        }
    }

    /// Наименьший прямоугольник, содержащий оба.
    #[inline]
    pub fn union(&self, other: Rect) -> Rect {
        Rect {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// Ближайшая к `p` точка прямоугольника.
    #[inline]
    pub fn clamp(&self, p: Pos2) -> Pos2 {
        p.clamp(self.min, self.max)
    }

    /// Точка по долям ширины и высоты: `(0, 0)` — `min`, `(1, 1)` — `max`.
    #[inline]
    pub fn lerp_inside(&self, t: Vec2) -> Pos2 {
        pos2(
            self.min.x + self.width() * t.x,
            self.min.y + self.height() * t.y,
        )
    }

    /// Тот же прямоугольник с другой левой границей.
    #[inline]
    pub fn with_min_x(mut self, x: f32) -> Rect {
        self.min.x = x;
        self
    }

    /// Тот же прямоугольник с другой правой границей.
    #[inline]
    pub fn with_max_x(mut self, x: f32) -> Rect {
        self.max.x = x;
        self
    }

    /// Тот же прямоугольник с другой верхней границей.
    #[inline]
    pub fn with_min_y(mut self, y: f32) -> Rect {
        self.min.y = y;
        self
    }

    /// Тот же прямоугольник с другой нижней границей.
    #[inline]
    pub fn with_max_y(mut self, y: f32) -> Rect {
        self.max.y = y;
        self
    }
}

/// Выравнивание по одной оси.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    /// К левому (верхнему) краю.
    Min,
    /// По центру.
    Center,
    /// К правому (нижнему) краю.
    Max,
}

impl Align {
    /// Доля: 0 — `Min`, 0,5 — `Center`, 1 — `Max`.
    #[inline]
    pub fn to_factor(self) -> f32 {
        match self {
            Align::Min => 0.0,
            Align::Center => 0.5,
            Align::Max => 1.0,
        }
    }
}

/// Выравнивание по двум осям: какая точка прямоугольника привязывается к опорной точке.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Align2 {
    /// По горизонтали.
    pub x: Align,
    /// По вертикали.
    pub y: Align,
}

impl Align2 {
    /// Левый верхний угол.
    pub const LEFT_TOP: Align2 = Align2 {
        x: Align::Min,
        y: Align::Min,
    };
    /// Середина левой стороны.
    pub const LEFT_CENTER: Align2 = Align2 {
        x: Align::Min,
        y: Align::Center,
    };
    /// Левый нижний угол.
    pub const LEFT_BOTTOM: Align2 = Align2 {
        x: Align::Min,
        y: Align::Max,
    };
    /// Середина верхней стороны.
    pub const CENTER_TOP: Align2 = Align2 {
        x: Align::Center,
        y: Align::Min,
    };
    /// Центр.
    pub const CENTER_CENTER: Align2 = Align2 {
        x: Align::Center,
        y: Align::Center,
    };
    /// Середина нижней стороны.
    pub const CENTER_BOTTOM: Align2 = Align2 {
        x: Align::Center,
        y: Align::Max,
    };
    /// Правый верхний угол.
    pub const RIGHT_TOP: Align2 = Align2 {
        x: Align::Max,
        y: Align::Min,
    };
    /// Середина правой стороны.
    pub const RIGHT_CENTER: Align2 = Align2 {
        x: Align::Max,
        y: Align::Center,
    };
    /// Правый нижний угол.
    pub const RIGHT_BOTTOM: Align2 = Align2 {
        x: Align::Max,
        y: Align::Max,
    };

    /// Прямоугольник размера `size`, у которого точка выравнивания совпадает с `pos`
    /// (как `egui::Align2::anchor_size`).
    #[inline]
    pub fn anchor_size(self, pos: Pos2, size: Vec2) -> Rect {
        let min = pos2(
            pos.x - size.x * self.x.to_factor(),
            pos.y - size.y * self.y.to_factor(),
        );
        Rect::from_min_size(min, size)
    }

    /// Точка выравнивания внутри прямоугольника `rect`.
    #[inline]
    pub fn pos_in_rect(self, rect: &Rect) -> Pos2 {
        rect.lerp_inside(vec2(self.x.to_factor(), self.y.to_factor()))
    }

    /// Разместить прямоугольник размера `size` внутри `frame` по этому выравниванию.
    #[inline]
    pub fn align_size_within_rect(self, size: Vec2, frame: Rect) -> Rect {
        let free = frame.size() - size;
        Rect::from_min_size(
            frame.min + vec2(free.x * self.x.to_factor(), free.y * self.y.to_factor()),
            size,
        )
    }
}
