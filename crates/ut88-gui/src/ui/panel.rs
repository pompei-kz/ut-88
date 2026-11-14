//! Панели (верхняя/нижняя, центральная), плавающие окна и всплывающая подсказка.

use std::hash::Hash;

use crate::gfx::{Color, Pos2, Rect, Vec2, pos2, vec2};

use super::layout::Ui;
use super::registry::WidgetKind;
use super::scroll::ScrollBody;
use super::text;
use super::{Ctx, Id, InnerResponse, LayerId, Order, Sense, Shape, WindowMem};

/// Рамка панели: заливка и внутренний отступ.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// Заливка (прозрачная — без заливки).
    pub fill: Color,
    /// Отступ содержимого.
    pub inner_margin: f32,
}

impl Frame {
    /// Без заливки и отступа.
    pub const NONE: Frame = Frame {
        fill: Color::TRANSPARENT,
        inner_margin: 0.0,
    };
}

// ------------------------------------------------------------------ верхняя и нижняя панели

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Top,
    Bottom,
}

/// Панель во всю ширину сверху или снизу. Высота — по
/// содержимому; нижние панели складываются снизу вверх в порядке вызова.
pub struct TopBottomPanel {
    id: Id,
    side: Side,
}

impl TopBottomPanel {
    /// Панель сверху.
    pub fn top(id: impl Hash) -> Self {
        TopBottomPanel {
            id: Id::new(("панель", id)),
            side: Side::Top,
        }
    }

    /// Панель снизу.
    pub fn bottom(id: impl Hash) -> Self {
        TopBottomPanel {
            id: Id::new(("панель", id)),
            side: Side::Bottom,
        }
    }

    /// Разложить содержимое.
    pub fn show<R>(self, ctx: &Ctx, f: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let layer = LayerId::background();
        let (th, avail, prev_h) = {
            let st = ctx.st();
            (
                st.theme.clone(),
                st.available,
                st.panels.get(&self.id).copied(),
            )
        };
        let m = th.panel_margin;
        let guess = prev_h.unwrap_or(th.interact_height + 2.0 * m.y);
        let (fill, line, area) = {
            let mut st = ctx.st();
            let fill = st.add_shape(layer, avail, Shape::Noop);
            let line = st.add_shape(layer, avail, Shape::Noop);
            let r = match self.side {
                Side::Top => Rect::from_min_size(avail.min, vec2(avail.width(), guess)),
                Side::Bottom => {
                    Rect::from_min_max(pos2(avail.min.x, avail.max.y - guess), avail.max)
                }
            };
            let area = st.add_area(layer, r);
            (fill, line, area)
        };
        let top = match self.side {
            Side::Top => avail.min.y,
            Side::Bottom => avail.max.y - guess,
        };
        let max_rect = Rect::from_min_max(
            pos2(avail.min.x + m.x, top + m.y),
            pos2(avail.max.x - m.x, avail.max.y - m.y),
        );
        let mut ui = Ui::new_root(ctx, self.id, layer, max_rect, avail);
        let inner = f(&mut ui);
        let content = ui.finish();
        let h = (content.height() + 2.0 * m.y).min(avail.height());
        let rect = match self.side {
            Side::Top => Rect::from_min_size(avail.min, vec2(avail.width(), h)),
            Side::Bottom => Rect::from_min_max(pos2(avail.min.x, avail.max.y - h), avail.max),
        };
        let (lw, lc) = th.noninteractive.bg_stroke;
        let ly = match self.side {
            Side::Top => rect.max.y - lw * 0.5,
            Side::Bottom => rect.min.y + lw * 0.5,
        };
        let mut st = ctx.st();
        st.set_shape(
            fill,
            Shape::Rect {
                rect,
                radius: 0.0,
                fill: th.panel_fill,
                stroke: (0.0, Color::TRANSPARENT),
            },
        );
        st.set_shape(
            line,
            Shape::Line {
                a: pos2(rect.min.x, ly),
                b: pos2(rect.max.x, ly),
                width: lw,
                color: lc,
            },
        );
        st.set_area(area, rect);
        st.panels.insert(self.id, h);
        match self.side {
            Side::Top => st.available.min.y = rect.max.y,
            Side::Bottom => st.available.max.y = rect.min.y,
        }
        let response = st.interact(ctx, layer, avail, rect, self.id, Sense::hover(), true);
        drop(st);
        InnerResponse { inner, response }
    }
}

