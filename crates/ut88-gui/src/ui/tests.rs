//! Тесты набора виджетов на стенде без окна.

use crate::gfx::{Color, pos2, vec2};

use super::testing::Harness;
use super::*;

/// Состояние для тестов: счётчики щелчков и значения виджетов.
#[derive(Default)]
struct S {
    clicks: usize,
    other: usize,
    flag: bool,
    radio: u8,
    vol: f32,
    text: String,
    lost: usize,
    changed: usize,
    open: bool,
    item: usize,
    bg: usize,
    sub: usize,
    combo: u8,
}

fn size() -> Vec2 {
    vec2(640.0, 480.0)
}

/// Кнопка доступна/недоступна в центральной панели.
fn buttons(ctx: &Ctx, s: &mut S) {
    CentralPanel::default().show(ctx, |ui| {
        if ui.button("Жми").clicked() {
            s.clicks += 1;
        }
        if ui.add_enabled(false, Button::new("Нельзя")).clicked() {
            s.other += 1;
        }
    });
}

#[test]
fn button_click_once() {
    let mut h = Harness::new(size(), S::default(), buttons);
    h.click("Жми");
    assert_eq!(h.state.clicks, 1);
    h.run(5);
    assert_eq!(h.state.clicks, 1);
    // Нажатие и отпускание в одном кадре — тоже щелчок.
    let p = h.get("Жми").rect.center();
    h.input_mut().pointer = Some(p);
    h.input_mut().pointer_button(PointerButton::Primary, true);
    h.input_mut().pointer_button(PointerButton::Primary, false);
    h.step();
    assert_eq!(h.state.clicks, 2);
    // Нажали на кнопке, отпустили вне — не щелчок.
    h.input_mut().pointer_button(PointerButton::Primary, true);
    h.step();
    h.input_mut().pointer = Some(pos2(600.0, 400.0));
    h.input_mut().pointer_button(PointerButton::Primary, false);
    h.step();
    assert_eq!(h.state.clicks, 2);
}

#[test]
fn disabled_button_not_clicked() {
    let mut h = Harness::new(size(), S::default(), buttons);
    let w = h.get("Нельзя");
    assert!(!w.enabled);
    assert_eq!(w.kind, WidgetKind::Button);
    h.click("Нельзя");
    assert_eq!(h.state.other, 0);
    // И «без мыши» тоже.
    h.ctx.queue_click(w.id);
    h.step();
    assert_eq!(h.state.other, 0);
}

fn values(ctx: &Ctx, s: &mut S) {
    CentralPanel::default().show(ctx, |ui| {
        if ui.checkbox(&mut s.flag, "Флажок").changed() {
            s.changed += 1;
        }
        ui.horizontal(|ui| {
            ui.radio_value(&mut s.radio, 0, "ноль");
            ui.radio_value(&mut s.radio, 1, "один");
            if ui.radio(s.radio == 2, "два").clicked() {
                s.radio = 2;
            }
        });
        if ui
            .add(Slider::new(&mut s.vol, 0.0..=1.0).text("громкость"))
            .changed()
        {
            s.changed += 1;
        }
    });
}

#[test]
fn checkbox_radio_slider_change() {
    let mut h = Harness::new(size(), S::default(), values);
    h.click("Флажок");
    assert!(h.state.flag);
    assert_eq!(h.get("Флажок").selected, Some(true));
    h.click("Флажок");
    assert!(!h.state.flag);
    assert_eq!(h.state.changed, 2);

    h.click("один");
    assert_eq!(h.state.radio, 1);
    assert_eq!(h.get("один").selected, Some(true));
    assert_eq!(h.get("ноль").selected, Some(false));
    h.click("два");
    assert_eq!(h.state.radio, 2);

    // Ползунок: щелчок в правый край дорожки — максимум, перетаскивание влево — меньше.
    let r = h.get("громкость").rect;
    h.click_at(pos2(r.min.x + 99.0, r.center().y));
    assert_eq!(h.state.vol, 1.0);
    assert_eq!(h.get("громкость").value.as_deref(), Some("1"));
    h.drag(
        pos2(r.min.x + 93.0, r.center().y),
        pos2(r.min.x + 50.0, r.center().y),
        4,
    );
    let v = h.state.vol;
    assert!(v > 0.4 && v < 0.6, "{v}");
    // Стрелки в фокусе.
    h.focus("громкость");
    h.key(KeyCode::ArrowLeft);
    h.key(KeyCode::ArrowLeft);
    assert!((h.state.vol - (v - 0.02)).abs() < 1e-4, "{}", h.state.vol);
    h.key(KeyCode::End);
    assert_eq!(h.state.vol, 1.0);
}

