//! Свой набор виджетов в немедленном режиме (immediate mode) поверх холста [`Canvas`].
//!
//! Набор виджетов, нужный окну эмулятора: [`Ui::label`], [`Ui::button`],
//! [`Ui::checkbox`], [`Ui::radio_value`], [`Slider`], [`TextEdit`], [`ComboBox`], [`Grid`],
//! [`ScrollArea`], меню ([`MenuBar`], [`Ui::menu_button`]), панели ([`TopBottomPanel`],
//! [`CentralPanel`]) и плавающие окна ([`Window`]).
//!
//! Кадр:
//!
//! ```ignore
//! ctx.begin_frame(&input);
//! TopBottomPanel::top("menu").show(&ctx, |ui| { /* … */ });
//! CentralPanel::default().frame(Frame::NONE).show(&ctx, |ui| { /* … */ });
//! Window::new("Окно").open(&mut open).show(&ctx, |ui| { /* … */ });
//! let out = ctx.end_frame();
//! ctx.with_canvas(|c| c.to_0rgb(buffer));
//! ```
//!
//! Виджеты рисуются не сразу, а записываются в списки фигур своего слоя ([`LayerId`]);
//! в [`Ctx::end_frame`] слои выводятся на холст по порядку: фон (панели), плавающие окна
//! (последнее поднятое — сверху), выпадающие меню, подсказки. Ввод мыши достаётся только
//! верхнему слою под мышью; слои определяются по их прямоугольникам из прошлого кадра.
//! Прямое рисование на холсте ([`Ui::canvas_at`]) идёт сразу, то есть **под** всеми
//! записанными фигурами, — для рисунка компьютера в центральной панели.
//!
//! Для тестов каждый кадр составляет [`Registry`] — список виджетов с подписями,
//! прямоугольниками и доступностью; [`testing::Harness`] прогоняет кадры без окна.

pub mod clipboard;
pub mod icon;
pub mod input;
pub mod registry;
pub mod testing;
pub mod theme;

mod layout;
mod menu;
mod panel;
mod scroll;
mod text;
mod text_edit;
mod widgets;

#[cfg(test)]
mod tests;

use std::cell::{Ref, RefCell, RefMut};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::gfx::{Canvas, Color, FontKind, Pos2, Rect, Vec2, vec2};

pub use clipboard::{Clipboard, MemoryClipboard, SystemClipboard};
pub use icon::Icon;
pub use input::{Event, Input, KeyCode, KeyEvent, Modifiers, PointerButton};
pub use layout::{Grid, Ui};
pub use menu::{ComboBox, MenuBar};
pub use panel::{CentralPanel, Frame, TopBottomPanel, Window};
pub use registry::{Registry, WidgetInfo, WidgetKind};
pub use scroll::ScrollArea;
pub use text_edit::TextEdit;
pub use theme::{Theme, WidgetVisuals};
pub use widgets::{Button, Label, Separator, Slider, Widget};

// ------------------------------------------------------------------ идентификаторы и слои

/// Идентификатор виджета, окна, меню (хэш пути и подписи).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(u64);

impl Id {
    /// Id из любого хэшируемого значения.
    pub fn new(source: impl Hash) -> Id {
        let mut h = std::hash::DefaultHasher::new();
        source.hash(&mut h);
        Id(h.finish())
    }

    /// Дочерний id.
    pub fn with(self, child: impl Hash) -> Id {
        let mut h = std::hash::DefaultHasher::new();
        self.0.hash(&mut h);
        child.hash(&mut h);
        Id(h.finish())
    }
}

/// Порядок слоя.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Order {
    /// Фон: панели и центральная панель.
    Background,
    /// Плавающие окна.
    Middle,
    /// Выпадающие меню и списки.
    Foreground,
    /// Всплывающие подсказки.
    Tooltip,
}

/// Слой рисования и ввода.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerId {
    /// Порядок.
    pub order: Order,
    /// Id окна или меню (у фона — общий).
    pub id: Id,
}

impl LayerId {
    /// Фоновый слой (все панели).
    pub fn background() -> LayerId {
        LayerId {
            order: Order::Background,
            id: Id::new("фон"),
        }
    }
}

/// Вид указателя мыши.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum CursorIcon {
    /// Обычная стрелка.
    #[default]
    Default,
    /// Рука (можно нажать).
    PointingHand,
    /// Текстовый курсор.
    Text,
    /// Перетаскивание.
    Grabbing,
}

/// Что виджет «чувствует».
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sense {
    /// Щелчки.
    pub click: bool,
    /// Перетаскивание.
    pub drag: bool,
    /// Получает фокус клавиатуры при нажатии.
    pub focusable: bool,
}

impl Sense {
    /// Только наведение.
    pub fn hover() -> Sense {
        Sense::default()
    }