// ------------------------------------------------------------------ центральная панель

/// Всё оставшееся место. По умолчанию — заливка и отступ;
/// с `Frame::NONE` — без них (для рисунка на холсте, см. [`Ui::canvas_at`]).
#[derive(Default)]
pub struct CentralPanel {
    frame: Option<Frame>,
}

impl CentralPanel {
    /// Рамка.
    pub fn frame(mut self, frame: Frame) -> Self {
        self.frame = Some(frame);
        self
    }

    /// Разложить содержимое.
    pub fn show<R>(self, ctx: &Ctx, f: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let layer = LayerId::background();
        let id = Id::new("центральная панель");
        let (th, rect) = {
            let st = ctx.st();
            (st.theme.clone(), st.available)
        };
        let frame = self.frame.unwrap_or(Frame {
            fill: th.panel_fill,
            inner_margin: th.central_margin,
        });
        {
            let mut st = ctx.st();
            if !frame.fill.is_transparent() {
                st.add_shape(
                    layer,
                    rect,
                    Shape::Rect {
                        rect,
                        radius: 0.0,
                        fill: frame.fill,
                        stroke: (0.0, Color::TRANSPARENT),
                    },
                );
            }
            st.add_area(layer, rect);
        }
        let mut ui = Ui::new_root(ctx, id, layer, rect.shrink(frame.inner_margin), rect);
        let inner = f(&mut ui);
        ui.finish();
        let response = ctx
            .st()
            .interact(ctx, layer, rect, rect, id, Sense::hover(), true);
        InnerResponse { inner, response }
    }
}

// ------------------------------------------------------------------ плавающее окно

/// Плавающее окно внутри главного: заголовок (перетаскивание),
/// кнопка свёртывания, крестик (если задан [`Window::open`]), прокрутка содержимого,
/// подъём наверх щелчком. Окно не выходит за пределы главного окна.
pub struct Window<'o> {
    title: String,
    id: Option<Id>,
    open: Option<&'o mut bool>,
    default_size: Option<Vec2>,
    default_width: Option<f32>,
    default_pos: Option<Pos2>,
    vscroll: bool,
    collapsible: bool,
}

impl<'o> Window<'o> {
    /// Окно с заголовком (он же определяет id).
    pub fn new(title: impl Into<String>) -> Self {
        Window {
            title: title.into(),
            id: None,
            open: None,
            default_size: None,
            default_width: None,
            default_pos: None,
            vscroll: false,
            collapsible: true,
        }
    }

