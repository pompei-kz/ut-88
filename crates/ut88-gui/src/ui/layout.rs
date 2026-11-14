//! [`Ui`] — раскладка виджетов в прямоугольнике: столбец, ряд, ряд с переносом,
//! колонки и таблица [`Grid`].

use std::hash::Hash;
use std::rc::Rc;

use crate::gfx::{Canvas, Color, Pos2, Rect, Vec2, pos2, vec2};

use super::registry::{WidgetInfo, WidgetKind};
use super::{
    Ctx, GridMem, Id, InnerResponse, Input, LayerId, Response, Sense, Shape, ShapeIdx, Theme,
};

/// Направление раскладки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dir {
    /// Сверху вниз.
    Vertical,
    /// Слева направо; `wrap` — переносить на новую строку, когда не помещается.
    Horizontal { wrap: bool },
}

/// Где находится `Ui` относительно меню.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MenuCtx {
    /// Строка меню.
    pub bar: bool,
    /// Глубина выпадающего меню (0 — не в меню).
    pub depth: usize,
    /// Выпадающий список `ComboBox`.
    pub combo: bool,
}

/// Таблица в процессе раскладки.
struct GridState {
    id: Id,
    striped: bool,
    col: usize,
    row: usize,
    row_top: f32,
    row_h: f32,
    x0: f32,
    prev: GridMem,
    widths: Vec<f32>,
    heights: Vec<f32>,
    stripe: Option<ShapeIdx>,
}

impl GridState {
    fn col_w(&self, c: usize) -> f32 {
        let p = self.prev.widths.get(c).copied().unwrap_or(0.0);
        let w = self.widths.get(c).copied().unwrap_or(0.0);
        p.max(w)
    }

    fn col_x(&self, c: usize, spacing: f32) -> f32 {
        self.x0 + (0..c).map(|i| self.col_w(i) + spacing).sum::<f32>()
    }

    fn total_w(&self, spacing: f32) -> f32 {
        let n = self.prev.widths.len().max(self.widths.len());
        if n == 0 {
            return 0.0;
        }
        self.col_x(n - 1, spacing) + self.col_w(n - 1) - self.x0
    }

    fn row_id(&self) -> Id {
        self.id.with(("ряд", self.row))
    }
}

/// Область раскладки виджетов.
pub struct Ui {
    pub(crate) ctx: Ctx,
    pub(crate) theme: Rc<Theme>,
    pub(crate) ppp: f32,
    id: Id,
    layer: LayerId,
    max_rect: Rect,
    clip: Rect,
    cursor: Pos2,
    min_rect: Rect,
    dir: Dir,
    grid: Option<GridState>,
    row_h: f32,
    spacing: Vec2,
    counter: u64,
    edits: u64,
    labels: u64,
    pub(crate) enabled: bool,
    row: Option<Id>,
    pub(crate) menu: MenuCtx,
    /// Отложенные горизонтальные разделители: место фигуры, Y, цвет, толщина.
    seps: Vec<(ShapeIdx, f32, Color, f32)>,
    placed_any: bool,
    /// Переносить подписи и в рядах (содержимое плавающих окон): подпись занимает
    /// остаток ряда и переносится по словам.
    pub(crate) wrap_rows: bool,
}

impl Ui {
    /// Корневая область слоя (панель, окно, меню): столбец в `max_rect`.
    pub(crate) fn new_root(ctx: &Ctx, id: Id, layer: LayerId, max_rect: Rect, clip: Rect) -> Ui {
        let (theme, ppp) = {
            let st = ctx.st();
            (st.theme.clone(), st.input.ppp)
        };
        let row_h = theme.interact_height;
        let spacing = theme.item_spacing;
        Ui {
            ctx: ctx.clone(),
            theme,
            ppp: if ppp > 0.0 { ppp } else { 1.0 },
            id,
            layer,
            max_rect,
            clip,
            cursor: max_rect.min,
            min_rect: Rect::from_min_size(max_rect.min, Vec2::ZERO),
            dir: Dir::Vertical,
            grid: None,
            row_h,
            spacing,
            counter: 0,
            edits: 0,
            labels: 0,
            enabled: true,
            row: None,
            menu: MenuCtx::default(),
            seps: Vec::new(),
            placed_any: false,
            wrap_rows: false,
        }
    }