    /// Щелчки.
    pub fn click() -> Sense {
        Sense {
            click: true,
            ..Sense::default()
        }
    }

    /// Перетаскивание.
    pub fn drag() -> Sense {
        Sense {
            drag: true,
            ..Sense::default()
        }
    }

    /// Щелчки и перетаскивание.
    pub fn click_and_drag() -> Sense {
        Sense {
            click: true,
            drag: true,
            focusable: false,
        }
    }

    /// То же с фокусом клавиатуры.
    pub fn focusable(mut self) -> Sense {
        self.focusable = true;
        self
    }

    fn interactive(self) -> bool {
        self.click || self.drag
    }
}

// ------------------------------------------------------------------ ответ виджета

/// Результат взаимодействия с виджетом за кадр.
#[derive(Clone)]
pub struct Response {
    /// Id виджета.
    pub id: Id,
    /// Прямоугольник виджета.
    pub rect: Rect,
    /// Слой.
    pub layer: LayerId,
    /// Мышь над виджетом (и он не перекрыт другим слоем).
    pub hovered: bool,
    /// Основная кнопка нажата на виджете в этом кадре.
    pub pressed: bool,
    /// Щелчок (нажатие и отпускание на виджете) в этом кадре.
    pub clicked: bool,
    /// Виджет тащат мышью.
    pub dragged: bool,
    /// Сдвиг мыши за кадр при перетаскивании.
    pub drag_delta: Vec2,
    /// Значение изменилось.
    pub changed: bool,
    /// Виджет доступен.
    pub enabled: bool,
    /// Виджет в фокусе клавиатуры.
    pub has_focus: bool,
    /// Виджет потерял фокус в этом кадре (Enter, Esc, щелчок вне).
    pub lost_focus: bool,
    /// Позиция мыши, если виджет нажат или тащится.
    pub interact_pointer_pos: Option<Pos2>,
    ctx: Ctx,
}

impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Response")
            .field("id", &self.id)
            .field("rect", &self.rect)
            .field("hovered", &self.hovered)
            .field("clicked", &self.clicked)
            .field("changed", &self.changed)
            .finish()
    }
}

impl Response {
    /// Щелчок.
    pub fn clicked(&self) -> bool {
        self.clicked
    }

    /// Значение изменилось.
    pub fn changed(&self) -> bool {
        self.changed
    }

    /// Мышь над виджетом.
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    /// Нажат в этом кадре.
    pub fn is_pointer_button_down_on(&self) -> bool {
        self.pressed || self.dragged
    }

    /// Тащат мышью.
    pub fn dragged(&self) -> bool {
        self.dragged
    }

    /// В фокусе клавиатуры.
    pub fn has_focus(&self) -> bool {
        self.has_focus
    }

    /// Потерял фокус в этом кадре.
    pub fn lost_focus(&self) -> bool {
        self.lost_focus
    }

    /// Отметить изменение значения.
    pub fn mark_changed(&mut self) {
        self.changed = true;
    }

    /// Запросить фокус клавиатуры.
    pub fn request_focus(&self) {
        self.ctx.request_focus(self.id);
    }

    /// Подсказка при наведении (показывается после задержки [`Theme::tooltip_delay`]).
    pub fn on_hover_text(self, text: impl Into<String>) -> Self {
        let text = text.into();
        let mut st = self.ctx.st();
        if let Some(w) = st
            .registry
            .widgets
            .iter_mut()
            .rev()
            .find(|w| w.id == self.id)
        {
            w.hover_text = Some(text.clone());
        }
        if self.hovered {
            st.request_tooltip(self.id, text, self.rect);
        }
        drop(st);
        self
    }
}

/// Результат контейнера: значение замыкания и ответ занятой области.
#[derive(Debug)]
pub struct InnerResponse<R> {
    /// Что вернуло замыкание.
    pub inner: R,
    /// Ответ области.
    pub response: Response,
}

// ------------------------------------------------------------------ текст

/// Начертание текста.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum TextStyle {
    /// Мелкий.
    Small,
    /// Обычный.
    #[default]
    Body,
    /// Моноширинный.
    Monospace,
    /// Кнопки.
    Button,
    /// Заголовок.
    Heading,
}

impl TextStyle {
    /// Кегль и шрифт по теме.
    pub fn font(self, theme: &Theme) -> (f32, FontKind) {
        match self {
            TextStyle::Small => (theme.small_size, FontKind::Sans),
            TextStyle::Body | TextStyle::Button => (theme.body_size, FontKind::Sans),
            TextStyle::Monospace => (theme.mono_size, FontKind::Mono),
            TextStyle::Heading => (theme.heading_size, FontKind::Sans),
        }
    }
}

