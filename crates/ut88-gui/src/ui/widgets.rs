//! Простые виджеты: подпись, кнопка, флажок, переключатель, выбираемая подпись,
//! ползунок, разделитель.

use std::ops::RangeInclusive;

use crate::gfx::{Color, Pos2, Rect, Vec2, pos2, vec2};

use super::layout::Ui;
use super::registry::WidgetKind;
use super::text;
use super::{KeyCode, Response, RichText, Sense, Shape, WidgetVisuals};

/// Виджет, который можно добавить через [`Ui::add`].
pub trait Widget {
    /// Разместить, обработать ввод, нарисовать.
    fn ui(self, ui: &mut Ui) -> Response;
}

/// Цвета виджета, приглушённые в `k` раз (недоступный виджет).
fn dim(v: WidgetVisuals, k: f32) -> WidgetVisuals {
    WidgetVisuals {
        bg_fill: v.bg_fill.gamma_multiply(k),
        weak_bg_fill: v.weak_bg_fill.gamma_multiply(k),
        bg_stroke: (v.bg_stroke.0, v.bg_stroke.1.gamma_multiply(k)),
        fg: v.fg.gamma_multiply(k),
        ..v
    }
}

impl Ui {
    /// Вид виджета по его состоянию.
    pub(crate) fn style_of(&self, r: &Response) -> WidgetVisuals {
        let t = &self.theme;
        if !r.enabled {
            return dim(t.inactive, t.disabled_alpha);
        }
        let down = r.interact_pointer_pos.is_some() && self.input(|i| i.primary_down());
        if down || r.dragged {
            t.active
        } else if r.hovered || r.has_focus {
            t.hovered
        } else {
            t.inactive
        }
    }

    /// Приглушить цвет, если область недоступна.
    pub(crate) fn fade(&self, c: Color) -> Color {
        if self.is_enabled() {
            c
        } else {
            c.gamma_multiply(self.theme.disabled_alpha)
        }
    }

    /// Нарисовать строку (значки подставляются).
    pub(crate) fn paint_text(
        &self,
        pos: Pos2,
        s: &str,
        size: f32,
        font: crate::gfx::FontKind,
        color: Color,
    ) {
        self.paint(Shape::Text {
            pos,
            text: s.to_owned(),
            size,
            font,
            color,
        });
    }

    /// Размер строки текста.
    pub(crate) fn text_size(&self, rt: &RichText) -> Vec2 {
        let (size, font) = rt.font(&self.theme);
        text::measure(rt.text(), size, font, self.ppp)
    }

    // ---------------------------------------------------------------- добавление

    /// Добавить виджет.
    pub fn add(&mut self, w: impl Widget) -> Response {
        w.ui(self)
    }

    /// Добавить виджет, доступный только при `enabled`.
    pub fn add_enabled(&mut self, enabled: bool, w: impl Widget) -> Response {
        let old = self.enabled;
        self.enabled = old && enabled;
        let r = w.ui(self);
        self.enabled = old;
        r
    }

    /// Подпись.
    pub fn label(&mut self, text: impl Into<RichText>) -> Response {
        Label::new(text).ui(self)
    }

    /// Цветная подпись.
    pub fn colored_label(&mut self, color: Color, text: impl Into<RichText>) -> Response {
        Label::new(text.into().color(color)).ui(self)
    }

    /// Моноширинная подпись.
    pub fn monospace(&mut self, text: impl Into<RichText>) -> Response {
        Label::new(text.into().monospace()).ui(self)
    }

    /// Выделенная (яркая) подпись.
    pub fn strong(&mut self, text: impl Into<RichText>) -> Response {
        Label::new(text.into().strong()).ui(self)
    }

    /// Заголовок.
    pub fn heading(&mut self, text: impl Into<RichText>) -> Response {
        Label::new(text.into().heading()).ui(self)
    }

