//! Меню: строка меню, выпадающие и вложенные меню, выпадающий список [`ComboBox`].
//!
//! Открытые меню хранятся путём `open_menus` (корневое → вложенные). Верхний пункт
//! строки меню открывается и закрывается нажатием; при открытом меню наведение на другой
//! верхний пункт переключает меню. Вложенное меню открывается наведением. Все меню
//! закрываются нажатием мыши вне них, клавишей Esc и [`Ui::close`].

use std::hash::Hash;

use crate::gfx::{Color, Pos2, Rect, Vec2, pos2, vec2};

use super::icon::Icon;
use super::layout::{MenuCtx, Ui};
use super::registry::WidgetKind;
use super::{Ctx, Id, InnerResponse, LayerId, Order, RichText, Sense, Shape};

/// Строка меню (как `egui::MenuBar`).
#[derive(Debug, Default)]
pub struct MenuBar;

impl MenuBar {
    /// Строка меню.
    pub fn new() -> Self {
        MenuBar
    }

    /// Разложить строку: внутри вызывайте [`Ui::menu_button`].
    pub fn ui<R>(self, ui: &mut Ui, f: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        ui.horizontal(|ui| {
            ui.menu = MenuCtx {
                bar: true,
                depth: 0,
                combo: false,
            };
            let sp = ui.item_spacing();
            ui.set_item_spacing(vec2(2.0, sp.y));
            f(ui)
        })
    }
}

/// Выпадающее окошко меню или списка: рамка, тень, пункты столбцом.
#[allow(clippy::too_many_arguments)]
pub(crate) fn popup<R>(
    ctx: &Ctx,
    id: Id,
    anchor: Pos2,
    flip_x: Option<f32>,
    depth: usize,
    combo: bool,
    min_w: f32,
    f: impl FnOnce(&mut Ui) -> R,
) -> R {
    let layer = LayerId {
        order: Order::Foreground,
        id,
    };
    let (th, screen, prev) = {
        let mut st = ctx.st();
        st.menu_seen(id);
        (
            st.theme.clone(),
            st.screen,
            st.popups.get(&id).copied().unwrap_or(Vec2::ZERO),
        )
    };
    let m = th.menu_margin;
    // Не выходить за экран (по размеру прошлого кадра).
    let mut pos = anchor;
    if pos.x + prev.x > screen.max.x {
        pos.x = match flip_x {
            Some(left) => left - prev.x,
            None => screen.max.x - prev.x,
        };
    }
    if pos.y + prev.y > screen.max.y {
        pos.y = screen.max.y - prev.y;
    }
    pos = pos.max(screen.min);
    let (shadow, frame, area) = {
        let mut st = ctx.st();
        let shadow = st.add_shape(layer, screen, Shape::Noop);
        let frame = st.add_shape(layer, screen, Shape::Noop);
        let area = st.add_area(layer, Rect::from_min_size(pos, prev));
        (shadow, frame, area)
    };
    let max_rect = Rect::from_min_max(
        pos + vec2(m, m),
        pos2(pos.x + m + th.wrap_width.max(min_w), screen.max.y),
    );
    let mut ui = Ui::new_root(ctx, id, layer, max_rect, screen);
    let min_inner = (prev.x - 2.0 * m).max(min_w - 2.0 * m).max(0.0);
    ui.set_bounds(max_rect, screen, min_inner);
    ui.menu = MenuCtx {
        bar: false,
        depth,
        combo,
    };
    let r = f(&mut ui);
    let content = ui.finish();
    let rect = Rect::from_min_max(pos, content.max + vec2(m, m));
    let mut st = ctx.st();
    st.set_shape(
        shadow,
        Shape::Shadow {
            rect,
            radius: th.menu_radius,
            offset: vec2(6.0, 10.0),
            blur: 8.0,
            color: th.shadow,
        },
    );
    st.set_shape(
        frame,
        Shape::Rect {
            rect,
            radius: th.menu_radius,
            fill: th.window_fill,
            stroke: (1.0, th.window_stroke),
        },
    );
    st.set_area(area, rect);
    st.add_popup_area(rect);
    st.popups.insert(id, rect.size());
    r
}