fn edit(ctx: &Ctx, s: &mut S) {
    CentralPanel::default().show(ctx, |ui| {
        let r = ui.add(
            TextEdit::singleline(&mut s.text)
                .desired_width(120.0)
                .hint_text("адрес"),
        );
        if r.changed() {
            s.changed += 1;
        }
        if r.lost_focus() {
            s.lost += 1;
            if ui.input(|i| i.key_pressed(KeyCode::Enter)) {
                s.item += 1;
            }
        }
        if ui.button("Кнопка").clicked() {
            s.clicks += 1;
        }
    });
}

#[test]
fn text_edit_typing() {
    let mut h = Harness::new(size(), S::default(), edit);
    assert!(h.get("адрес").value.as_deref() == Some(""));
    h.click("адрес");
    assert!(h.output.wants_keyboard_input);
    assert!(h.ctx.wants_keyboard_input());
    h.type_text("C00");
    h.type_text("0");
    assert_eq!(h.state.text, "C000");
    assert_eq!(h.get_by_value("C000").kind, WidgetKind::TextEdit);
    h.key(KeyCode::Backspace);
    assert_eq!(h.state.text, "C00");
    h.key(KeyCode::Home);
    h.type_text("1");
    assert_eq!(h.state.text, "1C00");
    h.key(KeyCode::ArrowRight);
    h.key(KeyCode::Delete);
    assert_eq!(h.state.text, "1C0");
    h.key(KeyCode::End);
    h.key_mods(Modifiers::SHIFT, KeyCode::ArrowLeft);
    h.key_mods(Modifiers::SHIFT, KeyCode::ArrowLeft);
    h.type_text("ЖЖ");
    assert_eq!(h.state.text, "1ЖЖ");
    // Буфер обмена: Ctrl+A, Ctrl+C, Ctrl+V, Ctrl+X.
    h.key_mods(Modifiers::CTRL, KeyCode::KeyA);
    h.key_mods(Modifiers::CTRL, KeyCode::KeyC);
    assert_eq!(h.clipboard().text().as_deref(), Some("1ЖЖ"));
    h.key(KeyCode::End);
    h.key_mods(Modifiers::CTRL, KeyCode::KeyV);
    assert_eq!(h.state.text, "1ЖЖ1ЖЖ");
    h.clipboard().set("AB\nC");
    h.key(KeyCode::Home);
    h.key_mods(Modifiers::CTRL, KeyCode::KeyV);
    assert_eq!(h.state.text, "AB C1ЖЖ1ЖЖ");
    h.key_mods(Modifiers::CTRL, KeyCode::KeyA);
    h.key_mods(Modifiers::CTRL, KeyCode::KeyX);
    assert_eq!(h.state.text, "");
    assert_eq!(h.clipboard().text().as_deref(), Some("AB C1ЖЖ1ЖЖ"));
    assert_eq!(h.state.lost, 0);
    // Enter снимает фокус; lost_focus и Enter — в одном кадре.
    h.type_text("F000");
    h.key(KeyCode::Enter);
    assert_eq!(h.state.lost, 1);
    assert_eq!(h.state.item, 1);
    assert!(!h.ctx.wants_keyboard_input());
    // Без фокуса набор не попадает в поле.
    h.type_text("zz");
    assert_eq!(h.state.text, "F000");
    // Esc и щелчок вне поля тоже снимают фокус.
    h.click("адрес");
    h.key(KeyCode::Escape);
    assert_eq!(h.state.lost, 2);
    h.click("адрес");
    h.click("Кнопка");
    assert_eq!(h.state.lost, 3);
    assert_eq!(h.state.item, 1);
    assert_eq!(h.state.clicks, 1);
    assert!(h.state.changed > 5);
}