    /// Мелкая подпись.
    pub fn small(&mut self, text: impl Into<RichText>) -> Response {
        Label::new(text.into().small()).ui(self)
    }

    /// Кнопка.
    pub fn button(&mut self, text: impl Into<RichText>) -> Response {
        Button::new(text).ui(self)
    }

    /// Маленькая кнопка (без минимальной высоты).
    pub fn small_button(&mut self, text: impl Into<RichText>) -> Response {
        Button::new(text).small().ui(self)
    }

    /// Флажок.
    pub fn checkbox(&mut self, checked: &mut bool, text: impl Into<RichText>) -> Response {
        Checkbox::new(checked, text).ui(self)
    }

    /// Переключатель: `selected` — выбран ли; щелчок сообщается в ответе.
    pub fn radio(&mut self, selected: bool, text: impl Into<RichText>) -> Response {
        RadioButton::new(selected, text).ui(self)
    }

    /// Переключатель значения: щелчок присваивает `*current = alternative`.
    pub fn radio_value<T: PartialEq>(
        &mut self,
        current: &mut T,
        alternative: T,
        text: impl Into<RichText>,
    ) -> Response {
        let mut r = self.radio(*current == alternative, text);
        if r.clicked() && *current != alternative {
            *current = alternative;
            r.mark_changed();
        }
        r
    }

    /// Выбираемая подпись (пункт списка).
    pub fn selectable_label(&mut self, selected: bool, text: impl Into<RichText>) -> Response {
        let r = Button::new(text)
            .selected(selected)
            .frame(false)
            .kind(WidgetKind::Selectable)
            .ui(self);
        if r.clicked() && self.menu.combo {
            self.close();
        }
        r
    }

    /// Выбираемое значение: щелчок присваивает `*current = alternative`.
    pub fn selectable_value<T: PartialEq>(
        &mut self,
        current: &mut T,
        alternative: T,
        text: impl Into<RichText>,
    ) -> Response {
        let mut r = self.selectable_label(*current == alternative, text);
        if r.clicked() && *current != alternative {
            *current = alternative;
            r.mark_changed();
        }
        r
    }

    /// Разделитель: горизонтальная линия в столбце, вертикальная в ряду.
    pub fn separator(&mut self) -> Response {
        self.paint_separator()
    }

    /// Закрыть меню или выпадающий список, в котором находится область.
    pub fn close(&mut self) {
        if self.menu.depth > 0 {
            self.ctx.st().close_menus();
        }
    }

    /// То же, что [`Ui::close`] (старое имя).
    pub fn close_menu(&mut self) {
        self.close();
    }
}

// ------------------------------------------------------------------ подпись

/// Подпись. В столбце (и в ряду внутри плавающего окна) длинный текст переносится
/// по словам; `'\n'` начинает новую строку.
pub struct Label {
    text: RichText,
    sense: Sense,
}

impl Label {
    /// Подпись.
    pub fn new(text: impl Into<RichText>) -> Self {
        Label {
            text: text.into(),
            sense: Sense::hover(),
        }
    }

    /// Что чувствует подпись (например, `Sense::click()`).
    pub fn sense(mut self, sense: Sense) -> Self {
        self.sense = sense;
        self
    }
}

