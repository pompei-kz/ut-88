//! Демонстрационный кадр набора виджетов `ui` в PNG: строка меню с открытым меню
//! «Машина», плавающее окно с полями ввода и таблицей, панель F-клавиш и строка
//! состояния.
//!
//! Запуск: `cargo run -p ut88-gui --example ui_png [-- ФАЙЛ.png [МАСШТАБ]]`.
//! Окно не открывается: кадр рисуется в памяти стендом `ui::testing::Harness`.

use ut88_gui::gfx::{Color, vec2};
use ut88_gui::ui::testing::Harness;
use ut88_gui::ui::{
    Button, CentralPanel, ComboBox, Ctx, Frame, Grid, KeyCode, MenuBar, RichText, Slider, TextEdit,
    TopBottomPanel, Window,
};

/// Состояние демонстрации.
struct Demo {
    speed: usize,
    config: usize,
    irq: bool,
    rom: bool,
    pc_to_pad: bool,
    load_path: String,
    load_addr: String,
    save_start: String,
    save_end: String,
    format_f: bool,
    volume: f32,
    port_dir: u8,
    tape_open: bool,
}

const SPEEDS: [&str; 8] = ["×0.25", "×0.5", "×1", "×2", "×4", "×8", "×16", "максимум"];

const FKEYS: [&str; 12] = [
    "F2 УСТ",
    "F3 Холод",
    "F4 Пауза",
    "F5 Скор+",
    "Shift+F5 Скор-",
    "F6 Снимок",
    "F7 Загр",
    "F8 Лента",
    "F9 Прогр",
    "F10 Выход",
    "F11 Справка",
    "F12 Отладчик",
];

