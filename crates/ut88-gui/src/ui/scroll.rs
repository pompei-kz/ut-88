//! Вертикальная прокрутка: колесо мыши и полоса справа.

use std::hash::Hash;

use crate::gfx::{Color, Pos2, Rect, Vec2, pos2, vec2};

use super::layout::{MenuCtx, Ui};
use super::{Ctx, Id, LayerId, ScrollMem, Sense, Shape};

/// Параметры области прокрутки.
pub(crate) struct ScrollBody {
    pub ctx: Ctx,
    pub layer: LayerId,
    pub clip: Rect,
    pub id: Id,
    pub origin: Pos2,
    pub width: f32,
    pub min_width: f32,
    pub max_h: f32,
    pub enabled: bool,
    pub menu: MenuCtx,
    pub spacing: Vec2,
}

/// Итог области прокрутки.
pub(crate) struct ScrollOut {
    /// Занятый прямоугольник (видимая часть).
    pub rect: Rect,
    /// Размер содержимого.
    pub content: Vec2,
}

impl ScrollBody {
    /// Разложить содержимое со сдвигом прокрутки; прокрутить колесом и полосой.
    pub fn show<R>(self, f: impl FnOnce(&mut Ui) -> R) -> (R, ScrollOut) {
        let (th, mem) = {
            let st = self.ctx.st();
            (
                st.theme.clone(),
                st.scrolls.get(&self.id).copied().unwrap_or_default(),
            )
        };
        let bar_w = th.scroll_bar_width;
        let overflow_prev = mem.content_h > self.max_h + 0.5;
        let content_w = if overflow_prev {
            (self.width - bar_w - 2.0).max(0.0)
        } else {
            self.width
        };
        let mut offset = mem.offset.clamp(0.0, (mem.content_h - self.max_h).max(0.0));
        let view_max = Rect::from_min_size(self.origin, vec2(self.width, self.max_h));
        let clip = view_max.intersect(self.clip);
        let top = pos2(self.origin.x, self.origin.y - offset);
        let mut ui = Ui::new_root(
            &self.ctx,
            self.id,
            self.layer,
            Rect::from_min_max(top, pos2(top.x + content_w, f32::INFINITY)),
            clip,
        );
        ui.set_bounds(
            Rect::from_min_max(top, pos2(top.x + content_w, f32::INFINITY)),
            clip,
            self.min_width.min(content_w),
        );
        ui.menu = self.menu;
        ui.enabled = self.enabled;
        ui.set_item_spacing(self.spacing);
        let r = f(&mut ui);
        let content = ui.finish();
        let content_h = content.height();
        let view_h = content_h.min(self.max_h);
        let overflow = content_h > self.max_h + 0.5;
        let width = if overflow {
            content.width() + bar_w + 2.0
        } else {
            content.width()
        }
        .max(self.min_width.min(self.width));
        let view = Rect::from_min_size(self.origin, vec2(width.min(self.width), view_h));
        let mut st = self.ctx.st();
        if overflow {
            let max_off = content_h - view_h;
            // Колесо.
            if st.scroll_left.y != 0.0
                && st
                    .input
                    .pointer
                    .is_some_and(|p| view.intersect(self.clip).contains(p))
                && st.layer_hovered(self.layer)
            {
                offset -= st.scroll_left.y;
                st.scroll_left.y = 0.0;
            }
            // Полоса.
            let track = Rect::from_min_max(
                pos2(self.origin.x + self.width - bar_w, self.origin.y),
                pos2(self.origin.x + self.width, self.origin.y + view_h),
            );
            let handle_h = (view_h * view_h / content_h).max(16.0).min(view_h);
            let free = (view_h - handle_h).max(1.0);
            let resp = st.interact(
                &self.ctx,
                self.layer,
                self.clip,
                track,
                self.id.with("полоса"),
                Sense::drag(),
                self.enabled,
            );
            let hy = |off: f32| self.origin.y + off / max_off * free;
            if resp.pressed
                && let Some(p) = resp.interact_pointer_pos
            {
                let y = hy(offset);
                if p.y < y || p.y > y + handle_h {
                    offset = (p.y - self.origin.y - handle_h * 0.5) / free * max_off;
                }
            } else if resp.dragged {
                offset += resp.drag_delta.y * max_off / free;
            }
            offset = offset.clamp(0.0, max_off);
            let handle = Rect::from_min_size(pos2(track.min.x, hy(offset)), vec2(bar_w, handle_h));
            let fill = if resp.dragged || resp.hovered {
                th.hovered.bg_fill
            } else {
                th.inactive.bg_fill
            };
            st.add_shape(
                self.layer,
                self.clip,
                Shape::Rect {
                    rect: handle,
                    radius: bar_w * 0.5,
                    fill,
                    stroke: (0.0, Color::TRANSPARENT),
                },
            );
        } else {
            offset = 0.0;
        }
        st.scrolls.insert(self.id, ScrollMem { offset, content_h });
        (
            r,
            ScrollOut {
                rect: view,
                content: content.size(),
            },
        )
    }
}

/// Область вертикальной прокрутки.
pub struct ScrollArea {
    salt: Option<Id>,
    max_height: f32,
}

impl ScrollArea {
    /// Вертикальная прокрутка.
    pub fn vertical() -> Self {
        ScrollArea {
            salt: None,
            max_height: f32::INFINITY,
        }
    }

    /// Наибольшая высота.
    pub fn max_height(mut self, h: f32) -> Self {
        self.max_height = h;
        self
    }

    /// Соль id.
    pub fn id_salt(mut self, salt: impl Hash) -> Self {
        self.salt = Some(Id::new(salt));
        self
    }

    /// Разложить содержимое.
    pub fn show<R>(self, ui: &mut Ui, f: impl FnOnce(&mut Ui) -> R) -> R {
        let id = match self.salt {
            Some(s) => ui.make_persistent_id(("прокрутка", s)),
            None => ui.next_auto_id().with("прокрутка"),
        };
        let avail = ui.available_rect_before_wrap();
        let body = ScrollBody {
            ctx: ui.ctx().clone(),
            layer: ui.layer_id(),
            clip: ui.clip_rect(),
            id,
            origin: avail.min,
            width: avail.width(),
            min_width: avail.width(),
            max_h: self.max_height.min(avail.height()).max(0.0),
            enabled: ui.is_enabled(),
            menu: ui.menu,
            spacing: ui.item_spacing(),
        };
        let (r, out) = body.show(f);
        ui.advance(out.rect);
        r
    }
}