impl Widget for Label {
    fn ui(self, ui: &mut Ui) -> Response {
        let th = ui.theme.clone();
        let (size, font) = self.text.font(&th);
        let ppp = ui.ppp;
        let s = self.text.text();
        let one_line = s
            .split('\n')
            .map(|l| text::measure(l, size, font, ppp).x)
            .fold(0.0, f32::max);
        let lines: Vec<String> = match ui.text_wrap_width(one_line) {
            Some(w) if one_line > w => text::wrap(s, size, font, ppp, w),
            _ => s.split('\n').map(str::to_owned).collect(),
        };
        let line_h = text::line_height(size, font, ppp);
        let w = lines
            .iter()
            .map(|l| text::measure(l, size, font, ppp).x)
            .fold(0.0, f32::max);
        let rect = ui.allocate_space(vec2(w, line_h * lines.len() as f32));
        let id = ui.next_label_id();
        let r = ui.interact(rect, id, self.sense);
        let color = ui.fade(self.text.color_or(&th, th.text_color));
        for (i, l) in lines.iter().enumerate() {
            if !l.is_empty() {
                ui.paint_text(
                    rect.min + vec2(0.0, line_h * i as f32),
                    l,
                    size,
                    font,
                    color,
                );
            }
        }
        ui.register(id, WidgetKind::Label, s, None, None, rect, r.enabled);
        r
    }
}

// ------------------------------------------------------------------ кнопка

/// Кнопка. В выпадающем меню — пункт меню во всю ширину без рамки.
pub struct Button {
    text: RichText,
    small: bool,
    frame: Option<bool>,
    selected: bool,
    min_size: Vec2,
    fill: Option<Color>,
    kind: WidgetKind,
}

impl Button {
    /// Кнопка с подписью.
    pub fn new(text: impl Into<RichText>) -> Self {
        Button {
            text: text.into(),
            small: false,
            frame: None,
            selected: false,
            min_size: Vec2::ZERO,
            fill: None,
            kind: WidgetKind::Button,
        }
    }

    /// Маленькая (без минимальной высоты).
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    /// Рисовать рамку и фон всегда (`true`) или только при наведении (`false`).
    pub fn frame(mut self, frame: bool) -> Self {
        self.frame = Some(frame);
        self
    }

    /// Выбрана (подсвечена цветом выделения).
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Наименьший размер.
    pub fn min_size(mut self, size: Vec2) -> Self {
        self.min_size = size;
        self
    }

    /// Цвет фона.
    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }

    pub(crate) fn kind(mut self, kind: WidgetKind) -> Self {
        self.kind = kind;
        self
    }
}

impl Widget for Button {
    fn ui(self, ui: &mut Ui) -> Response {
        let th = ui.theme.clone();
        let in_menu = ui.menu.depth > 0;
        let mut pad = th.button_padding;
        if self.small {
            pad.y = 0.0;
        }
        let ts = ui.text_size(&self.text);
        let mut size = ts + pad * 2.0;
        if !self.small {
            size.y = size.y.max(th.interact_height);
        }
        size = size.max(self.min_size);
        if in_menu {
            // Пункт меню — во всю ширину меню.
            size.x = size.x.max(ui.min_rect().max.x - ui.next_pos().x);
        }
        let rect = ui.allocate_space(size);
        let id = ui.next_auto_id();
        let r = ui.interact(rect, id, Sense::click());
        let v = ui.style_of(&r);
        let framed = self.frame.unwrap_or(!in_menu);
        let busy = r.hovered || r.interact_pointer_pos.is_some();
        if self.selected {
            ui.paint(Shape::Rect {
                rect: rect.expand(v.expansion),
                radius: v.radius,
                fill: ui.fade(th.selection_bg),
                stroke: (1.0, ui.fade(th.selection_stroke)),
            });
        } else if framed || busy {
            ui.paint(Shape::Rect {
                rect: rect.expand(v.expansion),
                radius: v.radius,
                fill: self.fill.map_or(v.weak_bg_fill, |c| ui.fade(c)),
                stroke: v.bg_stroke,
            });
        }
        let color = if self.selected {
            ui.fade(th.selection_stroke)
        } else if self.text.has_color() {
            ui.fade(self.text.color_or(&th, v.fg))
        } else {
            v.fg
        };
        let (fs, font) = self.text.font(&th);
        let pos = if in_menu || self.kind == WidgetKind::Selectable {
            pos2(rect.min.x + pad.x, rect.center().y - ts.y * 0.5)
        } else {
            rect.center() - ts * 0.5
        };
        ui.paint_text(pos, self.text.text(), fs, font, color);
        let selected = (self.kind == WidgetKind::Selectable).then_some(self.selected);
        ui.register(
            id,
            self.kind,
            self.text.text(),
            None,
            selected,
            rect,
            r.enabled,
        );
        r
    }
}