fn build(ctx: &Ctx, s: &mut Demo) {
    TopBottomPanel::top("menu").show(ctx, |ui| {
        MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Машина", |ui| {
                if ui.button("УСТ — сброс (F2)").clicked() {
                    ui.close();
                }
                ui.button("Холодный старт — выкл./вкл. питания (F3)");
                ui.button("Пауза (F4)");
                ui.separator();
                ui.label("Скорость (F5 — выше, Shift+F5 — ниже):");
                for (i, l) in SPEEDS.iter().enumerate() {
                    ui.radio_value(&mut s.speed, i, *l);
                }
                ui.separator();
                ui.label("Конфигурация (холодный старт):");
                for (i, l) in [
                    "минимальная (модуль 1)",
                    "полная (МОНИТОР-F, часы в нём)",
                    "полная + МОНИТОР модуля 1",
                    "CP/M и ОС ЮТ-88 (полная + квазидиск 256К)",
                ]
                .iter()
                .enumerate()
                {
                    if ui.radio(s.config == i, *l).clicked() {
                        s.config = i;
                    }
                }
                ui.checkbox(&mut s.irq, "Прерывания от часов");
                ui.checkbox(&mut s.rom, "ПЗУ МОНИТОРА-F (сегмент F000H–FFFFH)");
                ui.checkbox(&mut s.pc_to_pad, "Клавиатура ПК → кнопки модуля 1");
            });
            ui.menu_button("Магнитофон", |ui| {
                ui.button("Вставить кассету (WAV)…");
            });
            ui.menu_button("Файл", |_| {});
            ui.menu_button("CP/M", |_| {});
            ui.menu_button("Программы", |_| {});
            ui.menu_button("Вид", |ui| {
                ui.add(Slider::new(&mut s.volume, 0.0..=1.0).text("громкость"));
            });
            ui.menu_button("Справка", |_| {});
        });
    });
    TopBottomPanel::bottom("status").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("⏸ пауза").color(Color::YELLOW));
            ui.separator();
            ui.label("Конфигурация: полная");
            ui.separator();
            ui.label("Скорость: ×1 (факт. 0 %)");
            ui.separator();
            ui.label("Время: 12.3 с");
            ui.separator();
            ui.label("кассеты нет");
            ui.separator();
            ui.label("🔊");
        });
    });
    TopBottomPanel::bottom("fkeys").show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            for k in FKEYS {
                ui.button(RichText::new(k).monospace())
                    .on_hover_text("подсказка");
            }
        });
    });
    CentralPanel::default().frame(Frame::NONE).show(ctx, |ui| {
        let rect = ui.available_rect_before_wrap();
        ui.canvas_at(rect, |c| {
            c.fill_rect(rect, 0.0, Color::rgb(0x2C, 0x2A, 0x2E));
            c.fill_rect(rect.shrink(60.0), 14.0, Color::rgb(0xD9, 0xD0, 0xB8));
        });
    });
    Window::new("Магнитофон и файлы")
        .open(&mut s.tape_open)
        .default_width(520.0)
        .default_pos([300.0, 150.0])
        .show(ctx, |ui| {
            ui.heading("Кассета");
            ui.horizontal(|ui| {
                ui.add_enabled(false, Button::new("▶ Воспроизведение"));
                ui.add_enabled(false, Button::new("⏮ В начало"));
                ui.label("кассеты нет");
            });
            ui.label(
                "Вставленная кассета стоит; лента трогается сама, когда программа \
                 начинает читать вход магнитофона (директива A МОНИТОРА модуля 1, I \
                 МОНИТОРА-F), или кнопкой «Воспроизведение».",
            );
            ui.separator();
            ui.heading("Быстрая загрузка .bin");
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut s.load_path)
                        .desired_width(380.0)
                        .hint_text("путь к файлу"),
                );
                ui.button("Обзор…");
            });
            ui.horizontal(|ui| {
                ui.label("адрес:");
                ui.add(TextEdit::singleline(&mut s.load_addr).desired_width(60.0));
                ui.button("Загрузить");
                ui.button("Загрузить и запустить");
            });
            ui.separator();
            ui.heading("Сохранить диапазон памяти в .bin");
            ui.horizontal(|ui| {
                ui.label("с");
                ui.add(TextEdit::singleline(&mut s.save_start).desired_width(60.0));
                ui.label("по");
                ui.add(TextEdit::singleline(&mut s.save_end).desired_width(60.0));
                ui.radio_value(&mut s.format_f, false, "МОНИТОР модуля 1");
                ui.radio_value(&mut s.format_f, true, "МОНИТОР-F");
            });
            ui.horizontal(|ui| {
                ui.label("порт:");
                ComboBox::from_id_salt("dir")
                    .width(70.0)
                    .selected_text(["IN/OUT", "IN", "OUT"][s.port_dir as usize])
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut s.port_dir, 0, "IN/OUT");
                        ui.selectable_value(&mut s.port_dir, 1, "IN");
                        ui.selectable_value(&mut s.port_dir, 2, "OUT");
                    });
                ui.small_button("✖");
            });
            ui.separator();
            ui.heading("Горячие клавиши");
            Grid::new("hot").striped(true).show(ui, |ui| {
                for (k, what) in [
                    ("F2", "УСТ — сброс"),
                    ("F3", "холодный старт"),
                    ("F4", "пауза / продолжить"),
                    ("F5 / Shift+F5", "скорость выше / ниже"),
                    ("F6 / F7", "быстрый снимок / загрузить"),
                ] {
                    ui.monospace(k);
                    ui.label(what);
                    ui.end_row();
                }
            });
        });
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| {
        "/tmp/claude-1000/-home-pompei-var-ut-88/a5824d08-a20a-4850-b96c-b02c672361c9/scratchpad/ui-demo.png"
            .into()
    });
    let ppp: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let demo = Demo {
        speed: 2,
        config: 1,
        irq: true,
        rom: true,
        pc_to_pad: false,
        load_path: String::new(),
        load_addr: "C000".into(),
        save_start: "C000".into(),
        save_end: "C3FF".into(),
        format_f: false,
        volume: 0.7,
        port_dir: 0,
        tape_open: true,
    };
    let mut h = Harness::with_ppp(vec2(1280.0, 900.0), ppp, demo, build);
    h.settle();
    // Фокус в поле адреса и открытое меню «Машина» с мышью над пунктом.
    let addr = h.get_by_value("C000");
    h.click_widget(&addr);
    h.click("Машина");
    let item = h.get("Пауза (F4)");
    h.move_to(item.rect.center());
    h.settle();
    h.ctx
        .with_canvas(|c| c.pixmap().save_png(&out))
        .expect("не удалось записать PNG");
    println!("{out}");

    // Второй кадр: меню закрыто, открыт выпадающий список.
    h.key(KeyCode::Escape);
    h.click("IN/OUT");
    let out2 = out.replace(".png", "-2.png");
    h.ctx
        .with_canvas(|c| c.pixmap().save_png(&out2))
        .expect("не удалось записать PNG");
    println!("{out2}");
}
