//! Однострочное поле ввода.

use crate::gfx::{Color, Rect, pos2, vec2};

use super::layout::Ui;
use super::registry::WidgetKind;
use super::text::{self, byte_of};
use super::widgets::Widget;
use super::{CursorIcon, Event, Id, KeyCode, Response, Sense, Shape, TextMem, TextStyle};

/// Однострочное поле ввода.
///
/// Мышь: щелчок ставит курсор и даёт фокус, перетаскивание выделяет. Клавиатура в фокусе:
/// ввод текста, Backspace, Delete, ←/→ (с Ctrl — по словам, с Shift — выделение), Home,
/// End, Ctrl+A, Ctrl+C/X/V (буфер обмена [`super::Clipboard`]). Enter и Esc снимают фокус
/// ([`Response::lost_focus`] в том же кадре), щелчок вне поля — в следующем.
pub struct TextEdit<'t> {
    text: &'t mut String,
    width: Option<f32>,
    hint: String,
    style: TextStyle,
    id: Option<Id>,
}

impl<'t> TextEdit<'t> {
    /// Поле для строки `text`.
    pub fn singleline(text: &'t mut String) -> Self {
        TextEdit {
            text,
            width: None,
            hint: String::new(),
            style: TextStyle::Body,
            id: None,
        }
    }

    /// Ширина поля.
    pub fn desired_width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    /// Подсказка в пустом поле.
    pub fn hint_text(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    /// Начертание (например, `TextStyle::Monospace`).
    pub fn font(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// Явный id (иначе — по порядку полей в области).
    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }
}

/// Правка строки по номерам символов.
struct Editor<'a> {
    text: &'a mut String,
    mem: TextMem,
    changed: bool,
}

impl Editor<'_> {
    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn sel(&self) -> (usize, usize) {
        (
            self.mem.cursor.min(self.mem.anchor),
            self.mem.cursor.max(self.mem.anchor),
        )
    }

    fn selected(&self) -> String {
        let (a, b) = self.sel();
        self.text[byte_of(self.text, a)..byte_of(self.text, b)].to_owned()
    }

    fn delete_range(&mut self, a: usize, b: usize) {
        if a < b {
            let (ba, bb) = (byte_of(self.text, a), byte_of(self.text, b));
            self.text.replace_range(ba..bb, "");
            self.changed = true;
        }
        self.mem.cursor = a;
        self.mem.anchor = a;
    }

    fn delete_sel(&mut self) -> bool {
        let (a, b) = self.sel();
        if a < b {
            self.delete_range(a, b);
            true
        } else {
            false
        }
    }

    fn insert(&mut self, s: &str) {
        let s: String = s.chars().filter(|c| !c.is_control()).collect();
        self.delete_sel();
        if s.is_empty() {
            return;
        }
        let at = byte_of(self.text, self.mem.cursor);
        self.text.insert_str(at, &s);
        self.mem.cursor += s.chars().count();
        self.mem.anchor = self.mem.cursor;
        self.changed = true;
    }

    /// Начало слова слева от `ci`.
    fn word_left(&self, ci: usize) -> usize {
        let cs: Vec<char> = self.text.chars().collect();
        let mut i = ci;
        while i > 0 && !cs[i - 1].is_alphanumeric() {
            i -= 1;
        }
        while i > 0 && cs[i - 1].is_alphanumeric() {
            i -= 1;
        }
        i
    }

    /// Конец слова справа от `ci`.
    fn word_right(&self, ci: usize) -> usize {
        let cs: Vec<char> = self.text.chars().collect();
        let mut i = ci;
        while i < cs.len() && !cs[i].is_alphanumeric() {
            i += 1;
        }
        while i < cs.len() && cs[i].is_alphanumeric() {
            i += 1;
        }
        i
    }

    fn move_to(&mut self, ci: usize, select: bool) {
        self.mem.cursor = ci.min(self.len());
        if !select {
            self.mem.anchor = self.mem.cursor;
        }
    }
}