    /// Дочерняя область, начинающаяся там, где встал бы следующий виджет.
    pub(crate) fn child(&mut self, dir: Dir) -> Ui {
        let pos = self.next_pos();
        self.counter += 1;
        let id = self.id.with(("ребёнок", self.counter));
        let row = match dir {
            Dir::Horizontal { .. } if self.grid.is_none() => Some(id),
            _ => self.cur_row(),
        };
        let min_h = match dir {
            Dir::Horizontal { .. } => self.row_h,
            Dir::Vertical => 0.0,
        };
        Ui {
            ctx: self.ctx.clone(),
            theme: self.theme.clone(),
            ppp: self.ppp,
            id,
            layer: self.layer,
            max_rect: Rect::from_min_max(pos, self.max_rect.max.max(pos)),
            clip: self.clip,
            cursor: pos,
            min_rect: Rect::from_min_size(pos, vec2(0.0, min_h)),
            dir,
            grid: None,
            row_h: self.row_h,
            spacing: self.spacing,
            counter: 0,
            edits: 0,
            labels: 0,
            enabled: self.enabled,
            row,
            menu: self.menu,
            seps: Vec::new(),
            placed_any: false,
            wrap_rows: self.wrap_rows,
        }
    }

    /// Задать `max_rect`, отсечение и начальную занятую ширину (для окон и меню фиксированной
    /// ширины, колонок).
    pub(crate) fn set_bounds(&mut self, max_rect: Rect, clip: Rect, min_width: f32) {
        self.max_rect = max_rect;
        self.clip = clip;
        self.cursor = max_rect.min;
        self.min_rect = Rect::from_min_size(max_rect.min, vec2(min_width, 0.0));
    }

    /// Закончить раскладку: дорисовать разделители на всю ширину. Возвращает занятый
    /// прямоугольник.
    pub(crate) fn finish(&mut self) -> Rect {
        let r = self.min_rect;
        let seps = std::mem::take(&mut self.seps);
        if !seps.is_empty() {
            let mut st = self.ctx.st();
            for (at, y, color, w) in seps {
                st.set_shape(
                    at,
                    Shape::Line {
                        a: pos2(r.min.x, y),
                        b: pos2(r.max.x, y),
                        width: w,
                        color,
                    },
                );
            }
        }
        r
    }

    // ---------------------------------------------------------------- свойства

    /// Контекст.
    pub fn ctx(&self) -> &Ctx {
        &self.ctx
    }

    /// Тема.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Id области.
    pub fn id(&self) -> Id {
        self.id
    }

    /// Слой.
    pub fn layer_id(&self) -> LayerId {
        self.layer
    }

    /// Отсечение.
    pub fn clip_rect(&self) -> Rect {
        self.clip
    }

    /// Предельный прямоугольник раскладки.
    pub fn max_rect(&self) -> Rect {
        self.max_rect
    }

    /// Занятый виджетами прямоугольник.
    pub fn min_rect(&self) -> Rect {
        self.min_rect
    }

    /// Свободное место от курсора до края.
    pub fn available_rect_before_wrap(&self) -> Rect {
        let p = self.next_pos();
        Rect::from_min_max(p, self.max_rect.max.max(p))
    }

    /// Свободная ширина.
    pub fn available_width(&self) -> f32 {
        (self.max_rect.max.x - self.next_pos().x).max(0.0)
    }

    /// Свободный размер.
    pub fn available_size(&self) -> Vec2 {
        self.available_rect_before_wrap().size()
    }

    /// Виджеты доступны.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Сделать недоступными все следующие виджеты области.
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Доступность следующих виджетов (не может включить, если область выключена
    /// родителем).
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = self.enabled && enabled;
    }

    /// Ввод кадра.
    pub fn input<R>(&self, f: impl FnOnce(&Input) -> R) -> R {
        self.ctx.input(f)
    }

    /// Следующий автоматический id (стабилен, пока порядок виджетов не меняется).
    pub fn next_auto_id(&mut self) -> Id {
        self.counter += 1;
        self.id.with(("авто", self.counter))
    }

    /// Id поля ввода: свой счётчик, чтобы появление других виджетов не сбивало фокус.
    pub(crate) fn next_edit_id(&mut self) -> Id {
        self.edits += 1;
        self.id.with(("поле", self.edits))
    }

    /// Id подписи (свой счётчик).
    pub(crate) fn next_label_id(&mut self) -> Id {
        self.labels += 1;
        self.id.with(("подпись", self.labels))
    }

    /// Промежуток между виджетами.
    pub fn item_spacing(&self) -> Vec2 {
        self.spacing
    }

    /// Изменить промежуток между виджетами (для этой области и вложенных).
    pub fn set_item_spacing(&mut self, spacing: Vec2) {
        self.spacing = spacing;
    }

