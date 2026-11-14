//! Тестовый стенд: кадры интерфейса без окна, поиск виджетов по подписи, щелчки и
//! клавиши. Основа тестов окна эмулятора.
//!
//! ```ignore
//! let mut h = Harness::new(vec2(800.0, 600.0), state, |ctx, s| build(ctx, s));
//! h.click("Загрузить");
//! h.type_text("C100");
//! h.key(KeyCode::Enter);
//! assert!(h.has("Готово"));
//! ```

use crate::gfx::{Pos2, Vec2};

use super::registry::{Registry, WidgetInfo};
use super::{Ctx, FrameOutput, Input, KeyCode, MemoryClipboard, Modifiers, PointerButton};

/// Функция-построитель кадра.
type Build<S> = Box<dyn FnMut(&Ctx, &mut S)>;

/// Стенд: состояние `S` и функция, рисующая по нему кадр.
pub struct Harness<S> {
    /// Контекст виджетов.
    pub ctx: Ctx,
    /// Состояние приложения.
    pub state: S,
    build: Build<S>,
    input: Input,
    clipboard: MemoryClipboard,
    /// Итоги последнего кадра.
    pub output: FrameOutput,
    /// Длительность кадра, с.
    pub dt: f64,
}

impl<S> Harness<S> {
    /// Стенд с окном `size` точек; сразу прогоняет два кадра (раскладка устанавливается
    /// по прошлому кадру).
    pub fn new(size: Vec2, state: S, build: impl FnMut(&Ctx, &mut S) + 'static) -> Self {
        Self::with_ppp(size, 1.0, state, build)
    }

    /// То же с масштабом HiDPI.
    pub fn with_ppp(
        size: Vec2,
        ppp: f32,
        state: S,
        build: impl FnMut(&Ctx, &mut S) + 'static,
    ) -> Self {
        let ctx = Ctx::new();
        let clipboard = MemoryClipboard::new();
        ctx.set_clipboard(Box::new(clipboard.clone()));
        let mut input = Input::new(size);
        input.ppp = ppp;
        let mut h = Harness {
            ctx,
            state,
            build: Box::new(build),
            input,
            clipboard,
            output: FrameOutput::default(),
            dt: 1.0 / 60.0,
        };
        h.run(2);
        h
    }

    /// Один кадр с накопленными событиями.
    pub fn step(&mut self) {
        self.ctx.begin_frame(&self.input);
        (self.build)(&self.ctx, &mut self.state);
        self.output = self.ctx.end_frame();
        self.input.next_frame();
        self.input.time += self.dt;
    }

    /// `n` кадров.
    pub fn run(&mut self, n: usize) {
        for _ in 0..n {
            self.step();
        }
    }

    /// Два кадра: обработать события и дорисовать следствия.
    pub fn settle(&mut self) {
        self.run(2);
    }

    /// Ввод следующего кадра (для своих событий).
    pub fn input_mut(&mut self) -> &mut Input {
        &mut self.input
    }

    /// Буфер обмена стенда.
    pub fn clipboard(&self) -> &MemoryClipboard {
        &self.clipboard
    }

    /// Виджеты последнего кадра.
    pub fn registry(&self) -> Registry {
        self.ctx.registry()
    }

    /// Есть виджет с подписью `label`.
    pub fn has(&self, label: &str) -> bool {
        self.registry().has(label)
    }

    /// Есть виджет, подпись которого содержит `part`.
    pub fn has_containing(&self, part: &str) -> bool {
        self.registry().has_containing(part)
    }

    /// Единственный виджет с подписью `label` (паника, если нет или несколько).
    pub fn get(&self, label: &str) -> WidgetInfo {
        self.registry().get_by_label(label).clone()
    }

    /// Все виджеты с подписью `label`.
    pub fn get_all(&self, label: &str) -> Vec<WidgetInfo> {
        self.registry().all_by_label(label).cloned().collect()
    }

    /// Первый виджет со значением `value` (поле ввода с этим текстом и т. п.).
    pub fn get_by_value(&self, value: &str) -> WidgetInfo {
        self.registry()
            .all_by_value(value)
            .next()
            .cloned()
            .unwrap_or_else(|| panic!("нет виджета со значением «{value}»"))
    }