impl Widget for TextEdit<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let th = ui.theme.clone();
        let (size, font) = self.style.font(&th);
        let ppp = ui.ppp;
        let m = th.text_edit_margin;
        let line_h = text::line_height(size, font, ppp);
        let w = self.width.unwrap_or(th.text_edit_width);
        let h = (line_h + 2.0 * m.y).max(th.interact_height);
        let rect = ui.allocate_space(vec2(w, h));
        let id = self.id.unwrap_or_else(|| ui.next_edit_id());
        let mut r = ui.interact(rect, id, Sense::click_and_drag().focusable());
        let inner = Rect::from_min_max(
            pos2(rect.min.x + m.x, rect.center().y - line_h * 0.5),
            pos2(rect.max.x - m.x, rect.center().y + line_h * 0.5),
        );
        let ctx = ui.ctx.clone();
        let mem = ctx.st().texts.get(&id).copied().unwrap_or_default();
        let mut ed = Editor {
            text: self.text,
            mem,
            changed: false,
        };
        let n = ed.len();
        ed.mem.cursor = ed.mem.cursor.min(n);
        ed.mem.anchor = ed.mem.anchor.min(n);

        if r.hovered {
            ctx.st().set_cursor_icon(CursorIcon::Text);
        }
        // Мышь: курсор и выделение.
        if let Some(p) = r.interact_pointer_pos {
            let ci = text::char_at(ed.text, p.x - inner.min.x + ed.mem.scroll, size, font, ppp);
            if r.pressed {
                let shift = ui.input(|i| i.modifiers.shift);
                ed.move_to(ci, shift);
            } else if r.dragged {
                ed.move_to(ci, true);
            }
        }
        // Клавиатура.
        let mut submit = false;
        if r.has_focus && r.enabled {
            ctx.st().text_focus = true;
            let events = ui.input(|i| i.events.clone());
            for ev in events {
                match ev {
                    Event::Text(s) => ed.insert(&s),
                    Event::Key(k) if k.pressed => {
                        let (ctrl, shift) = (k.modifiers.ctrl, k.modifiers.shift);
                        match k.key {
                            KeyCode::Backspace => {
                                if !ed.delete_sel() && ed.mem.cursor > 0 {
                                    let to = if ctrl {
                                        ed.word_left(ed.mem.cursor)
                                    } else {
                                        ed.mem.cursor - 1
                                    };
                                    ed.delete_range(to, ed.mem.cursor);
                                }
                            }
                            KeyCode::Delete => {
                                if !ed.delete_sel() && ed.mem.cursor < ed.len() {
                                    let to = if ctrl {
                                        ed.word_right(ed.mem.cursor)
                                    } else {
                                        ed.mem.cursor + 1
                                    };
                                    let c = ed.mem.cursor;
                                    ed.delete_range(c, to);
                                }
                            }
                            KeyCode::ArrowLeft => {
                                let (a, _) = ed.sel();
                                let to = if ctrl {
                                    ed.word_left(ed.mem.cursor)
                                } else if !shift && ed.mem.cursor != ed.mem.anchor {
                                    a
                                } else {
                                    ed.mem.cursor.saturating_sub(1)
                                };
                                ed.move_to(to, shift);
                            }
                            KeyCode::ArrowRight => {
                                let (_, b) = ed.sel();
                                let to = if ctrl {
                                    ed.word_right(ed.mem.cursor)
                                } else if !shift && ed.mem.cursor != ed.mem.anchor {
                                    b
                                } else {
                                    ed.mem.cursor + 1
                                };
                                ed.move_to(to, shift);
                            }
                            KeyCode::Home | KeyCode::ArrowUp => ed.move_to(0, shift),
                            KeyCode::End | KeyCode::ArrowDown => {
                                let n = ed.len();
                                ed.move_to(n, shift);
                            }
                            KeyCode::KeyA if ctrl => {
                                ed.mem.anchor = 0;
                                ed.mem.cursor = ed.len();
                            }
                            KeyCode::KeyC | KeyCode::Insert if ctrl => {
                                let s = ed.selected();
                                if !s.is_empty() {
                                    ctx.st().clipboard.set_text(&s);
                                }
                            }
                            KeyCode::KeyX if ctrl => {
                                let s = ed.selected();
                                if !s.is_empty() {
                                    ctx.st().clipboard.set_text(&s);
                                    ed.delete_sel();
                                }
                            }
                            KeyCode::KeyV if ctrl => {
                                let s = ctx.st().clipboard.get_text();
                                if let Some(s) = s {
                                    ed.insert(&s.replace(['\n', '\r', '\t'], " "));
                                }
                            }
                            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Escape => {
                                submit = true;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        if submit {
            ctx.st().surrender_focus(id);
            r.has_focus = false;
            r.lost_focus = true;
        }
        r.changed = ed.changed;

        // Прокрутка текста, чтобы курсор был виден.
        let tw = text::measure(ed.text, size, font, ppp).x;
        let cx = text::char_x(ed.text, ed.mem.cursor, size, font, ppp);
        let avail = inner.width().max(1.0);
        if tw <= avail {
            ed.mem.scroll = 0.0;
        } else {
            if cx - ed.mem.scroll < 0.0 {
                ed.mem.scroll = cx;
            }
            if cx - ed.mem.scroll > avail {
                ed.mem.scroll = cx - avail;
            }
            ed.mem.scroll = ed.mem.scroll.clamp(0.0, tw - avail + 2.0);
        }
        ctx.st().texts.insert(id, ed.mem);

        // Рисование.
        let v = ui.style_of(&r);
        let stroke = if r.has_focus {
            (1.0, th.selection_stroke)
        } else if r.hovered {
            th.hovered.bg_stroke
        } else {
            th.noninteractive.bg_stroke
        };
        ui.paint(Shape::Rect {
            rect,
            radius: th.inactive.radius,
            fill: ui.fade(th.extreme_bg),
            stroke: (stroke.0, ui.fade(stroke.1)),
        });
        let clip = Rect::from_min_max(
            pos2(rect.min.x + 2.0, rect.min.y),
            pos2(rect.max.x - 2.0, rect.max.y),
        )
        .intersect(ui.clip_rect());
        let mut st = ctx.st();
        let layer = ui.layer_id();
        let x = inner.min.x - ed.mem.scroll;
        if r.has_focus && ed.mem.cursor != ed.mem.anchor {
            let (a, b) = ed.sel();
            let xa = x + text::char_x(ed.text, a, size, font, ppp);
            let xb = x + text::char_x(ed.text, b, size, font, ppp);
            st.add_shape(
                layer,
                clip,
                Shape::Rect {
                    rect: Rect::from_min_max(pos2(xa, inner.min.y), pos2(xb, inner.max.y)),
                    radius: 0.0,
                    fill: th.selection_bg,
                    stroke: (0.0, Color::TRANSPARENT),
                },
            );
        }
        let (s, color) = if ed.text.is_empty() {
            (self.hint.clone(), th.weak_text)
        } else {
            (
                ed.text.clone(),
                if r.enabled { th.inactive.fg } else { v.fg },
            )
        };
        if !s.is_empty() {
            st.add_shape(
                layer,
                clip,
                Shape::Text {
                    pos: pos2(
                        if ed.text.is_empty() { inner.min.x } else { x },
                        inner.min.y,
                    ),
                    text: s,
                    size,
                    font,
                    color: if r.enabled {
                        color
                    } else {
                        color.gamma_multiply(th.disabled_alpha)
                    },
                },
            );
        }
        if r.has_focus {
            let cx = (x + cx).round();
            st.add_shape(
                layer,
                clip,
                Shape::Line {
                    a: pos2(cx, inner.min.y),
                    b: pos2(cx, inner.max.y),
                    width: 2.0,
                    color: th.cursor_color,
                },
            );
        }
        drop(st);
        ui.register(
            id,
            WidgetKind::TextEdit,
            &self.hint,
            Some(ed.text.clone()),
            None,
            rect,
            r.enabled,
        );
        r
    }
}