    /// Id из соли, постоянный для этой области.
    pub fn make_persistent_id(&self, salt: impl Hash) -> Id {
        self.id.with(salt)
    }

    /// Раскладка — ряд.
    pub(crate) fn is_horizontal(&self) -> bool {
        matches!(self.dir, Dir::Horizontal { .. })
    }

    /// Ширина, по которой переносится по словам подпись шириной `text_w` в одну строку;
    /// `None` — не переносить. В столбце вне таблицы — по свободной ширине. В ряду (только
    /// при [`Ui::wrap_rows`]) — по остатку ряда; в ряду с переносом подпись, не
    /// помещающаяся в остаток, уходит на новую строку и переносится по всей ширине.
    pub(crate) fn text_wrap_width(&self, text_w: f32) -> Option<f32> {
        if self.grid.is_some() {
            return None;
        }
        let rest = self.available_width().max(1.0);
        match self.dir {
            Dir::Vertical => Some(rest),
            Dir::Horizontal { .. } if !self.wrap_rows => None,
            Dir::Horizontal { wrap: true } if self.placed_any && text_w > rest => {
                Some(self.max_rect.width().max(1.0))
            }
            Dir::Horizontal { .. } => Some(rest),
        }
    }

    fn cur_row(&self) -> Option<Id> {
        match &self.grid {
            Some(g) => Some(g.row_id()),
            None => self.row,
        }
    }

    // ---------------------------------------------------------------- раскладка

    /// Где встанет левый верхний угол следующего виджета.
    pub(crate) fn next_pos(&self) -> Pos2 {
        match &self.grid {
            Some(g) => pos2(g.col_x(g.col, self.spacing.x), g.row_top),
            None => self.cursor,
        }
    }

    /// Прямоугольник для виджета размера `size` (ещё не занят).
    fn place(&mut self, size: Vec2) -> Rect {
        let sp = self.spacing;
        if let Some(g) = &self.grid {
            let h = g
                .prev
                .heights
                .get(g.row)
                .copied()
                .unwrap_or(0.0)
                .max(self.row_h);
            let x = g.col_x(g.col, sp.x);
            let y = g.row_top + ((h - size.y) * 0.5).max(0.0);
            return Rect::from_min_size(pos2(x, y), size);
        }
        match self.dir {
            Dir::Vertical => Rect::from_min_size(self.cursor, size),
            Dir::Horizontal { wrap } => {
                if wrap && self.placed_any && self.cursor.x + size.x > self.max_rect.max.x {
                    self.cursor.x = self.max_rect.min.x;
                    self.cursor.y = self.min_rect.max.y + sp.y;
                }
                let y = self.cursor.y + ((self.row_h - size.y) * 0.5).max(0.0);
                Rect::from_min_size(pos2(self.cursor.x, y), size)
            }
        }
    }

    /// Занять прямоугольник `rect` (сдвинуть курсор).
    pub(crate) fn advance(&mut self, rect: Rect) {
        let sp = self.spacing;
        self.min_rect = self.min_rect.union(rect);
        self.placed_any = true;
        if let Some(g) = &mut self.grid {
            if g.widths.len() <= g.col {
                g.widths.resize(g.col + 1, 0.0);
            }
            g.widths[g.col] = g.widths[g.col].max(rect.width());
            g.row_h = g.row_h.max(rect.max.y - g.row_top);
            g.col += 1;
            return;
        }
        match self.dir {
            Dir::Vertical => self.cursor.y = rect.max.y + sp.y,
            Dir::Horizontal { .. } => self.cursor.x = rect.max.x + sp.x,
        }
    }

    /// Занять место размера `size`; возвращает прямоугольник.
    pub fn allocate_space(&mut self, size: Vec2) -> Rect {
        let r = self.place(size);
        self.advance(r);
        r
    }

    /// Занять место и получить ответ на взаимодействие.
    pub fn allocate_exact_size(&mut self, size: Vec2, sense: Sense) -> (Rect, Response) {
        let r = self.allocate_space(size);
        let id = self.next_auto_id();
        (r, self.interact(r, id, sense))
    }

    /// Занять заданный прямоугольник (например, всё свободное место центральной панели).
    pub fn allocate_rect(&mut self, rect: Rect, sense: Sense) -> Response {
        self.advance(rect);
        let id = self.next_auto_id();
        self.interact(rect, id, sense)
    }

    /// Пустой промежуток.
    pub fn add_space(&mut self, amount: f32) {
        match self.dir {
            Dir::Vertical => self.cursor.y += amount,
            Dir::Horizontal { .. } => self.cursor.x += amount,
        }
    }