impl Ui {
    /// Пункт меню: в строке меню — верхний пункт, в выпадающем меню — пункт с вложенным
    /// меню (стрелка справа). `f` рисует содержимое, когда меню открыто.
    pub fn menu_button<R>(
        &mut self,
        text: impl Into<RichText>,
        f: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<Option<R>> {
        let text = text.into();
        let th = self.theme.clone();
        let id = self.id().with(("меню", text.text()));
        let depth = self.menu.depth;
        let top = depth == 0;
        let pad = th.button_padding;
        let ts = self.text_size(&text);
        let arrow_w = if top {
            0.0
        } else {
            th.icon_spacing * 2.0 + ts.y * 0.6
        };
        let mut size = vec2(ts.x + arrow_w, ts.y) + pad * 2.0;
        size.y = size.y.max(th.interact_height);
        if !top {
            size.x = size.x.max(self.min_rect().max.x - self.next_pos().x);
        }
        let rect = self.allocate_space(size);
        let r = self.interact(rect, id, Sense::click());
        let open = {
            let mut st = self.ctx.st();
            if top {
                if r.pressed {
                    st.menu_button_pressed = true;
                    if st.open_menus.first() == Some(&id) {
                        st.open_menus.clear();
                    } else {
                        st.open_menus = vec![id];
                    }
                } else if r.hovered
                    && !st.open_menus.is_empty()
                    && st.open_menus.first() != Some(&id)
                    && self.menu.bar
                {
                    // Переход между меню строки наведением.
                    st.open_menus = vec![id];
                }
                st.open_menus.first() == Some(&id)
            } else {
                if r.hovered || r.clicked {
                    st.open_menus.truncate(depth);
                    st.open_menus.push(id);
                }
                st.open_menus.get(depth) == Some(&id)
            }
        };
        let v = if open && r.enabled {
            if top { th.open } else { th.hovered }
        } else {
            self.style_of(&r)
        };
        if open || r.hovered {
            self.paint(Shape::Rect {
                rect: rect.expand(v.expansion),
                radius: v.radius,
                fill: v.weak_bg_fill,
                stroke: v.bg_stroke,
            });
        }
        let color = if text.has_color() {
            self.fade(text.color_or(&th, v.fg))
        } else {
            v.fg
        };
        let (fs, font) = text.font(&th);
        let tp = if top {
            rect.center() - ts * 0.5
        } else {
            pos2(rect.min.x + pad.x, rect.center().y - ts.y * 0.5)
        };
        self.paint_text(tp, text.text(), fs, font, color);
        if !top {
            let s = ts.y * 0.6;
            self.paint(Shape::Icon {
                rect: Rect::from_center_size(
                    pos2(rect.max.x - pad.x - s * 0.5, rect.center().y),
                    vec2(s, ts.y),
                ),
                icon: Icon::Right,
                size: s * 1.1,
                color,
            });
        }
        self.register(
            id,
            WidgetKind::MenuButton,
            text.text(),
            None,
            Some(open),
            rect,
            r.enabled,
        );
        let inner = if open {
            let (anchor, flip) = if top {
                (rect.left_bottom() + vec2(0.0, 2.0), None)
            } else {
                (
                    pos2(
                        rect.max.x + th.menu_margin + 2.0,
                        rect.min.y - th.menu_margin,
                    ),
                    Some(rect.min.x - th.menu_margin - 2.0),
                )
            };
            Some(popup(&self.ctx, id, anchor, flip, depth + 1, false, 0.0, f))
        } else {
            None
        };
        InnerResponse { inner, response: r }
    }
}

/// Выпадающий список (как `egui::ComboBox`): кнопка с выбранным значением, по нажатию —
/// список пунктов ([`Ui::selectable_value`]); выбор закрывает список.
pub struct ComboBox {
    salt: Id,
    width: Option<f32>,
    selected: RichText,
}

impl ComboBox {
    /// Список с солью id.
    pub fn from_id_salt(salt: impl Hash) -> Self {
        ComboBox {
            salt: Id::new(salt),
            width: None,
            selected: RichText::default(),
        }
    }

    /// Ширина кнопки.
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    /// Текст выбранного значения на кнопке.
    pub fn selected_text(mut self, text: impl Into<RichText>) -> Self {
        self.selected = text.into();
        self
    }

    /// Разложить кнопку и (если открыт) список.
    pub fn show_ui<R>(self, ui: &mut Ui, f: impl FnOnce(&mut Ui) -> R) -> InnerResponse<Option<R>> {
        let th = ui.theme.clone();
        let id = ui.id().with(("список", self.salt));
        let ts = ui.text_size(&self.selected);
        let arrow = th.icon_width;
        let w = self
            .width
            .unwrap_or(th.combo_width)
            .max(ts.x + arrow + th.button_padding.x * 3.0);
        let rect = ui.allocate_space(vec2(w, th.interact_height.max(ts.y)));
        let r = ui.interact(rect, id, Sense::click());
        let depth = ui.menu.depth;
        let open = {
            let mut st = ui.ctx.st();
            if r.pressed {
                st.menu_button_pressed = true;
                if st.open_menus.get(depth) == Some(&id) {
                    st.open_menus.truncate(depth);
                } else {
                    st.open_menus.truncate(depth);
                    st.open_menus.push(id);
                }
            }
            st.open_menus.get(depth) == Some(&id)
        };
        let v = if open { th.open } else { ui.style_of(&r) };
        ui.paint(Shape::Rect {
            rect: rect.expand(v.expansion),
            radius: v.radius,
            fill: if open {
                v.weak_bg_fill
            } else {
                ui.fade(v.weak_bg_fill)
            },
            stroke: v.bg_stroke,
        });
        let (fs, font) = self.selected.font(&th);
        ui.paint_text(
            pos2(
                rect.min.x + th.button_padding.x,
                rect.center().y - ts.y * 0.5,
            ),
            self.selected.text(),
            fs,
            font,
            v.fg,
        );
        let ar = Rect::from_center_size(
            pos2(
                rect.max.x - th.button_padding.x - arrow * 0.5,
                rect.center().y,
            ),
            Vec2::splat(arrow),
        );
        paint_arrow(ui, ar, v.fg);
        ui.register(
            id,
            WidgetKind::ComboBox,
            self.selected.text(),
            Some(self.selected.text().to_owned()),
            Some(open),
            rect,
            r.enabled,
        );
        let inner = open.then(|| {
            popup(
                &ui.ctx,
                id,
                rect.left_bottom() + vec2(0.0, 2.0),
                None,
                depth + 1,
                true,
                rect.width(),
                f,
            )
        });
        InnerResponse { inner, response: r }
    }
}

/// Стрелка вниз выпадающего списка (через фигуру-многоугольник).
fn paint_arrow(ui: &Ui, r: Rect, color: Color) {
    let c = r.center();
    let w = r.width().min(r.height()) * 0.5;
    ui.paint(Shape::Polygon {
        points: vec![
            pos2(c.x - w * 0.5, c.y - w * 0.25),
            pos2(c.x + w * 0.5, c.y - w * 0.25),
            pos2(c.x, c.y + w * 0.3),
        ],
        fill: color,
    });
}