/// Текст с оформлением.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RichText {
    text: String,
    style: TextStyle,
    color: Option<Color>,
    strong: bool,
    size: Option<f32>,
}

impl RichText {
    /// Обычный текст.
    pub fn new(text: impl Into<String>) -> Self {
        RichText {
            text: text.into(),
            ..RichText::default()
        }
    }

    /// Цвет.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Выделенный (яркий) цвет.
    pub fn strong(mut self) -> Self {
        self.strong = true;
        self
    }

    /// Моноширинный шрифт.
    pub fn monospace(mut self) -> Self {
        self.style = TextStyle::Monospace;
        self
    }

    /// Заголовок.
    pub fn heading(mut self) -> Self {
        self.style = TextStyle::Heading;
        self
    }

    /// Мелкий текст.
    pub fn small(mut self) -> Self {
        self.style = TextStyle::Small;
        self
    }

    /// Начертание.
    pub fn text_style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// Кегль.
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Текст.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Кегль и шрифт.
    pub(crate) fn font(&self, theme: &Theme) -> (f32, FontKind) {
        let (s, f) = self.style.font(theme);
        (self.size.unwrap_or(s), f)
    }

    /// Цвет: заданный, или выделенный/заголовок, или `default`.
    pub(crate) fn color_or(&self, theme: &Theme, default: Color) -> Color {
        self.color
            .unwrap_or(if self.strong || self.style == TextStyle::Heading {
                theme.strong_text
            } else {
                default
            })
    }

    /// Цвет задан явно или выделен.
    pub(crate) fn has_color(&self) -> bool {
        self.color.is_some() || self.strong || self.style == TextStyle::Heading
    }
}

impl From<&str> for RichText {
    fn from(s: &str) -> Self {
        RichText::new(s)
    }
}

impl From<&String> for RichText {
    fn from(s: &String) -> Self {
        RichText::new(s.clone())
    }
}

impl From<String> for RichText {
    fn from(s: String) -> Self {
        RichText::new(s)
    }
}

impl From<[f32; 2]> for Vec2 {
    fn from(v: [f32; 2]) -> Self {
        vec2(v[0], v[1])
    }
}

impl From<[f32; 2]> for Pos2 {
    fn from(v: [f32; 2]) -> Self {
        Pos2::new(v[0], v[1])
    }
}

// ------------------------------------------------------------------ фигуры

/// Фигура в списке слоя.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Пусто (место, зарезервированное под фон, — см. [`Ui::reserve_shape`]).
    Noop,
    /// Прямоугольник с заливкой и рамкой внутри (толщина 0 — без рамки).
    Rect {
        /// Прямоугольник.
        rect: Rect,
        /// Скругление.
        radius: f32,
        /// Заливка.
        fill: Color,
        /// Рамка.
        stroke: (f32, Color),
    },
    /// Круг.
    Circle {
        /// Центр.
        center: Pos2,
        /// Радиус.
        radius: f32,
        /// Заливка.
        fill: Color,
        /// Рамка.
        stroke: (f32, Color),
    },
    /// Отрезок.
    Line {
        /// Начало.
        a: Pos2,
        /// Конец.
        b: Pos2,
        /// Толщина.
        width: f32,
        /// Цвет.
        color: Color,
    },
    /// Залитый многоугольник.
    Polygon {
        /// Вершины.
        points: Vec<Pos2>,
        /// Заливка.
        fill: Color,
    },
    /// Ломаная.
    Polyline {
        /// Вершины.
        points: Vec<Pos2>,
        /// Толщина.
        width: f32,
        /// Цвет.
        color: Color,
    },
    /// Строка текста (значки подставляются), левый верхний угол — `pos`.
    Text {
        /// Левый верхний угол.
        pos: Pos2,
        /// Текст.
        text: String,
        /// Кегль.
        size: f32,
        /// Шрифт.
        font: FontKind,
        /// Цвет.
        color: Color,
    },
    /// Значок в прямоугольнике.
    Icon {
        /// Прямоугольник.
        rect: Rect,
        /// Значок.
        icon: Icon,
        /// Кегль.
        size: f32,
        /// Цвет.
        color: Color,
    },
    /// Мягкая тень под прямоугольником.
    Shadow {
        /// Прямоугольник, отбрасывающий тень.
        rect: Rect,
        /// Скругление.
        radius: f32,
        /// Сдвиг тени.
        offset: Vec2,
        /// Размытие.
        blur: f32,
        /// Цвет (непрозрачность в середине тени).
        color: Color,
    },
}