    /// Взаимодействие с прямоугольником (учитывает слой под мышью и доступность).
    pub fn interact(&self, rect: Rect, id: Id, sense: Sense) -> Response {
        let mut st = self.ctx.st();
        let r = st.interact(
            &self.ctx,
            self.layer,
            self.clip,
            rect,
            id,
            sense,
            self.enabled,
        );
        // Наведение на пункт меню закрывает более глубокие вложенные меню.
        if self.menu.depth > 0 && r.hovered && (sense.click || sense.drag) {
            st.open_menus.truncate(self.menu.depth);
        }
        r
    }

    /// Записать виджет в реестр кадра.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn register(
        &self,
        id: Id,
        kind: WidgetKind,
        label: &str,
        value: Option<String>,
        selected: Option<bool>,
        rect: Rect,
        enabled: bool,
    ) {
        let info = WidgetInfo {
            id,
            kind,
            label: label.to_owned(),
            value,
            selected,
            rect,
            visible_rect: rect.intersect(self.clip),
            enabled,
            layer: self.layer,
            row: self.cur_row(),
            hover_text: None,
        };
        self.ctx.st().registry.widgets.push(info);
    }

    // ---------------------------------------------------------------- рисование

    /// Добавить фигуру в слой (с текущим отсечением).
    pub fn paint(&self, shape: Shape) -> ShapeIdx {
        self.ctx.st().add_shape(self.layer, self.clip, shape)
    }

    /// Зарезервировать место под фигуру, которая станет известна позже (фон).
    pub fn reserve_shape(&self) -> ShapeIdx {
        self.paint(Shape::Noop)
    }

    /// Заменить зарезервированную фигуру.
    pub fn set_shape(&self, at: ShapeIdx, shape: Shape) {
        self.ctx.st().set_shape(at, shape);
    }

    /// Рисовать прямо на холсте в прямоугольнике `rect` (отсекается). Рисунок ложится
    /// **под** все записанные фигуры кадра — годится для фона центральной панели.
    /// Внутри `f` нельзя обращаться к [`Ctx`].
    pub fn canvas_at<R>(&self, rect: Rect, f: impl FnOnce(&mut Canvas) -> R) -> R {
        let clip = rect.intersect(self.clip);
        let mut st = self.ctx.st();
        st.canvas.push_clip(clip);
        let r = f(&mut st.canvas);
        st.canvas.pop_clip();
        r
    }

    /// Горизонтальный разделитель на всю ширину области (дорисовывается в
    /// [`Ui::finish`]) или вертикальный в ряду.
    pub(crate) fn paint_separator(&mut self) -> Response {
        let sp = self.theme.separator_spacing;
        let (w, color) = self.theme.noninteractive.bg_stroke;
        if self.is_horizontal() {
            let r = self.allocate_space(vec2(sp, self.row_h));
            let x = r.center().x;
            self.paint(Shape::Line {
                a: pos2(x, r.min.y),
                b: pos2(x, r.max.y),
                width: w,
                color,
            });
            let id = self.next_auto_id();
            self.interact(r, id, Sense::hover())
        } else {
            let r = self.allocate_space(vec2(0.0, sp));
            let at = self.reserve_shape();
            self.seps.push((at, r.center().y, color, w));
            let id = self.next_auto_id();
            self.interact(r, id, Sense::hover())
        }
    }

    // ---------------------------------------------------------------- контейнеры

    fn scope_dir<R>(&mut self, dir: Dir, f: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let mut child = self.child(dir);
        let inner = f(&mut child);
        let rect = child.finish();
        self.advance(rect);
        let response = self.interact(rect, child.id, Sense::hover());
        InnerResponse { inner, response }
    }

    /// Ряд слева направо.
    pub fn horizontal<R>(&mut self, f: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        self.scope_dir(Dir::Horizontal { wrap: false }, f)
    }

    /// Ряд с переносом на новую строку.
    pub fn horizontal_wrapped<R>(&mut self, f: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        self.scope_dir(Dir::Horizontal { wrap: true }, f)
    }

    /// Столбец сверху вниз.
    pub fn vertical<R>(&mut self, f: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        self.scope_dir(Dir::Vertical, f)
    }

    /// Вложенная область с той же раскладкой (для временного `set_enabled`).
    pub fn scope<R>(&mut self, f: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let dir = self.dir;
        self.scope_dir(dir, f)
    }

    /// Вложенная область, доступная только при `enabled`.
    pub fn add_enabled_ui<R>(
        &mut self,
        enabled: bool,
        f: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        self.scope(|ui| {
            ui.set_enabled(enabled);
            f(ui)
        })
    }

    /// `n` колонок равной ширины.
    pub fn columns<R>(&mut self, n: usize, f: impl FnOnce(&mut [Ui]) -> R) -> R {
        let n = n.max(1);
        let pos = self.next_pos();
        let sp = self.spacing.x;
        let avail = (self.max_rect.max.x - pos.x).max(0.0);
        let w = ((avail - sp * (n - 1) as f32) / n as f32).max(0.0);
        let mut cols: Vec<Ui> = (0..n)
            .map(|i| {
                let mut c = self.child(Dir::Vertical);
                let x = pos.x + i as f32 * (w + sp);
                let r = Rect::from_min_max(pos2(x, pos.y), pos2(x + w, self.max_rect.max.y));
                c.set_bounds(r, self.clip, w);
                c
            })
            .collect();
        let inner = f(&mut cols);
        let rect = cols
            .iter_mut()
            .map(|c| c.finish())
            .fold(Rect::from_min_size(pos, Vec2::ZERO), |a, b| a.union(b));
        self.advance(rect);
        inner
    }

    /// Закончить строку таблицы ([`Grid`]); вне таблицы ничего не делает.
    pub fn end_row(&mut self) {
        let sp = self.spacing;
        let fill = self.theme.faint_bg;
        let row_h = self.row_h;
        let Some(g) = &mut self.grid else {
            return;
        };
        let h = g.row_h.max(row_h);
        if g.heights.len() <= g.row {
            g.heights.resize(g.row + 1, 0.0);
        }
        g.heights[g.row] = h;
        let stripe = g.stripe.take();
        let paint_stripe = g.striped && g.row % 2 == 1;
        let total = g.total_w(sp.x);
        let (x0, top) = (g.x0, g.row_top);
        g.row_top += h + sp.y;
        g.row += 1;
        g.col = 0;
        g.row_h = 0.0;
        let striped = g.striped;
        if let Some(at) = stripe
            && paint_stripe
        {
            let r = Rect::from_min_max(
                pos2(x0 - sp.x * 0.25, top - sp.y * 0.5),
                pos2(x0 + total + sp.x * 0.25, top + h + sp.y * 0.5),
            );
            self.set_shape(
                at,
                Shape::Rect {
                    rect: r,
                    radius: 0.0,
                    fill,
                    stroke: (0.0, Color::TRANSPARENT),
                },
            );
        }
        if striped {
            let at = self.reserve_shape();
            if let Some(g) = &mut self.grid {
                g.stripe = Some(at);
            }
        }
    }
}