// ------------------------------------------------------------------ флажок и переключатель

/// Флажок.
pub struct Checkbox<'a> {
    checked: &'a mut bool,
    text: RichText,
}

impl<'a> Checkbox<'a> {
    /// Флажок с подписью.
    pub fn new(checked: &'a mut bool, text: impl Into<RichText>) -> Self {
        Checkbox {
            checked,
            text: text.into(),
        }
    }
}

/// Разместить значок и подпись (флажок, переключатель); возвращает ответ и квадрат значка.
fn icon_and_text(ui: &mut Ui, text: &RichText) -> (Response, Rect, Pos2) {
    let th = ui.theme.clone();
    let ts = ui.text_size(text);
    let w = th.icon_width
        + if text.text().is_empty() {
            0.0
        } else {
            th.icon_spacing + ts.x
        };
    let rect = ui.allocate_space(vec2(w, ts.y.max(th.interact_height)));
    let id = ui.next_auto_id();
    let r = ui.interact(rect, id, Sense::click());
    let icon = Rect::from_min_size(
        pos2(rect.min.x, rect.center().y - th.icon_width * 0.5),
        Vec2::splat(th.icon_width),
    );
    let tp = pos2(icon.max.x + th.icon_spacing, rect.center().y - ts.y * 0.5);
    (r, icon, tp)
}

impl Widget for Checkbox<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let th = ui.theme.clone();
        let (mut r, icon, tp) = icon_and_text(ui, &self.text);
        if r.clicked() {
            *self.checked = !*self.checked;
            r.mark_changed();
        }
        let v = ui.style_of(&r);
        ui.paint(Shape::Rect {
            rect: icon.expand(v.expansion),
            radius: v.radius,
            fill: v.bg_fill,
            stroke: v.bg_stroke,
        });
        if *self.checked {
            let s = Rect::from_center_size(icon.center(), Vec2::splat(th.icon_width_inner));
            ui.paint(Shape::Polyline {
                points: vec![
                    pos2(s.min.x, s.center().y),
                    pos2(s.center().x, s.max.y),
                    pos2(s.max.x, s.min.y),
                ],
                width: v.fg_width.max(1.5),
                color: v.fg,
            });
        }
        let (fs, font) = self.text.font(&th);
        let color = if self.text.has_color() {
            ui.fade(self.text.color_or(&th, v.fg))
        } else {
            v.fg
        };
        ui.paint_text(tp, self.text.text(), fs, font, color);
        ui.register(
            r.id,
            WidgetKind::Checkbox,
            self.text.text(),
            None,
            Some(*self.checked),
            r.rect,
            r.enabled,
        );
        r
    }
}

/// Переключатель.
pub struct RadioButton {
    selected: bool,
    text: RichText,
}

impl RadioButton {
    /// Переключатель с подписью.
    pub fn new(selected: bool, text: impl Into<RichText>) -> Self {
        RadioButton {
            selected,
            text: text.into(),
        }
    }
}

impl Widget for RadioButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let th = ui.theme.clone();
        let (r, icon, tp) = icon_and_text(ui, &self.text);
        let v = ui.style_of(&r);
        ui.paint(Shape::Circle {
            center: icon.center(),
            radius: icon.width() * 0.5 + v.expansion,
            fill: v.bg_fill,
            stroke: v.bg_stroke,
        });
        if self.selected {
            ui.paint(Shape::Circle {
                center: icon.center(),
                radius: th.icon_width_inner / 3.0 + 0.5,
                fill: v.fg,
                stroke: (0.0, Color::TRANSPARENT),
            });
        }
        let (fs, font) = self.text.font(&th);
        let color = if self.text.has_color() {
            ui.fade(self.text.color_or(&th, v.fg))
        } else {
            v.fg
        };
        ui.paint_text(tp, self.text.text(), fs, font, color);
        ui.register(
            r.id,
            WidgetKind::Radio,
            self.text.text(),
            None,
            Some(self.selected),
            r.rect,
            r.enabled,
        );
        r
    }
}