impl Shape {
    /// Нарисовать на холсте.
    pub fn paint(&self, c: &mut Canvas) {
        match self {
            Shape::Noop => {}
            Shape::Rect {
                rect,
                radius,
                fill,
                stroke,
            } => {
                c.fill_rect(*rect, *radius, *fill);
                if stroke.0 > 0.0 && !stroke.1.is_transparent() {
                    let w = stroke.0;
                    c.stroke_rect(
                        rect.shrink(w * 0.5),
                        (radius - w * 0.5).max(0.0),
                        w,
                        stroke.1,
                    );
                }
            }
            Shape::Circle {
                center,
                radius,
                fill,
                stroke,
            } => {
                if !fill.is_transparent() {
                    c.fill_circle(*center, *radius, *fill);
                }
                if stroke.0 > 0.0 && !stroke.1.is_transparent() {
                    c.stroke_circle(*center, radius - stroke.0 * 0.5, stroke.0, stroke.1);
                }
            }
            Shape::Line { a, b, width, color } => c.line(*a, *b, *width, *color),
            Shape::Polygon { points, fill } => c.fill_polygon(points, *fill),
            Shape::Polyline {
                points,
                width,
                color,
            } => c.polyline(points, *width, *color),
            Shape::Text {
                pos,
                text: t,
                size,
                font,
                color,
            } => {
                text::paint(c, *pos, t, *size, *font, *color);
            }
            Shape::Icon {
                rect,
                icon,
                size,
                color,
            } => icon.paint(c, *rect, *size, *color),
            Shape::Shadow {
                rect,
                radius,
                offset,
                blur,
                color,
            } => {
                // Несколько расширяющихся полупрозрачных прямоугольников.
                const N: usize = 6;
                let a = color.a as f32 / N as f32;
                let r = rect.translate(*offset * 0.5);
                for i in 0..N {
                    let e = blur * (1.0 - i as f32 / N as f32) * 0.5;
                    c.fill_rect(
                        r.expand(e),
                        radius + e,
                        color.with_alpha(a.round().max(1.0) as u8),
                    );
                }
            }
        }
    }
}

/// Место фигуры в списке слоя (для замены после раскладки содержимого).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapeIdx {
    list: usize,
    idx: usize,
}

// ------------------------------------------------------------------ состояние

/// Фигуры слоя за кадр.
struct LayerPaint {
    layer: LayerId,
    shapes: Vec<(Rect, Shape)>,
    /// Занятые прямоугольники (для определения слоя под мышью).
    areas: Vec<Rect>,
}

/// Состояние плавающего окна между кадрами.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WindowMem {
    pub pos: Pos2,
    /// Размер содержимого в прошлом кадре.
    pub content: Vec2,
    pub collapsed: bool,
    /// Кадр, в котором окно показывалось последний раз.
    pub last_frame: u64,
}

/// Состояние прокрутки.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ScrollMem {
    pub offset: f32,
    pub content_h: f32,
}

/// Размеры столбцов и строк таблицы в прошлом кадре.
#[derive(Debug, Clone, Default)]
pub(crate) struct GridMem {
    pub widths: Vec<f32>,
    pub heights: Vec<f32>,
}

/// Состояние поля ввода.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TextMem {
    /// Курсор (номер символа).
    pub cursor: usize,
    /// Другой конец выделения.
    pub anchor: usize,
    /// Горизонтальная прокрутка текста.
    pub scroll: f32,
}

/// Состояние набора виджетов: всё, что живёт между кадрами.
pub(crate) struct State {
    pub canvas: Canvas,
    pub input: Input,
    pub theme: Rc<Theme>,
    pub clipboard: Box<dyn Clipboard>,
    pub frame: u64,
    pub screen: Rect,
    /// Свободная часть экрана для панелей.
    pub available: Rect,

    lists: Vec<LayerPaint>,
    prev_areas: Vec<(LayerId, Vec<Rect>)>,
    /// Порядок окон снизу вверх.
    pub window_order: Vec<Id>,
    hover_layer: Option<LayerId>,
    press_layer: Option<LayerId>,
    /// Нажатие мимо открытого меню: только закрывает меню, виджетам не достаётся.
    press_blocked: bool,
    prev_pointer: Option<Pos2>,

    pub active: Option<Id>,
    pub focus: Option<Id>,
    focus_at_begin: Option<Id>,
    focus_claimed: bool,
    focus_seen: bool,
    /// Фокус у поля ввода, нарисованного в этом кадре.
    pub text_focus: bool,
    wants_keyboard: bool,
    /// Виджет, потерявший фокус в прошлом кадре (сообщается в этом).
    lost_focus: Option<Id>,
    lost_reported: bool,
    /// Щелчок «без мыши» по id (тестовый стенд).
    pub pending_click: Option<Id>,

    /// Открытые меню: путь от корневого (строка меню, выпадающий список) к вложенным.
    pub open_menus: Vec<Id>,
    menus_seen: HashSet<Id>,
    close_menus: bool,
    pub menu_button_pressed: bool,
    popup_areas: Vec<Rect>,

