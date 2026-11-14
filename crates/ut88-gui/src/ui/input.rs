//! Ввод кадра: снимок событий мыши и клавиатуры, не зависящий от окна.
//!
//! Обвязка окна (winit) переводит свои события в [`Input`] и передаёт его в
//! [`Ctx::begin_frame`](super::Ctx::begin_frame). Тесты собирают [`Input`] сами — окно не
//! нужно.

use crate::gfx::{Pos2, Vec2, vec2};

pub use winit::keyboard::KeyCode;

/// Кнопка мыши.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerButton {
    /// Левая (основная).
    Primary = 0,
    /// Правая.
    Secondary = 1,
    /// Средняя.
    Middle = 2,
}

/// Состояние клавиш-модификаторов.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    /// Shift.
    pub shift: bool,
    /// Ctrl.
    pub ctrl: bool,
    /// Alt.
    pub alt: bool,
    /// Клавиша «Windows»/Super.
    pub logo: bool,
}

impl Modifiers {
    /// Ничего не нажато.
    pub const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
        logo: false,
    };
    /// Только Shift.
    pub const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    /// Только Ctrl.
    pub const CTRL: Modifiers = Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    };

    /// Нажат хотя бы один модификатор.
    pub fn any(self) -> bool {
        self.shift || self.ctrl || self.alt || self.logo
    }
}

/// Нажатие или отпускание клавиши.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    /// Физический код клавиши (не зависит от раскладки).
    pub key: KeyCode,
    /// `true` — нажата, `false` — отпущена.
    pub pressed: bool,
    /// Автоповтор удерживаемой клавиши.
    pub repeat: bool,
    /// Текст, который даёт клавиша в текущей раскладке (`KeyEvent::text` winit), если есть.
    pub text: Option<String>,
    /// Модификаторы в момент события.
    pub modifiers: Modifiers,
}

/// Событие кадра (в порядке поступления).
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Клавиша.
    Key(KeyEvent),
    /// Введённый текст (печатные символы для полей ввода; без управляющих символов).
    Text(String),
    /// Окно получило (`true`) или потеряло (`false`) фокус ввода.
    WindowFocused(bool),
}

/// Снимок ввода за кадр.
///
/// Координаты — логические точки окна. Состояние мыши дано итогом кадра: где она
/// сейчас, какие кнопки удерживаются, и были ли за кадр нажатия/отпускания (если кнопку
/// нажали и отпустили в пределах одного кадра, верны оба флага — это щелчок).
#[derive(Debug, Clone)]
pub struct Input {
    /// Позиция мыши (`None` — мышь вне окна).
    pub pointer: Option<Pos2>,
    /// Кнопки, удерживаемые в конце кадра (индекс — [`PointerButton`]).
    pub down: [bool; 3],
    /// Кнопки, нажатые за кадр.
    pub pressed: [bool; 3],
    /// Кнопки, отпущенные за кадр.
    pub released: [bool; 3],
    /// Где была мышь при нажатии основной кнопки за кадр (если нажималась).
    pub press_origin: Option<Pos2>,
    /// Прокрутка колесом за кадр в точках; `y > 0` — колесо «от себя» (содержимое
    /// сдвигается вниз, видно то, что выше).
    pub scroll: Vec2,
    /// События клавиатуры и текста по порядку.
    pub events: Vec<Event>,
    /// Модификаторы в конце кадра.
    pub modifiers: Modifiers,
    /// Окно в фокусе ввода.
    pub focused: bool,
    /// Размер окна в логических точках.
    pub screen_size: Vec2,
    /// Физических пикселей в точке (HiDPI).
    pub ppp: f32,
    /// Время начала кадра, с (монотонное, от любого начала отсчёта).
    pub time: f64,
    /// Длительность прошлого кадра, с.
    pub dt: f32,
}

impl Default for Input {
    fn default() -> Self {
        Input {
            pointer: None,
            down: [false; 3],
            pressed: [false; 3],
            released: [false; 3],
            press_origin: None,
            scroll: Vec2::ZERO,
            events: Vec::new(),
            modifiers: Modifiers::NONE,
            focused: true,
            screen_size: vec2(800.0, 600.0),
            ppp: 1.0,
            time: 0.0,
            dt: 1.0 / 60.0,
        }
    }
}

impl Input {
    /// Ввод для окна размером `size` точек.
    pub fn new(size: Vec2) -> Self {
        Input {
            screen_size: size,
            ..Input::default()
        }
    }

    /// Позиция мыши.
    pub fn pointer_pos(&self) -> Option<Pos2> {
        self.pointer
    }

    /// Позиция мыши для взаимодействия: при нажатии за кадр — точка нажатия.
    pub fn interact_pos(&self) -> Option<Pos2> {
        if self.pressed[0] {
            self.press_origin.or(self.pointer)
        } else {
            self.pointer
        }
    }

    /// Основная кнопка удерживается.
    pub fn primary_down(&self) -> bool {
        self.down[0]
    }

    /// Основная кнопка нажата за кадр.
    pub fn primary_pressed(&self) -> bool {
        self.pressed[0]
    }

    /// Основная кнопка отпущена за кадр.
    pub fn primary_released(&self) -> bool {
        self.released[0]
    }

    /// Клавиша нажата за кадр (с автоповтором).
    pub fn key_pressed(&self, key: KeyCode) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, Event::Key(k) if k.key == key && k.pressed))
    }

    /// Клавиша нажата за кадр без модификаторов (с автоповтором).
    pub fn key_pressed_plain(&self, key: KeyCode) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, Event::Key(k) if k.key == key && k.pressed && !k.modifiers.any()))
    }

    /// Клавиша отпущена за кадр.
    pub fn key_released(&self, key: KeyCode) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, Event::Key(k) if k.key == key && !k.pressed))
    }

    /// Нажатия и отпускания клавиш за кадр.
    pub fn key_events(&self) -> impl Iterator<Item = &KeyEvent> {
        self.events.iter().filter_map(|e| match e {
            Event::Key(k) => Some(k),
            _ => None,
        })
    }

    /// Окно потеряло фокус за кадр.
    pub fn focus_lost(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e, Event::WindowFocused(false)))
    }

    /// Начать следующий кадр: убрать одноразовые события (нажатия, отпускания, колесо,
    /// клавиши), сохранив позицию мыши, удерживаемые кнопки и модификаторы.
    pub fn next_frame(&mut self) {
        self.pressed = [false; 3];
        self.released = [false; 3];
        self.press_origin = None;
        self.scroll = Vec2::ZERO;
        self.events.clear();
    }

    /// Записать нажатие (`down = true`) или отпускание кнопки мыши.
    pub fn pointer_button(&mut self, button: PointerButton, down: bool) {
        let i = button as usize;
        if down {
            self.pressed[i] = true;
            if i == 0 && self.press_origin.is_none() {
                self.press_origin = self.pointer;
            }
        } else {
            self.released[i] = true;
        }
        self.down[i] = down;
    }

    /// Записать событие клавиши (модификаторы берутся из [`Input::modifiers`]).
    pub fn key(&mut self, key: KeyCode, pressed: bool, repeat: bool, text: Option<String>) {
        self.events.push(Event::Key(KeyEvent {
            key,
            pressed,
            repeat,
            text,
            modifiers: self.modifiers,
        }));
    }

    /// Записать введённый текст (управляющие символы отбрасываются).
    pub fn text(&mut self, s: &str) {
        let s: String = s.chars().filter(|c| !c.is_control()).collect();
        if !s.is_empty() {
            self.events.push(Event::Text(s));
        }
    }
}