// ------------------------------------------------------------------ разделитель

/// Разделитель.
#[derive(Default)]
pub struct Separator;

impl Widget for Separator {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.paint_separator()
    }
}

// ------------------------------------------------------------------ ползунок

/// Число для ползунка.
pub trait Numeric: Copy {
    /// Целое (значение округляется).
    const INTEGRAL: bool;
    /// В `f64`.
    fn to_f64(self) -> f64;
    /// Из `f64`.
    fn from_f64(v: f64) -> Self;
}

macro_rules! numeric {
    ($($t:ty: $int:expr),*) => {$(
        impl Numeric for $t {
            const INTEGRAL: bool = $int;
            fn to_f64(self) -> f64 {
                self as f64
            }
            fn from_f64(v: f64) -> Self {
                v as $t
            }
        }
    )*};
}

numeric!(f32: false, f64: false, i32: true, i64: true, u8: true, u16: true, u32: true, usize: true);

/// Ползунок с полем значения и подписью. Мышью — щелчок и
/// перетаскивание; в фокусе — стрелки (шаг 1/100 диапазона или `step_by`), Home, End.
pub struct Slider<'a> {
    get_set: Box<dyn FnMut(Option<f64>) -> f64 + 'a>,
    range: RangeInclusive<f64>,
    text: String,
    step: Option<f64>,
    decimals: Option<usize>,
    integral: bool,
}

impl<'a> Slider<'a> {
    /// Ползунок для `value` в диапазоне `range`.
    pub fn new<N: Numeric>(value: &'a mut N, range: RangeInclusive<N>) -> Self {
        let range = range.start().to_f64()..=range.end().to_f64();
        Slider {
            get_set: Box::new(move |v| {
                if let Some(v) = v {
                    *value = N::from_f64(v);
                }
                value.to_f64()
            }),
            range,
            text: String::new(),
            step: None,
            decimals: None,
            integral: N::INTEGRAL,
        }
    }

    /// Подпись справа.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    /// Шаг значения.
    pub fn step_by(mut self, step: f64) -> Self {
        self.step = (step > 0.0).then_some(step);
        self
    }

    /// Число знаков после запятой в поле значения.
    pub fn fixed_decimals(mut self, n: usize) -> Self {
        self.decimals = Some(n);
        self
    }

    fn format(&self, v: f64) -> String {
        let d = self.decimals.unwrap_or(if self.integral { 0 } else { 2 });
        let s = format!("{v:.d$}");
        if self.decimals.is_none() && s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            s
        }
    }

    fn clean(&self, v: f64) -> f64 {
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let mut v = v.clamp(lo.min(hi), hi.max(lo));
        if let Some(s) = self.step {
            v = lo + ((v - lo) / s).round() * s;
        }
        if self.integral {
            v = v.round();
        }
        v.clamp(lo.min(hi), hi.max(lo))
    }
}