    pub windows: HashMap<Id, WindowMem>,
    pub scrolls: HashMap<Id, ScrollMem>,
    pub grids: HashMap<Id, GridMem>,
    /// Размеры выпадающих меню в прошлом кадре.
    pub popups: HashMap<Id, Vec2>,
    /// Высоты панелей в прошлом кадре.
    pub panels: HashMap<Id, f32>,
    pub texts: HashMap<Id, TextMem>,
    /// Колесо, ещё не использованное областями прокрутки.
    pub scroll_left: Vec2,

    cursor_icon: CursorIcon,
    tooltip: Option<(Id, String, Rect)>,
    hover_since: Option<(Id, f64)>,
    pub registry: Registry,
    prev_registry: Registry,
}

impl State {
    fn new() -> Self {
        State {
            canvas: Canvas::new(1, 1),
            input: Input::default(),
            theme: Rc::new(Theme::dark()),
            clipboard: Box::new(MemoryClipboard::new()),
            frame: 0,
            screen: Rect::NOTHING,
            available: Rect::NOTHING,
            lists: Vec::new(),
            prev_areas: Vec::new(),
            window_order: Vec::new(),
            hover_layer: None,
            press_layer: None,
            press_blocked: false,
            prev_pointer: None,
            active: None,
            focus: None,
            focus_at_begin: None,
            focus_claimed: false,
            focus_seen: false,
            text_focus: false,
            wants_keyboard: false,
            lost_focus: None,
            lost_reported: false,
            pending_click: None,
            open_menus: Vec::new(),
            menus_seen: HashSet::new(),
            close_menus: false,
            menu_button_pressed: false,
            popup_areas: Vec::new(),
            windows: HashMap::new(),
            scrolls: HashMap::new(),
            grids: HashMap::new(),
            popups: HashMap::new(),
            panels: HashMap::new(),
            texts: HashMap::new(),
            scroll_left: Vec2::ZERO,
            cursor_icon: CursorIcon::Default,
            tooltip: None,
            hover_since: None,
            registry: Registry::default(),
            prev_registry: Registry::default(),
        }
    }

    fn list_idx(&mut self, layer: LayerId) -> usize {
        if let Some(i) = self.lists.iter().position(|l| l.layer == layer) {
            return i;
        }
        self.lists.push(LayerPaint {
            layer,
            shapes: Vec::new(),
            areas: Vec::new(),
        });
        self.lists.len() - 1
    }

    pub fn add_shape(&mut self, layer: LayerId, clip: Rect, shape: Shape) -> ShapeIdx {
        let list = self.list_idx(layer);
        let shapes = &mut self.lists[list].shapes;
        shapes.push((clip, shape));
        ShapeIdx {
            list,
            idx: shapes.len() - 1,
        }
    }

    pub fn set_shape(&mut self, at: ShapeIdx, shape: Shape) {
        if let Some(s) = self
            .lists
            .get_mut(at.list)
            .and_then(|l| l.shapes.get_mut(at.idx))
        {
            s.1 = shape;
        }
    }

    /// Добавить занятый прямоугольник слоя; возвращает его номер для уточнения.
    pub fn add_area(&mut self, layer: LayerId, rect: Rect) -> (usize, usize) {
        let i = self.list_idx(layer);
        self.lists[i].areas.push(rect);
        (i, self.lists[i].areas.len() - 1)
    }

    pub fn set_area(&mut self, at: (usize, usize), rect: Rect) {
        if let Some(a) = self.lists.get_mut(at.0).and_then(|l| l.areas.get_mut(at.1)) {
            *a = rect;
        }
    }

    /// Верхний слой в точке по прямоугольникам прошлого кадра.
    pub fn layer_at(&self, pos: Pos2) -> Option<LayerId> {
        self.prev_areas
            .iter()
            .rev()
            .find(|(_, rs)| rs.iter().any(|r| r.contains(pos)))
            .map(|(l, _)| *l)
    }

    /// Мышь над слоем `layer` (он верхний под мышью).
    pub fn layer_hovered(&self, layer: LayerId) -> bool {
        match self.hover_layer {
            Some(l) => l == layer,
            None => layer.order == Order::Background,
        }
    }

    /// Нажатие в этом кадре пришлось на слой `layer`.
    pub fn layer_pressed(&self, layer: LayerId) -> bool {
        self.input.primary_pressed()
            && !self.press_blocked
            && match self.press_layer {
                Some(l) => l == layer,
                None => layer.order == Order::Background,
            }
    }

    /// Поднять окно наверх.
    pub fn raise_window(&mut self, id: Id) {
        self.window_order.retain(|w| *w != id);
        self.window_order.push(id);
    }