#[test]
fn text_edit_mouse_cursor() {
    let mut h = Harness::new(size(), S::default(), edit);
    h.state.text = "ABCDEF".into();
    h.step();
    let r = h.get("адрес").rect;
    // Щелчок у левого края — курсор в начале.
    h.click_at(pos2(r.min.x + 4.0, r.center().y));
    h.type_text("_");
    assert_eq!(h.state.text, "_ABCDEF");
    // Перетаскивание выделяет весь текст.
    h.drag(
        pos2(r.min.x + 3.0, r.center().y),
        pos2(r.max.x - 2.0, r.center().y),
        3,
    );
    h.type_text("x");
    assert_eq!(h.state.text, "x");
}

/// Меню над кнопкой центральной панели.
fn menus(ctx: &Ctx, s: &mut S) {
    TopBottomPanel::top("menu").show(ctx, |ui| {
        MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Машина", |ui| {
                if ui.button("Пункт").clicked() {
                    s.item += 1;
                    ui.close();
                }
                ui.checkbox(&mut s.flag, "Галочка");
                ui.separator();
                ui.menu_button("Ещё", |ui| {
                    if ui.button("Вложенный").clicked() {
                        s.sub += 1;
                        ui.close();
                    }
                });
                ui.add_enabled(false, Button::new("Выключен"));
            });
            ui.menu_button("Вид", |ui| {
                ui.label("пусто");
            });
        });
    });
    CentralPanel::default().show(ctx, |ui| {
        ui.add_space(4.0);
        if ui.button("Под меню, широкая кнопка фона").clicked() {
            s.bg += 1;
        }
    });
}

#[test]
fn menu_open_click_close() {
    let mut h = Harness::new(size(), S::default(), menus);
    assert!(!h.has("Пункт"));
    h.click("Машина");
    assert!(h.has("Пункт"));
    assert!(h.ctx.any_popup_open());
    assert_eq!(h.get("Машина").selected, Some(true));
    // Флажок не закрывает меню.
    h.click("Галочка");
    assert!(h.state.flag);
    assert!(h.has("Пункт"));
    // Недоступный пункт не закрывает и не щёлкается.
    assert!(!h.get("Выключен").enabled);
    h.click("Выключен");
    assert!(h.has("Пункт"));
    h.click("Пункт");
    assert_eq!(h.state.item, 1);
    assert!(!h.has("Пункт"));
    assert!(!h.ctx.any_popup_open());
    // Повторное нажатие на верхний пункт закрывает меню.
    h.click("Машина");
    assert!(h.has("Пункт"));
    h.click("Машина");
    assert!(!h.has("Пункт"));
    // Esc закрывает.
    h.click("Машина");
    h.key(KeyCode::Escape);
    assert!(!h.has("Пункт"));
}

#[test]
fn menu_hover_switch_and_submenu() {
    let mut h = Harness::new(size(), S::default(), menus);
    h.click("Машина");
    // Наведение на другой верхний пункт переключает меню.
    let vid = h.get("Вид").rect.center();
    h.move_to(vid);
    h.step();
    assert!(h.has("пусто"));
    assert!(!h.has("Пункт"));
    let m = h.get("Машина").rect.center();
    h.move_to(m);
    h.step();
    assert!(h.has("Пункт"));
    // Вложенное меню открывается наведением, пункт в нём щёлкается.
    let more = h.get("Ещё");
    assert_eq!(more.kind, WidgetKind::MenuButton);
    h.move_to(more.rect.center());
    h.step();
    assert!(h.has("Вложенный"));
    // Наведение на другой пункт закрывает вложенное меню.
    let p = h.get("Пункт").rect.center();
    h.move_to(p);
    h.step();
    assert!(!h.has("Вложенный"));
    h.move_to(more.rect.center());
    h.step();
    h.click("Вложенный");
    assert_eq!(h.state.sub, 1);
    assert!(!h.ctx.any_popup_open());
}