    /// Переместить мышь (один кадр).
    pub fn move_to(&mut self, pos: Pos2) {
        self.input.pointer = Some(pos);
        self.step();
    }

    /// Нажать основную кнопку мыши в точке (кадр) и отпустить (ещё кадр).
    pub fn click_at(&mut self, pos: Pos2) {
        self.input.pointer = Some(pos);
        self.input.pointer_button(PointerButton::Primary, true);
        self.step();
        self.input.pointer_button(PointerButton::Primary, false);
        self.step();
    }

    /// Протащить мышь с нажатой кнопкой из `from` в `to` (за `steps` кадров).
    pub fn drag(&mut self, from: Pos2, to: Pos2, steps: usize) {
        self.input.pointer = Some(from);
        self.step();
        self.input.pointer_button(PointerButton::Primary, true);
        self.step();
        let n = steps.max(1);
        for i in 1..=n {
            self.input.pointer = Some(from.lerp(to, i as f32 / n as f32));
            self.step();
        }
        self.input.pointer_button(PointerButton::Primary, false);
        self.step();
    }

    /// Прокрутить колесом над точкой (`delta.y > 0` — вверх).
    pub fn scroll_at(&mut self, pos: Pos2, delta: Vec2) {
        self.input.pointer = Some(pos);
        self.input.scroll = delta;
        self.step();
    }

    /// Щёлкнуть виджет: мышью по центру видимой части, если он там верхний; иначе
    /// (прокручен за край, перекрыт) — «без мыши», как действие AccessKit.
    pub fn click_widget(&mut self, w: &WidgetInfo) {
        let p = Registry::click_point(w);
        let on_top = p.is_some_and(|p| self.ctx.layer_id_at(p).is_none_or(|l| l == w.layer));
        match p {
            Some(p) if on_top => self.click_at(p),
            _ => {
                self.ctx.queue_click(w.id);
                self.step();
            }
        }
        self.settle();
    }

    /// Щёлкнуть единственный виджет с подписью `label`.
    pub fn click(&mut self, label: &str) {
        let w = self.get(label);
        self.click_widget(&w);
    }

    /// Щёлкнуть `n`-й (по порядку кадра) виджет с подписью `label`.
    pub fn click_nth(&mut self, label: &str, n: usize) {
        let w = self
            .get_all(label)
            .into_iter()
            .nth(n)
            .unwrap_or_else(|| panic!("нет {n}-го «{label}»"));
        self.click_widget(&w);
    }

    /// Закрыть окно `title` крестиком.
    pub fn close_window(&mut self, title: &str) {
        self.click(&format!("Закрыть «{title}»"));
    }

    /// Дать фокус клавиатуры виджету с подписью `label` (без мыши).
    pub fn focus(&mut self, label: &str) {
        let w = self.get(label);
        self.ctx.request_focus(w.id);
        self.step();
    }

    /// Нажать и отпустить клавишу (кадр на нажатие, кадр на отпускание).
    pub fn key(&mut self, key: KeyCode) {
        self.key_mods(Modifiers::NONE, key);
    }

    /// Нажать и отпустить клавишу с модификаторами.
    pub fn key_mods(&mut self, mods: Modifiers, key: KeyCode) {
        let old = self.input.modifiers;
        self.input.modifiers = mods;
        self.input.key(key, true, false, None);
        self.step();
        self.input.key(key, false, false, None);
        self.step();
        self.input.modifiers = old;
    }

    /// Нажать клавишу (без отпускания) — событие уйдёт в следующем кадре.
    pub fn key_down(&mut self, key: KeyCode) {
        self.input.key(key, true, false, None);
    }

    /// Отпустить клавишу — событие уйдёт в следующем кадре.
    pub fn key_up(&mut self, key: KeyCode) {
        self.input.key(key, false, false, None);
    }

    /// Набрать текст (один кадр).
    pub fn type_text(&mut self, s: &str) {
        self.input.text(s);
        self.step();
    }

    /// Пиксель холста последнего кадра (физические координаты).
    pub fn pixel(&self, x: u32, y: u32) -> crate::gfx::Color {
        self.ctx.with_canvas(|c| c.pixel(x, y))
    }
}