    pub fn menu_seen(&mut self, id: Id) {
        self.menus_seen.insert(id);
    }

    pub fn close_menus(&mut self) {
        self.close_menus = true;
    }

    pub fn add_popup_area(&mut self, r: Rect) {
        self.popup_areas.push(r);
    }

    pub fn set_cursor_icon(&mut self, icon: CursorIcon) {
        self.cursor_icon = icon;
    }

    fn request_tooltip(&mut self, id: Id, text: String, rect: Rect) {
        let now = self.input.time;
        if self.hover_since.is_none_or(|(h, _)| h != id) {
            self.hover_since = Some((id, now));
        }
        self.tooltip = Some((id, text, rect));
    }

    /// Взаимодействие с прямоугольником `rect` (обрезанным `clip`) в слое `layer`.
    #[allow(clippy::too_many_arguments)]
    pub fn interact(
        &mut self,
        ctx: &Ctx,
        layer: LayerId,
        clip: Rect,
        rect: Rect,
        id: Id,
        sense: Sense,
        enabled: bool,
    ) -> Response {
        let hit = rect.intersect(clip);
        let ptr = self.input.pointer;
        let over = ptr.is_some_and(|p| hit.contains(p)) && self.layer_hovered(layer);
        let hovered = enabled && over && self.active.is_none_or(|a| a == id);
        let mut pressed = false;
        if sense.interactive()
            && enabled
            && self.input.primary_pressed()
            && self.active.is_none()
            && self.input.interact_pos().is_some_and(|p| hit.contains(p))
            && self.layer_pressed(layer)
        {
            self.active = Some(id);
            pressed = true;
            if sense.focusable {
                self.focus = Some(id);
                self.focus_claimed = true;
            }
        }
        let is_active = self.active == Some(id);
        let mut clicked = sense.click
            && enabled
            && is_active
            && self.input.primary_released()
            && ptr.is_some_and(|p| hit.contains(p));
        if self.pending_click == Some(id) {
            self.pending_click = None;
            if enabled {
                clicked = true;
                if sense.focusable {
                    self.focus = Some(id);
                    self.focus_claimed = true;
                }
            }
        }
        let dragged = sense.drag && is_active && self.input.primary_down();
        let drag_delta = match (dragged, ptr, self.prev_pointer) {
            (true, Some(p), Some(q)) => p - q,
            _ => Vec2::ZERO,
        };
        let has_focus = self.focus == Some(id);
        if has_focus {
            self.focus_seen = true;
        }
        let lost_focus = self.lost_focus == Some(id);
        if lost_focus {
            self.lost_reported = true;
        }
        Response {
            id,
            rect,
            layer,
            hovered,
            pressed,
            clicked,
            dragged,
            drag_delta,
            changed: false,
            enabled,
            has_focus,
            lost_focus,
            interact_pointer_pos: if is_active { ptr } else { None },
            ctx: ctx.clone(),
        }
    }

    /// Снять фокус немедленно (поле ввода по Enter/Esc).
    pub fn surrender_focus(&mut self, id: Id) {
        if self.focus == Some(id) {
            self.focus = None;
            // Потеря уже сообщена самим виджетом в этом кадре.
            self.focus_at_begin = None;
        }
    }
}

/// Порядок вывода слоя: фон, окна (по `window_order`), меню, подсказки.
fn layer_rank(layer: LayerId, seq: usize, windows: &[Id]) -> (Order, usize) {
    let pos = match layer.order {
        Order::Middle => windows
            .iter()
            .position(|w| *w == layer.id)
            .unwrap_or(windows.len() + seq),
        _ => seq,
    };
    (layer.order, pos)
}

// ------------------------------------------------------------------ контекст

/// Итоги кадра для обвязки окна.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameOutput {
    /// Вид указателя мыши.
    pub cursor_icon: CursorIcon,
    /// Фокус в поле ввода — клавиатура не должна нажимать клавиши ЮТ-88.
    pub wants_keyboard_input: bool,
}

/// Контекст набора виджетов : дешёвый в копировании указатель на
/// общее состояние. Владеет холстом кадра.
#[derive(Clone)]
pub struct Ctx(Rc<RefCell<State>>);

impl Default for Ctx {
    fn default() -> Self {
        Ctx::new()
    }
}

impl std::fmt::Debug for Ctx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ctx")
    }
}

impl Ctx {
    /// Новый контекст: тёмная тема, буфер обмена в памяти (для окна замените на
    /// [`SystemClipboard`] через [`Ctx::set_clipboard`]).
    pub fn new() -> Self {
        Ctx(Rc::new(RefCell::new(State::new())))
    }