#[test]
fn click_through_menu_blocked() {
    let mut h = Harness::new(size(), S::default(), menus);
    let bg = h.get("Под меню, широкая кнопка фона");
    h.click("Машина");
    let item = h.get("Пункт");
    assert!(
        item.rect.intersects(bg.rect),
        "меню должно перекрывать кнопку"
    );
    // Щелчок по месту, где меню перекрывает кнопку фона: кнопка не нажимается.
    let p = item.rect.intersect(bg.rect).center();
    assert!(
        h.ctx
            .layer_id_at(p)
            .is_some_and(|l| l.order == Order::Foreground)
    );
    h.click_at(p);
    assert_eq!(h.state.bg, 0);
    assert_eq!(h.state.item, 1);
    // Щелчок вне меню закрывает его и сам до кнопки не доходит.
    h.click("Машина");
    let free = pos2(bg.rect.max.x - 5.0, bg.rect.center().y);
    assert!(!h.get("Пункт").rect.contains(free));
    h.click_at(free);
    assert!(!h.has("Пункт"));
    assert_eq!(h.state.bg, 0);
    // Без меню кнопка нажимается.
    h.click_at(free);
    assert_eq!(h.state.bg, 1);
}

fn windowed(ctx: &Ctx, s: &mut S) {
    CentralPanel::default().show(ctx, |ui| {
        for i in 0..6 {
            if ui.button(format!("Фон {i}")).clicked() {
                s.bg += 1;
            }
        }
    });
    Window::new("Окно")
        .open(&mut s.open)
        .default_width(200.0)
        .default_pos([4.0, 4.0])
        .show(ctx, |ui| {
            if ui.button("В окне").clicked() {
                s.clicks += 1;
            }
            ui.label("текст");
        });
}

#[test]
fn window_blocks_drag_close() {
    let mut h = Harness::new(
        size(),
        S {
            open: true,
            ..S::default()
        },
        windowed,
    );
    assert!(h.has("Окно"));
    assert_eq!(h.get("Окно").kind, WidgetKind::WindowTitle);
    // Окно перекрывает кнопки фона: щелчок по окну до них не доходит.
    let win = h.registry().by_label("Окно").unwrap().rect;
    let covered = h.get("Фон 0");
    let p = covered.rect.intersect(win.expand(100.0)).center();
    assert!(h.ctx.is_pointer_over_area() || h.ctx.layer_id_at(p).is_some());
    let wl = h.get("В окне").layer;
    assert_eq!(h.ctx.layer_id_at(p), Some(wl));
    h.click_at(p);
    assert_eq!(h.state.bg, 0);
    h.click("В окне");
    assert_eq!(h.state.clicks, 1);

    // Перетаскивание за заголовок.
    let t = h.get("Окно").rect;
    let before = h.get("В окне").rect;
    h.drag(t.center(), t.center() + vec2(150.0, 100.0), 5);
    let after = h.get("В окне").rect;
    assert!(
        (after.min.x - before.min.x - 150.0).abs() < 0.5,
        "{before:?} {after:?}"
    );
    assert!((after.min.y - before.min.y - 100.0).abs() < 0.5);
    // Окно не выходит за пределы главного окна.
    let t = h.get("Окно").rect;
    h.drag(t.center(), t.center() + vec2(2000.0, 2000.0), 3);
    let t = h.get("Окно").rect;
    assert!(t.max.x <= 640.5, "{t:?}");
    // Теперь кнопка фона открыта.
    h.click("Фон 0");
    assert_eq!(h.state.bg, 1);
    // Крестик закрывает окно.
    let close = h
        .registry()
        .all_by_kind(WidgetKind::CloseButton)
        .next()
        .cloned()
        .unwrap();
    assert_eq!(close.label, "Закрыть «Окно»");
    h.click_widget(&close);
    assert!(!h.state.open);
    assert!(!h.has("В окне"));
}