    /// Флаг «окно открыто»: крестик в заголовке сбрасывает его.
    pub fn open(mut self, open: &'o mut bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Явный id.
    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    /// Начальный размер содержимого (с `vscroll` высота — предел до прокрутки).
    pub fn default_size(mut self, size: impl Into<Vec2>) -> Self {
        self.default_size = Some(size.into());
        self
    }

    /// Начальная ширина содержимого.
    pub fn default_width(mut self, w: f32) -> Self {
        self.default_width = Some(w);
        self
    }

    /// Начальное положение левого верхнего угла.
    pub fn default_pos(mut self, pos: impl Into<Pos2>) -> Self {
        self.default_pos = Some(pos.into());
        self
    }

    /// Вертикальная прокрутка содержимого.
    pub fn vscroll(mut self, on: bool) -> Self {
        self.vscroll = on;
        self
    }

    /// Можно ли свернуть окно.
    pub fn collapsible(mut self, on: bool) -> Self {
        self.collapsible = on;
        self
    }

    /// Показать окно. `None` — окно закрыто; `inner` равен `None`, если окно свёрнуто.
    pub fn show<R>(
        self,
        ctx: &Ctx,
        f: impl FnOnce(&mut Ui) -> R,
    ) -> Option<InnerResponse<Option<R>>> {
        if self.open.as_deref() == Some(&false) {
            return None;
        }
        let id = self.id.unwrap_or_else(|| Id::new(("окно", &self.title)));
        let layer = LayerId {
            order: Order::Middle,
            id,
        };
        let (th, screen, avail, frame, ppp) = {
            let st = ctx.st();
            (
                st.theme.clone(),
                st.screen,
                st.available,
                st.frame,
                st.input.ppp.max(0.1),
            )
        };
        let m = th.window_margin;
        let def_w = self.default_size.map(|s| s.x).or(self.default_width);
        let def_h = self.default_size.map(|s| s.y);
        let title_text_h = text::line_height(th.title_size, crate::gfx::FontKind::Sans, ppp);
        let title_h = (title_text_h + 8.0).max(th.interact_height + 4.0);
        let title_w = text::measure(&self.title, th.title_size, crate::gfx::FontKind::Sans, ppp).x;
        let btn = th.icon_width + 2.0;
        let min_inner = title_w + 2.0 * (btn + th.item_spacing.x);

        // Память окна.
        let mut mem = {
            let mut st = ctx.st();
            let n = st.windows.len();
            let mem = *st.windows.entry(id).or_insert_with(|| {
                let k = (n % 8) as f32 * 24.0;
                WindowMem {
                    pos: self
                        .default_pos
                        .unwrap_or(avail.min + vec2(16.0 + k, 16.0 + k)),
                    content: vec2(def_w.unwrap_or(0.0), def_h.unwrap_or(0.0)),
                    collapsed: false,
                    last_frame: 0,
                }
            });
            if mem.last_frame + 1 != frame || !st.window_order.contains(&id) {
                st.raise_window(id);
            }
            mem
        };
        mem.last_frame = frame;

        let inner_w = def_w.unwrap_or(0.0).max(mem.content.x).max(min_inner);
        let wrap_w = match def_w {
            Some(_) => inner_w,
            None => th.wrap_width.max(mem.content.x),
        };
        let screen_max_view = (screen.height() - title_h - 2.0 * m - 2.0).max(20.0);
        let max_view = match (self.vscroll, def_h) {
            (true, Some(h)) => h.min(screen_max_view),
            _ => screen_max_view,
        };
        let view_h = mem.content.y.min(max_view);
        let est = vec2(
            inner_w + 2.0 * m,
            title_h
                + if mem.collapsed {
                    0.0
                } else {
                    1.0 + 2.0 * m + view_h
                },
        );
        let clamp_pos = |p: Pos2, size: Vec2| {
            pos2(
                p.x.clamp(screen.min.x, (screen.max.x - size.x).max(screen.min.x)),
                p.y.clamp(screen.min.y, (screen.max.y - size.y).max(screen.min.y)),
            )
        };
        mem.pos = clamp_pos(mem.pos, est);

        // Слой: тень и рамка — первыми, размер станет известен после содержимого.
        let (shadow, frame_at, area) = {
            let mut st = ctx.st();
            let s = st.add_shape(layer, screen, Shape::Noop);
            let f = st.add_shape(layer, screen, Shape::Noop);
            let a = st.add_area(layer, Rect::from_min_size(mem.pos, est));
            (s, f, a)
        };

        // Заголовок: кнопки и перетаскивание.
        let tui = Ui::new_root(ctx, id.with("заголовок"), layer, screen, screen);
        let title_rect = |pos: Pos2| Rect::from_min_size(pos, vec2(est.x, title_h));
        let tr = title_rect(mem.pos);
        let close_rect = Rect::from_center_size(
            pos2(tr.max.x - m - btn * 0.5, tr.center().y),
            Vec2::splat(btn),
        );
        let coll_rect = Rect::from_center_size(
            pos2(tr.min.x + m + btn * 0.5, tr.center().y),
            Vec2::splat(btn),
        );
        let close = self
            .open
            .is_some()
            .then(|| tui.interact(close_rect, id.with("закрыть"), Sense::click()));
        let coll = self
            .collapsible
            .then(|| tui.interact(coll_rect, id.with("свернуть"), Sense::click()));
        let drag = tui.interact(tr, id.with("тащить"), Sense::drag());
        if drag.dragged {
            mem.pos = clamp_pos(mem.pos + drag.drag_delta, est);
            ctx.st().set_cursor_icon(super::CursorIcon::Grabbing);
        }
        if coll.as_ref().is_some_and(|r| r.clicked) {
            mem.collapsed = !mem.collapsed;
        }
        let closed = close.as_ref().is_some_and(|r| r.clicked);
        let tr = title_rect(mem.pos);

        // Содержимое.
        let mut inner = None;
        let mut content = mem.content;
        let mut body_h = 0.0;
        let mut body_w = inner_w;
        if !mem.collapsed {
            let origin = pos2(mem.pos.x + m, tr.max.y + 1.0 + m);
            let body = ScrollBody {
                ctx: ctx.clone(),
                layer,
                clip: screen,
                id: id.with("содержимое"),
                origin,
                width: wrap_w,
                min_width: if def_w.is_some() { inner_w } else { min_inner },
                max_h: max_view,
                enabled: true,
                menu: Default::default(),
                spacing: th.item_spacing,
            };
            let (r, out) = body.show(f);
            inner = Some(r);
            content = out.content;
            body_h = out.rect.height();
            body_w = match def_w {
                Some(_) => inner_w.max(out.rect.width()),
                None => out.rect.width().max(min_inner),
            };
        }
        let w = body_w + 2.0 * m;
        let h = title_h
            + if mem.collapsed {
                0.0
            } else {
                1.0 + 2.0 * m + body_h
            };
        let rect = Rect::from_min_size(mem.pos, vec2(w, h));

        // Рисование рамки и заголовка.
        let fg = th.inactive.fg;
        {
            let mut st = ctx.st();
            st.set_shape(
                shadow,
                Shape::Shadow {
                    rect,
                    radius: th.window_radius,
                    offset: vec2(10.0, 20.0),
                    blur: 15.0,
                    color: th.shadow,
                },
            );
            st.set_shape(
                frame_at,
                Shape::Rect {
                    rect,
                    radius: th.window_radius,
                    fill: th.window_fill,
                    stroke: (1.0, th.window_stroke),
                },
            );
            st.set_area(area, rect);
        }
        let tr = Rect::from_min_size(rect.min, vec2(w, title_h));
        if !mem.collapsed {
            let (lw, lc) = th.noninteractive.bg_stroke;
            tui.paint(Shape::Line {
                a: pos2(rect.min.x, tr.max.y + 0.5),
                b: pos2(rect.max.x, tr.max.y + 0.5),
                width: lw,
                color: lc,
            });
        }
        let tp = pos2(
            tr.center().x - title_w * 0.5,
            tr.center().y - title_text_h * 0.5,
        );
        tui.paint_text(
            tp,
            &self.title,
            th.title_size,
            crate::gfx::FontKind::Sans,
            fg,
        );
        tui.register(
            id.with("заголовок"),
            WidgetKind::WindowTitle,
            &self.title,
            None,
            None,
            tr,
            true,
        );
        if let Some(r) = &coll {
            let c = pos2(tr.min.x + m + btn * 0.5, tr.center().y);
            let color = if r.hovered { th.hovered.fg } else { fg };
            let s = th.icon_width_inner * 0.6;
            let pts = if mem.collapsed {
                vec![
                    pos2(c.x - s * 0.5, c.y - s),
                    pos2(c.x - s * 0.5, c.y + s),
                    pos2(c.x + s * 0.8, c.y),
                ]
            } else {
                vec![
                    pos2(c.x - s, c.y - s * 0.5),
                    pos2(c.x + s, c.y - s * 0.5),
                    pos2(c.x, c.y + s * 0.8),
                ]
            };
            tui.paint(Shape::Polygon {
                points: pts,
                fill: color,
            });
            tui.register(
                r.id,
                WidgetKind::CollapseButton,
                &format!("Свернуть «{}»", self.title),
                None,
                Some(mem.collapsed),
                Rect::from_center_size(c, Vec2::splat(btn)),
                true,
            );
        }
        if let Some(r) = &close {
            let c = pos2(tr.max.x - m - btn * 0.5, tr.center().y);
            let (color, width) = if r.hovered {
                (th.hovered.fg, 1.5)
            } else {
                (fg, 1.0)
            };
            let s = th.icon_width_inner * 0.5;
            tui.paint(Shape::Line {
                a: pos2(c.x - s, c.y - s),
                b: pos2(c.x + s, c.y + s),
                width,
                color,
            });
            tui.paint(Shape::Line {
                a: pos2(c.x - s, c.y + s),
                b: pos2(c.x + s, c.y - s),
                width,
                color,
            });
            tui.register(
                r.id,
                WidgetKind::CloseButton,
                &format!("Закрыть «{}»", self.title),
                None,
                None,
                Rect::from_center_size(c, Vec2::splat(btn)),
                true,
            );
        }

        mem.content = content;
        ctx.st().windows.insert(id, mem);
        if closed && let Some(open) = self.open {
            *open = false;
        }
        let response = tui.interact(rect, id, Sense::hover());
        Some(InnerResponse { inner, response })
    }
}

// ------------------------------------------------------------------ подсказка

/// Нарисовать подсказку `text` под прямоугольником `rect`.
pub(crate) fn show_tooltip(ctx: &Ctx, id: Id, text_s: &str, rect: Rect) {
    let layer = LayerId {
        order: Order::Tooltip,
        id: id.with("подсказка"),
    };
    let (th, screen, ppp) = {
        let st = ctx.st();
        (st.theme.clone(), st.screen, st.input.ppp.max(0.1))
    };
    let (size, font) = (th.body_size, crate::gfx::FontKind::Sans);
    let lines = text::wrap(text_s, size, font, ppp, th.wrap_width);
    let lh = text::line_height(size, font, ppp);
    let w = lines
        .iter()
        .map(|l| text::measure(l, size, font, ppp).x)
        .fold(0.0, f32::max);
    let m = th.menu_margin;
    let sz = vec2(w + 2.0 * m, lh * lines.len() as f32 + 2.0 * m);
    let mut pos = rect.left_bottom() + vec2(0.0, 4.0);
    if pos.x + sz.x > screen.max.x {
        pos.x = screen.max.x - sz.x;
    }
    if pos.y + sz.y > screen.max.y {
        pos.y = rect.min.y - 4.0 - sz.y;
    }
    let pos = pos.max(screen.min);
    let r = Rect::from_min_size(pos, sz);
    let mut st = ctx.st();
    st.add_shape(
        layer,
        screen,
        Shape::Shadow {
            rect: r,
            radius: th.menu_radius,
            offset: vec2(6.0, 10.0),
            blur: 8.0,
            color: th.shadow,
        },
    );
    st.add_shape(
        layer,
        screen,
        Shape::Rect {
            rect: r,
            radius: th.menu_radius,
            fill: th.window_fill,
            stroke: (1.0, th.window_stroke),
        },
    );
    for (i, l) in lines.into_iter().enumerate() {
        st.add_shape(
            layer,
            screen,
            Shape::Text {
                pos: pos + vec2(m, m + lh * i as f32),
                text: l,
                size,
                font,
                color: th.text_color,
            },
        );
    }
    st.add_area(layer, r);
}