    pub(crate) fn st(&self) -> RefMut<'_, State> {
        self.0.borrow_mut()
    }

    fn st_ref(&self) -> Ref<'_, State> {
        self.0.borrow()
    }

    /// Начать кадр: принять ввод, подогнать холст под размер окна и очистить его.
    pub fn begin_frame(&self, input: &Input) {
        let mut st = self.st();
        let st = &mut *st;
        st.frame += 1;
        st.input = input.clone();
        let ppp = if input.ppp.is_finite() && input.ppp > 0.0 {
            input.ppp
        } else {
            1.0
        };
        let w = (input.screen_size.x * ppp).round().max(1.0) as u32;
        let h = (input.screen_size.y * ppp).round().max(1.0) as u32;
        st.canvas.resize(w, h);
        st.canvas.set_scale(ppp);
        st.canvas.clear(st.theme.panel_fill);
        st.screen = Rect::from_min_size(Pos2::ZERO, input.screen_size);
        st.available = st.screen;
        st.lists.clear();
        st.hover_layer = input.pointer.and_then(|p| st.layer_at(p));
        st.press_layer = input.interact_pos().and_then(|p| st.layer_at(p));
        st.press_blocked = input.primary_pressed()
            && !st.open_menus.is_empty()
            && st.press_layer.is_none_or(|l| l.order != Order::Foreground)
            && !input.interact_pos().is_some_and(|p| {
                st.prev_registry.widgets.iter().any(|w| {
                    matches!(w.kind, WidgetKind::MenuButton | WidgetKind::ComboBox)
                        && w.visible_rect.contains(p)
                })
            });
        st.focus_at_begin = st.focus;
        st.focus_claimed = false;
        st.focus_seen = false;
        st.lost_reported = false;
        st.text_focus = false;
        st.menus_seen.clear();
        st.close_menus = false;
        st.menu_button_pressed = false;
        st.popup_areas.clear();
        st.scroll_left = input.scroll;
        st.cursor_icon = CursorIcon::Default;
        st.tooltip = None;
        st.registry = Registry::default();
        // Нажатие поднимает окно, на которое пришлось.
        if input.primary_pressed()
            && !st.press_blocked
            && let Some(l) = st.press_layer
            && l.order == Order::Middle
        {
            st.raise_window(l.id);
        }
    }

    /// Закончить кадр: подсказка, вывод слоёв на холст, фокус и меню.
    pub fn end_frame(&self) -> FrameOutput {
        self.tooltip_layer();
        let mut st = self.st();
        let st = &mut *st;
        let esc = st.input.key_pressed(KeyCode::Escape);

        // Фокус.
        if st.input.primary_pressed() && !st.focus_claimed {
            st.focus = None;
        }
        if esc {
            st.focus = None;
        }
        if !st.focus_seen && st.focus == st.focus_at_begin {
            // Виджет с фокусом не рисовался (окно закрыто и т. п.).
            st.focus = None;
        }
        st.lost_focus = match st.focus_at_begin {
            Some(old) if st.focus != Some(old) => Some(old),
            _ => None,
        };
        st.wants_keyboard = st.text_focus && st.focus.is_some();

        // Меню.
        if st.close_menus || esc {
            st.open_menus.clear();
        }
        if st.input.primary_pressed()
            && !st.menu_button_pressed
            && let Some(p) = st.input.interact_pos()
            && !st.popup_areas.iter().any(|r| r.contains(p))
        {
            st.open_menus.clear();
        }
        if let Some(i) = st
            .open_menus
            .iter()
            .position(|m| !st.menus_seen.contains(m))
        {
            st.open_menus.truncate(i);
        }

        // Активный виджет отпускается вместе с кнопкой мыши.
        if !st.input.primary_down() {
            st.active = None;
        }
        st.pending_click = None;
        if st.tooltip.is_none() {
            st.hover_since = None;
        }
        st.prev_pointer = st.input.pointer;

        // Вывод слоёв по порядку.
        st.window_order
            .retain(|w| st.windows.get(w).is_some_and(|m| m.last_frame == st.frame));
        let mut order: Vec<usize> = (0..st.lists.len()).collect();
        let windows = st.window_order.clone();
        let lists = &st.lists;
        order.sort_by_key(|&i| layer_rank(lists[i].layer, i, &windows));
        let canvas = &mut st.canvas;
        let base = canvas.clip_depth();
        for &i in &order {
            let mut cur: Option<Rect> = None;
            for (clip, shape) in &lists[i].shapes {
                if cur != Some(*clip) {
                    if cur.is_some() {
                        canvas.pop_clip();
                    }
                    canvas.push_clip(*clip);
                    cur = Some(*clip);
                }
                shape.paint(canvas);
            }
            if cur.is_some() {
                canvas.pop_clip();
            }
        }
        debug_assert_eq!(canvas.clip_depth(), base);
        st.prev_areas = order
            .iter()
            .map(|&i| (lists[i].layer, lists[i].areas.clone()))
            .collect();
        st.prev_registry = std::mem::take(&mut st.registry);
        FrameOutput {
            cursor_icon: st.cursor_icon,
            wants_keyboard_input: st.wants_keyboard,
        }
    }

    /// Нарисовать подсказку, если мышь задержалась над виджетом.
    fn tooltip_layer(&self) {
        let (id, txt, rect) = {
            let st = self.st_ref();
            match (&st.tooltip, st.hover_since) {
                (Some((id, t, r)), Some((h, since)))
                    if *id == h && st.input.time - since >= st.theme.tooltip_delay =>
                {
                    (*id, t.clone(), *r)
                }
                _ => return,
            }
        };
        panel::show_tooltip(self, id, &txt, rect);
    }

    /// Холст кадра (для вывода в окно).
    pub fn with_canvas<R>(&self, f: impl FnOnce(&Canvas) -> R) -> R {
        f(&self.st_ref().canvas)
    }

    /// Холст кадра для изменения.
    pub fn with_canvas_mut<R>(&self, f: impl FnOnce(&mut Canvas) -> R) -> R {
        f(&mut self.st().canvas)
    }

    /// Ввод текущего кадра.
    pub fn input<R>(&self, f: impl FnOnce(&Input) -> R) -> R {
        f(&self.st_ref().input)
    }

    /// Номер кадра.
    pub fn frame_nr(&self) -> u64 {
        self.st_ref().frame
    }

    /// Весь экран (окно) в точках.
    pub fn screen_rect(&self) -> Rect {
        self.st_ref().screen
    }

    /// Свободная от панелей часть экрана.
    pub fn available_rect(&self) -> Rect {
        self.st_ref().available
    }

    /// Масштаб HiDPI.
    pub fn pixels_per_point(&self) -> f32 {
        self.st_ref().input.ppp
    }

    /// Тема.
    pub fn theme(&self) -> Rc<Theme> {
        self.st_ref().theme.clone()
    }

    /// Сменить тему.
    pub fn set_theme(&self, theme: Theme) {
        self.st().theme = Rc::new(theme);
    }

    /// Сменить буфер обмена.
    pub fn set_clipboard(&self, clipboard: Box<dyn Clipboard>) {
        self.st().clipboard = clipboard;
    }

    /// Вид указателя мыши на этот кадр.
    pub fn set_cursor_icon(&self, icon: CursorIcon) {
        self.st().cursor_icon = icon;
    }

    /// Клавиатура занята полем ввода (по итогам прошлого кадра).
    pub fn wants_keyboard_input(&self) -> bool {
        self.st_ref().wants_keyboard
    }

    /// Верхний слой в точке (по прямоугольникам прошлого кадра; `None` — ничего нет).
    pub fn layer_id_at(&self, pos: Pos2) -> Option<LayerId> {
        self.st_ref().layer_at(pos)
    }

    /// Мышь над окном, меню или подсказкой (не над фоном).
    pub fn is_pointer_over_area(&self) -> bool {
        let st = self.st_ref();
        st.input
            .pointer
            .and_then(|p| st.layer_at(p))
            .is_some_and(|l| l.order != Order::Background)
    }

    /// Нажатие мыши в этом кадре пришлось мимо открытого меню: оно только закрывает
    /// меню и не должно действовать на то, что под ним (рисунок компьютера и т. п.).
    pub fn press_blocked(&self) -> bool {
        self.st_ref().press_blocked
    }

    /// Открыто выпадающее меню или список.
    pub fn any_popup_open(&self) -> bool {
        !self.st_ref().open_menus.is_empty()
    }

    /// Закрыть все меню.
    pub fn close_menus(&self) {
        self.st().open_menus.clear();
    }

    /// Виджет в фокусе клавиатуры.
    pub fn focused(&self) -> Option<Id> {
        self.st_ref().focus
    }

    /// Дать фокус виджету.
    pub fn request_focus(&self, id: Id) {
        let mut st = self.st();
        st.focus = Some(id);
        st.focus_claimed = true;
    }

    /// Снять фокус.
    pub fn surrender_focus(&self) {
        self.st().focus = None;
    }

    /// Щёлкнуть виджет `id` в следующем кадре без мыши (как действие экранного диктора).
    pub fn queue_click(&self, id: Id) {
        self.st().pending_click = Some(id);
    }

    /// Виджеты последнего законченного кадра.
    pub fn registry(&self) -> Registry {
        self.st_ref().prev_registry.clone()
    }

    /// Поднять окно с заголовком/id наверх.
    pub fn move_to_top(&self, window: Id) {
        self.st().raise_window(window);
    }
}