/// Таблица: виджеты по столбцам, строки заканчиваются [`Ui::end_row`].
/// Ширины столбцов берутся из прошлого кадра.
pub struct Grid {
    salt: Id,
    striped: bool,
}

impl Grid {
    /// Таблица с солью id.
    pub fn new(id_salt: impl Hash) -> Self {
        Grid {
            salt: Id::new(id_salt),
            striped: false,
        }
    }

    /// Полосы через строку.
    pub fn striped(mut self, striped: bool) -> Self {
        self.striped = striped;
        self
    }

    /// Число столбцов (для совместимости; столбцы считаются сами).
    pub fn num_columns(self, _n: usize) -> Self {
        self
    }

    /// Разложить таблицу.
    pub fn show<R>(self, ui: &mut Ui, f: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let id = ui.id.with(("таблица", self.salt));
        let prev = ui.ctx.st().grids.get(&id).cloned().unwrap_or_default();
        let mut child = ui.child(Dir::Vertical);
        let start = child.cursor;
        child.grid = Some(GridState {
            id,
            striped: self.striped,
            col: 0,
            row: 0,
            row_top: start.y,
            row_h: 0.0,
            x0: start.x,
            prev,
            widths: Vec::new(),
            heights: Vec::new(),
            stripe: None,
        });
        if self.striped {
            let at = child.reserve_shape();
            if let Some(g) = &mut child.grid {
                g.stripe = Some(at);
            }
        }
        let inner = f(&mut child);
        if child.grid.as_ref().is_some_and(|g| g.col > 0) {
            child.end_row();
        }
        if let Some(g) = child.grid.take() {
            ui.ctx.st().grids.insert(
                id,
                GridMem {
                    widths: g.widths,
                    heights: g.heights,
                },
            );
        }
        let rect = child.finish();
        ui.advance(rect);
        let response = ui.interact(rect, id, Sense::hover());
        InnerResponse { inner, response }
    }
}