#[test]
fn window_raise_and_collapse() {
    struct W {
        a: bool,
        b: bool,
    }
    let build = |ctx: &Ctx, s: &mut W| {
        Window::new("А")
            .open(&mut s.a)
            .default_pos([10.0, 10.0])
            .default_width(200.0)
            .show(ctx, |ui| ui.label("в окне А"));
        Window::new("Б")
            .open(&mut s.b)
            .default_pos([60.0, 30.0])
            .default_width(200.0)
            .show(ctx, |ui| ui.label("в окне Б"));
    };
    let mut h = Harness::new(size(), W { a: true, b: true }, build);
    let p = pos2(100.0, 50.0);
    let la = h.get("в окне А").layer;
    let lb = h.get("в окне Б").layer;
    assert_eq!(h.ctx.layer_id_at(p), Some(lb));
    // Щелчок по видимой части окна А поднимает его.
    h.click_at(pos2(40.0, 15.0));
    assert_eq!(h.ctx.layer_id_at(p), Some(la));
    // Свёртывание прячет содержимое.
    let c = h
        .registry()
        .all_by_kind(WidgetKind::CollapseButton)
        .find(|w| w.label == "Свернуть «А»")
        .cloned()
        .unwrap();
    h.click_widget(&c);
    assert!(!h.has("в окне А"));
    assert!(h.has("А"));
    h.click_widget(&c);
    assert!(h.has("в окне А"));
}

fn long(ctx: &Ctx, s: &mut S) {
    Window::new("Длинное")
        .default_size([200.0, 100.0])
        .default_pos([10.0, 10.0])
        .vscroll(true)
        .show(ctx, |ui| {
            for i in 0..40 {
                if ui.button(format!("Строка {i}")).clicked() {
                    s.item = i;
                }
            }
        });
}

#[test]
fn scroll_wheel_and_bar() {
    let mut h = Harness::new(size(), S::default(), long);
    let first = h.get("Строка 0");
    assert!(first.is_visible());
    assert!(!h.get("Строка 39").is_visible());
    // Колесо.
    h.scroll_at(first.rect.center(), vec2(0.0, -60.0));
    h.step();
    let moved = h.get("Строка 0").rect;
    assert!(
        (moved.min.y - (first.rect.min.y - 60.0)).abs() < 0.5,
        "{moved:?}"
    );
    // Перетаскивание полосы до конца.
    let win = h.registry().by_label("Длинное").unwrap().rect;
    let bar_x = win.max.x - 6.0 - 4.0;
    let y0 = h.get("Строка 1").visible_rect.min.y;
    h.drag(
        pos2(bar_x + 2.0, y0 + 20.0),
        pos2(bar_x + 2.0, y0 + 600.0),
        4,
    );
    assert!(h.get("Строка 39").is_visible());
    assert!(!h.get("Строка 0").is_visible());
    // Щелчок по прокрученной кнопке.
    h.click("Строка 39");
    assert_eq!(h.state.item, 39);
    // Невидимую кнопку стенд щёлкает «без мыши».
    h.click("Строка 0");
    assert_eq!(h.state.item, 0);
}

fn rows(ctx: &Ctx, _s: &mut S) {
    CentralPanel::default().show(ctx, |ui| {
        for (name, ok) in [("gamma", true), ("tetris", false)] {
            ui.horizontal(|ui| {
                ui.add_enabled(ok, Button::new("Загрузить"));
                ui.label(RichText::new(format!("{name} — программа")).strong())
                    .on_hover_text("подсказка");
            });
        }
        Grid::new("g").striped(true).show(ui, |ui| {
            for k in ["F2", "F3"] {
                ui.monospace(k);
                ui.button("Пуск");
                ui.end_row();
            }
        });
        ScrollArea::vertical().show(ui, |ui| {
            ui.label("в прокрутке");
        });
    });
}