impl Widget for Slider<'_> {
    fn ui(mut self, ui: &mut Ui) -> Response {
        let th = ui.theme.clone();
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let span = hi - lo;
        let mut value = (self.get_set)(None);
        let h = th.interact_height;
        let rt_val = RichText::new(self.format(value));
        let val_w = (ui.text_size(&rt_val).x + th.button_padding.x * 2.0).max(40.0);
        let label = RichText::new(self.text.clone());
        let lab_w = if self.text.is_empty() {
            0.0
        } else {
            th.item_spacing.x + ui.text_size(&label).x
        };
        let total = vec2(th.slider_width + th.item_spacing.x + val_w + lab_w, h);
        let rect = ui.allocate_space(total);
        let srect = Rect::from_min_size(rect.min, vec2(th.slider_width, h));
        let id = ui.next_auto_id();
        let mut r = ui.interact(srect, id, Sense::click_and_drag().focusable());
        let handle_r = h / 2.5;
        let x0 = srect.min.x + handle_r;
        let x1 = srect.max.x - handle_r;
        let old = value;
        if let Some(p) = r.interact_pointer_pos
            && (r.pressed || r.dragged)
            && x1 > x0
        {
            let t = ((p.x - x0) / (x1 - x0)).clamp(0.0, 1.0) as f64;
            value = self.clean(lo + t * span);
        }
        if r.has_focus && r.enabled {
            let step = self.step.unwrap_or(if self.integral {
                1.0
            } else {
                span.abs() / 100.0
            });
            let evs: Vec<KeyCode> = ui.input(|i| {
                i.key_events()
                    .filter(|k| k.pressed)
                    .map(|k| k.key)
                    .collect()
            });
            for k in evs {
                value = match k {
                    KeyCode::ArrowLeft | KeyCode::ArrowDown => self.clean(value - step),
                    KeyCode::ArrowRight | KeyCode::ArrowUp => self.clean(value + step),
                    KeyCode::Home => lo,
                    KeyCode::End => hi,
                    _ => value,
                };
            }
        }
        if value != old {
            (self.get_set)(Some(value));
            value = (self.get_set)(None);
            r.mark_changed();
        }
        // Дорожка и ручка.
        let v = ui.style_of(&r);
        let rail = Rect::from_min_max(
            pos2(
                x0 - th.slider_rail * 0.5,
                srect.center().y - th.slider_rail * 0.5,
            ),
            pos2(
                x1 + th.slider_rail * 0.5,
                srect.center().y + th.slider_rail * 0.5,
            ),
        );
        ui.paint(Shape::Rect {
            rect: rail,
            radius: th.slider_rail * 0.5,
            fill: ui.fade(th.inactive.bg_fill),
            stroke: (0.0, Color::TRANSPARENT),
        });
        let t = if span != 0.0 {
            ((value - lo) / span) as f32
        } else {
            0.0
        };
        let hx = x0 + t.clamp(0.0, 1.0) * (x1 - x0);
        ui.paint(Shape::Circle {
            center: pos2(hx, srect.center().y),
            radius: handle_r + v.expansion,
            fill: v.bg_fill,
            stroke: (v.fg_width, v.fg),
        });
        // Поле значения.
        let vrect = Rect::from_min_size(
            pos2(srect.max.x + th.item_spacing.x, rect.min.y),
            vec2(val_w, h),
        );
        let vs = self.format(value);
        ui.paint(Shape::Rect {
            rect: vrect,
            radius: th.inactive.radius,
            fill: ui.fade(th.inactive.weak_bg_fill),
            stroke: (0.0, Color::TRANSPARENT),
        });
        let vts = ui.text_size(&RichText::new(vs.clone()));
        let (fs, font) = (th.body_size, crate::gfx::FontKind::Sans);
        ui.paint_text(
            vrect.center() - vts * 0.5,
            &vs,
            fs,
            font,
            ui.fade(th.inactive.fg),
        );
        if !self.text.is_empty() {
            let ls = ui.text_size(&label);
            ui.paint_text(
                pos2(
                    vrect.max.x + th.item_spacing.x,
                    rect.center().y - ls.y * 0.5,
                ),
                &self.text,
                fs,
                font,
                ui.fade(th.text_color),
            );
        }
        ui.register(
            id,
            WidgetKind::Slider,
            &self.text,
            Some(vs),
            None,
            rect,
            r.enabled,
        );
        r.rect = rect;
        r
    }
}
