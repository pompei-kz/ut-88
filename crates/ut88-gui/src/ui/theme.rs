//! Тема: цвета и размеры виджетов (тёмная, близкая к прежнему виду окна).

use crate::gfx::{Color, Vec2, vec2};

/// Вид виджета в одном состоянии (покой, наведение, нажатие…).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WidgetVisuals {
    /// Заливка «сильная» (флажок, переключатель, ползунок).
    pub bg_fill: Color,
    /// Заливка «слабая» (кнопки).
    pub weak_bg_fill: Color,
    /// Рамка: толщина и цвет (толщина 0 — без рамки).
    pub bg_stroke: (f32, Color),
    /// Цвет текста и значков.
    pub fg: Color,
    /// Толщина линий значков (галочка и т. п.).
    pub fg_width: f32,
    /// Скругление углов.
    pub radius: f32,
    /// На сколько точек рамка рисуется шире прямоугольника виджета.
    pub expansion: f32,
}

/// Цвета и размеры набора виджетов.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Фон панелей (строка меню, строка состояния).
    pub panel_fill: Color,
    /// Фон плавающих окон и меню.
    pub window_fill: Color,
    /// Рамка окон и меню.
    pub window_stroke: Color,
    /// Скругление окон.
    pub window_radius: f32,
    /// Скругление выпадающих меню и подсказок.
    pub menu_radius: f32,
    /// Тень окон и меню.
    pub shadow: Color,
    /// Фон полей ввода.
    pub extreme_bg: Color,
    /// Полосы таблиц (`Grid::striped`).
    pub faint_bg: Color,
    /// Обычный текст.
    pub text_color: Color,
    /// Выделенный текст (`strong`, заголовки).
    pub strong_text: Color,
    /// Бледный текст (подсказка в пустом поле ввода).
    pub weak_text: Color,
    /// Вид невзаимодействующих элементов (подписи, разделители).
    pub noninteractive: WidgetVisuals,
    /// Виджет в покое.
    pub inactive: WidgetVisuals,
    /// Под мышью.
    pub hovered: WidgetVisuals,
    /// Нажат.
    pub active: WidgetVisuals,
    /// Открытое меню / выпадающий список.
    pub open: WidgetVisuals,
    /// Фон выделения (текст в поле, выбранный пункт).
    pub selection_bg: Color,
    /// Рамка и текст выделения.
    pub selection_stroke: Color,
    /// Текстовый курсор.
    pub cursor_color: Color,
    /// Множитель непрозрачности недоступных виджетов.
    pub disabled_alpha: f32,

    /// Промежуток между виджетами (по X — в ряду, по Y — в столбце).
    pub item_spacing: Vec2,
    /// Отступ текста кнопки от её края.
    pub button_padding: Vec2,
    /// Высота виджетов (кнопки, поля, ряды).
    pub interact_height: f32,
    /// Отступ содержимого окна от рамки.
    pub window_margin: f32,
    /// Отступ пунктов меню от рамки.
    pub menu_margin: f32,
    /// Отступ содержимого панелей (`TopBottomPanel`).
    pub panel_margin: Vec2,
    /// Отступ содержимого центральной панели (с рамкой по умолчанию).
    pub central_margin: f32,
    /// Размер значка флажка/переключателя.
    pub icon_width: f32,
    /// Внутренний размер значка (галочка, точка).
    pub icon_width_inner: f32,
    /// Промежуток между значком и подписью.
    pub icon_spacing: f32,
    /// Длина ползунка.
    pub slider_width: f32,
    /// Толщина дорожки ползунка.
    pub slider_rail: f32,
    /// Ширина выпадающего списка по умолчанию.
    pub combo_width: f32,
    /// Ширина поля ввода по умолчанию.
    pub text_edit_width: f32,
    /// Отступ текста в поле ввода.
    pub text_edit_margin: Vec2,
    /// Ширина полосы прокрутки.
    pub scroll_bar_width: f32,
    /// Высота, которую занимает разделитель.
    pub separator_spacing: f32,
    /// Наибольшая ширина текста подсказки и окна без заданной ширины.
    pub wrap_width: f32,
    /// Задержка перед показом подсказки, с.
    pub tooltip_delay: f64,

    /// Кегль обычного текста.
    pub body_size: f32,
    /// Кегль моноширинного текста.
    pub mono_size: f32,
    /// Кегль заголовка (`heading`).
    pub heading_size: f32,
    /// Кегль мелкого текста (`small`).
    pub small_size: f32,
    /// Кегль заголовка окна.
    pub title_size: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::dark()
    }
}

impl Theme {
    /// Тёмная тема (цвета и отступы прежнего вида окна).
    pub fn dark() -> Self {
        let g = Color::from_gray;
        Theme {
            panel_fill: g(27),
            window_fill: g(27),
            window_stroke: g(60),
            window_radius: 6.0,
            menu_radius: 6.0,
            shadow: Color::from_black_alpha(96),
            extreme_bg: g(10),
            faint_bg: g(32),
            text_color: g(140),
            strong_text: g(255),
            weak_text: g(90),
            noninteractive: WidgetVisuals {
                bg_fill: g(27),
                weak_bg_fill: g(27),
                bg_stroke: (1.0, g(60)),
                fg: g(140),
                fg_width: 1.0,
                radius: 2.0,
                expansion: 0.0,
            },
            inactive: WidgetVisuals {
                bg_fill: g(60),
                weak_bg_fill: g(60),
                bg_stroke: (0.0, Color::TRANSPARENT),
                fg: g(180),
                fg_width: 1.0,
                radius: 2.0,
                expansion: 0.0,
            },
            hovered: WidgetVisuals {
                bg_fill: g(70),
                weak_bg_fill: g(70),
                bg_stroke: (1.0, g(150)),
                fg: g(240),
                fg_width: 1.5,
                radius: 3.0,
                expansion: 1.0,
            },
            active: WidgetVisuals {
                bg_fill: g(55),
                weak_bg_fill: g(55),
                bg_stroke: (1.0, g(255)),
                fg: g(255),
                fg_width: 2.0,
                radius: 2.0,
                expansion: 1.0,
            },
            open: WidgetVisuals {
                bg_fill: g(27),
                weak_bg_fill: g(45),
                bg_stroke: (1.0, g(60)),
                fg: g(210),
                fg_width: 1.0,
                radius: 2.0,
                expansion: 0.0,
            },
            selection_bg: Color::rgb(0, 92, 128),
            selection_stroke: Color::rgb(192, 222, 255),
            cursor_color: Color::rgb(192, 222, 255),
            disabled_alpha: 0.45,

            item_spacing: vec2(8.0, 3.0),
            button_padding: vec2(4.0, 1.0),
            interact_height: 18.0,
            window_margin: 6.0,
            menu_margin: 6.0,
            panel_margin: vec2(8.0, 2.0),
            central_margin: 8.0,
            icon_width: 14.0,
            icon_width_inner: 8.0,
            icon_spacing: 4.0,
            slider_width: 100.0,
            slider_rail: 8.0,
            combo_width: 100.0,
            text_edit_width: 280.0,
            text_edit_margin: vec2(4.0, 2.0),
            scroll_bar_width: 8.0,
            separator_spacing: 6.0,
            wrap_width: 420.0,
            tooltip_delay: 0.5,

            body_size: 12.5,
            mono_size: 12.0,
            heading_size: 18.0,
            small_size: 9.0,
            title_size: 14.0,
        }
    }
}