#[test]
fn registry_rows() {
    let h = Harness::new(size(), S::default(), rows);
    let reg = h.registry();
    assert_eq!(reg.all_by_label("Загрузить").count(), 2);
    let g = reg.in_row_with_prefix("gamma — ", "Загрузить").unwrap();
    assert!(g.enabled);
    let t = reg.in_row_with_prefix("tetris — ", "Загрузить").unwrap();
    assert!(!t.enabled);
    assert_ne!(g.id, t.id);
    assert_eq!(
        reg.by_label("gamma — программа")
            .unwrap()
            .hover_text
            .as_deref(),
        Some("подсказка")
    );
    // Строки таблицы.
    let f3 = reg.in_row_with_prefix("F3", "Пуск").unwrap();
    let f2 = reg.in_row_with_prefix("F2", "Пуск").unwrap();
    assert!(f3.rect.min.y > f2.rect.min.y);
    // Столбцы таблицы выровнены.
    assert_eq!(f3.rect.min.x, f2.rect.min.x);
    assert!(reg.has_containing("прокрутке"));
    assert!(!reg.has("нет такого"));
}

#[test]
fn combo_box_select() {
    let build = |ctx: &Ctx, s: &mut S| {
        CentralPanel::default().show(ctx, |ui| {
            ComboBox::from_id_salt("dir")
                .selected_text(["IN/OUT", "IN", "OUT"][s.combo as usize])
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.combo, 0, "IN/OUT");
                    ui.selectable_value(&mut s.combo, 1, "IN");
                    ui.selectable_value(&mut s.combo, 2, "OUT");
                });
        });
    };
    let mut h = Harness::new(size(), S::default(), build);
    h.click("IN/OUT");
    assert!(h.has("OUT"));
    assert_eq!(h.get_all("IN/OUT").len(), 2);
    h.click("OUT");
    assert_eq!(h.state.combo, 2);
    assert!(!h.has("IN"));
    assert_eq!(h.get("OUT").kind, WidgetKind::ComboBox);
}

#[test]
fn tooltip_after_delay() {
    let build = |ctx: &Ctx, _s: &mut S| {
        CentralPanel::default().show(ctx, |ui| {
            ui.button("Кнопка").on_hover_text("подсказка к кнопке");
        });
    };
    let mut h = Harness::new(size(), S::default(), build);
    let r = h.get("Кнопка").rect;
    h.move_to(r.center());
    // Есть ли под кнопкой пиксели, отличные от фона.
    let marked = |h: &Harness<S>| {
        (r.max.y as u32 + 2..r.max.y as u32 + 30).any(|y| {
            (r.min.x as u32..r.min.x as u32 + 100).any(|x| h.pixel(x, y) != h.pixel(600, 470))
        })
    };
    h.step();
    assert!(!marked(&h), "подсказка до задержки");
    h.run(40);
    assert!(marked(&h), "подсказка не нарисована");
    // Мышь ушла — подсказки нет.
    h.move_to(pos2(600.0, 400.0));
    assert!(!marked(&h));
}

#[test]
fn icons_drawn_instead_of_missing_glyphs() {
    let build = |ctx: &Ctx, _s: &mut S| {
        CentralPanel::default()
            .frame(Frame {
                fill: Color::BLACK,
                inner_margin: 0.0,
            })
            .show(ctx, |ui| {
                ui.label(RichText::new("⏸").color(Color::WHITE).size(40.0));
            });
    };
    let h = Harness::new(size(), S::default(), build);
    let r = h.get("⏸").rect;
    assert!(r.width() > 30.0);
    // Две светлые полосы паузы и тёмный промежуток между ними.
    let y = r.center().y as u32;
    let lit: Vec<bool> = (r.min.x as u32..r.max.x as u32)
        .map(|x| h.pixel(x, y).r > 128)
        .collect();
    let runs = lit.windows(2).filter(|w| !w[0] && w[1]).count();
    assert_eq!(runs, 2, "{lit:?}");
}

#[test]
fn panels_layout_and_background_canvas() {
    let build = |ctx: &Ctx, s: &mut S| {
        TopBottomPanel::top("t").show(ctx, |ui| ui.label("верх"));
        TopBottomPanel::bottom("b1").show(ctx, |ui| ui.label("низ 1"));
        TopBottomPanel::bottom("b2").show(ctx, |ui| ui.label("низ 2"));
        CentralPanel::default().frame(Frame::NONE).show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            let resp = ui.allocate_rect(rect, Sense::click());
            if resp.clicked() {
                s.bg += 1;
            }
            ui.canvas_at(rect, |c| c.fill_rect(rect, 0.0, Color::RED));
        });
    };
    let mut h = Harness::new(size(), S::default(), build);
    let top = h.get("верх").rect;
    let b1 = h.get("низ 1").rect;
    let b2 = h.get("низ 2").rect;
    assert!(top.max.y < 30.0);
    assert!(b1.min.y > b2.max.y, "первая нижняя панель — самая нижняя");
    assert!(b1.max.y <= 480.0);
    // Центральная панель залита рисунком на холсте, панели — поверх своим фоном.
    assert_eq!(h.pixel(320, 240), Color::RED);
    assert_ne!(h.pixel(320, top.center().y as u32), Color::RED);
    h.click_at(pos2(320.0, 240.0));
    assert_eq!(h.state.bg, 1);
}

#[test]
fn columns_wrap_and_auto_window() {
    let build = |ctx: &Ctx, s: &mut S| {
        TopBottomPanel::bottom("keys").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for i in 0..12 {
                    ui.button(format!("Клавиша F{i}"));
                }
            });
        });
        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.columns(2, |cols| {
                    cols[0].label("левая");
                    cols[0].separator();
                    cols[0].label("левая 2");
                    cols[1].label("правая");
                });
                ui.label("под колонками");
            });
        });
        Window::new("О программе")
            .open(&mut s.open)
            .default_pos([100.0, 100.0])
            .show(ctx, |ui| {
                ui.label("Эмулятор микро-ЭВМ «ЮТ-88»");
                ui.label("по журналу «ЮТ для умелых рук», 1989–1990.");
            });
    };
    let h = Harness::new(
        size(),
        S {
            open: true,
            ..S::default()
        },
        build,
    );
    let reg = h.registry();
    // Панель клавиш переносится на вторую строку.
    let f0 = reg.by_label("Клавиша F0").unwrap().rect;
    let f11 = reg.by_label("Клавиша F11").unwrap().rect;
    assert!(f11.min.y > f0.max.y, "{f0:?} {f11:?}");
    assert!(f11.max.x <= 640.0);
    // Колонки рядом, текст под ними — ниже обеих.
    let l = reg.by_label("левая 2").unwrap().rect;
    let r = reg.by_label("правая").unwrap().rect;
    assert!(r.min.x > 300.0 && l.min.x < 100.0);
    assert!(reg.by_label("под колонками").unwrap().rect.min.y > l.max.y);
    // Окно без заданной ширины — по содержимому.
    let t = reg.by_label("О программе").unwrap().rect;
    let line = reg
        .by_label("по журналу «ЮТ для умелых рук», 1989–1990.")
        .unwrap()
        .rect;
    assert!(t.width() < line.width() + 30.0, "{t:?} {line:?}");
    assert!(t.width() > line.width());
}

#[test]
fn hidpi_layout_in_points() {
    let mut h = Harness::with_ppp(size(), 2.0, S::default(), buttons);
    let r = h.get("Жми").rect;
    assert!(r.max.x < 100.0, "{r:?}");
    assert_eq!(h.ctx.with_canvas(|c| (c.width(), c.height())), (1280, 960));
    h.click("Жми");
    assert_eq!(h.state.clicks, 1);
}
