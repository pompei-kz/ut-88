//! Список виджетов кадра — для тестов (подписи, прямоугольники, доступность).

use crate::gfx::Rect;

use super::{Id, LayerId};

/// Вид виджета.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WidgetKind {
    /// Подпись (в т. ч. заголовок, моноширинный текст).
    Label,
    /// Кнопка (в т. ч. пункт меню).
    Button,
    /// Флажок.
    Checkbox,
    /// Переключатель.
    Radio,
    /// Выбираемая подпись (пункт выпадающего списка).
    Selectable,
    /// Ползунок.
    Slider,
    /// Поле ввода.
    TextEdit,
    /// Выпадающий список (кнопка).
    ComboBox,
    /// Кнопка меню (верхний пункт строки меню или пункт с вложенным меню).
    MenuButton,
    /// Заголовок плавающего окна.
    WindowTitle,
    /// Крестик закрытия окна (подпись — `Закрыть «заголовок»`).
    CloseButton,
    /// Свёртывание окна (подпись — `Свернуть «заголовок»`).
    CollapseButton,
}

/// Виджет кадра.
#[derive(Debug, Clone, PartialEq)]
pub struct WidgetInfo {
    /// Id (стабилен между кадрами, пока не меняется раскладка).
    pub id: Id,
    /// Вид.
    pub kind: WidgetKind,
    /// Подпись (у поля ввода — подсказка `hint_text`).
    pub label: String,
    /// Значение: текст поля, значение ползунка, выбранный пункт списка.
    pub value: Option<String>,
    /// Состояние флажка/переключателя/выбираемой подписи.
    pub selected: Option<bool>,
    /// Прямоугольник (логические точки).
    pub rect: Rect,
    /// Видимая часть прямоугольника (с учётом отсечения и прокрутки).
    pub visible_rect: Rect,
    /// Доступен.
    pub enabled: bool,
    /// Слой.
    pub layer: LayerId,
    /// Ряд: id горизонтального ряда (`Ui::horizontal`) или строки таблицы (`Grid`).
    pub row: Option<Id>,
    /// Подсказка при наведении (`on_hover_text`).
    pub hover_text: Option<String>,
}

impl WidgetInfo {
    /// Виден хоть частично.
    pub fn is_visible(&self) -> bool {
        self.visible_rect.is_positive()
    }
}

/// Виджеты кадра в порядке добавления.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Registry {
    /// Виджеты.
    pub widgets: Vec<WidgetInfo>,
}

impl Registry {
    /// Все виджеты с подписью `label` (точное совпадение).
    pub fn all_by_label<'a>(&'a self, label: &'a str) -> impl Iterator<Item = &'a WidgetInfo> {
        self.widgets.iter().filter(move |w| w.label == label)
    }

    /// Первый виджет с подписью `label`.
    pub fn by_label(&self, label: &str) -> Option<&WidgetInfo> {
        self.widgets.iter().find(|w| w.label == label)
    }

    /// Единственный виджет с подписью `label`; паника, если таких нет или несколько.
    pub fn get_by_label(&self, label: &str) -> &WidgetInfo {
        let mut it = self.widgets.iter().filter(|w| w.label == label);
        let w = it.next().unwrap_or_else(|| panic!("нет виджета «{label}»"));
        assert!(it.next().is_none(), "несколько виджетов «{label}»");
        w
    }

    /// Есть виджет с подписью `label`.
    pub fn has(&self, label: &str) -> bool {
        self.by_label(label).is_some()
    }

    /// Есть виджет, подпись которого содержит `part`.
    pub fn has_containing(&self, part: &str) -> bool {
        self.widgets.iter().any(|w| w.label.contains(part))
    }

    /// Виджеты, подпись которых содержит `part`.
    pub fn all_containing<'a>(&'a self, part: &'a str) -> impl Iterator<Item = &'a WidgetInfo> {
        self.widgets.iter().filter(move |w| w.label.contains(part))
    }

    /// Виджеты со значением `value` (например, поле ввода с текстом).
    pub fn all_by_value<'a>(&'a self, value: &'a str) -> impl Iterator<Item = &'a WidgetInfo> {
        self.widgets
            .iter()
            .filter(move |w| w.value.as_deref() == Some(value))
    }

    /// Виджеты вида `kind`.
    pub fn all_by_kind(&self, kind: WidgetKind) -> impl Iterator<Item = &WidgetInfo> {
        self.widgets.iter().filter(move |w| w.kind == kind)
    }

    /// Виджет по id.
    pub fn by_id(&self, id: Id) -> Option<&WidgetInfo> {
        self.widgets.iter().find(|w| w.id == id)
    }

    /// Виджет с подписью `label` в том же ряду, что и первая подпись, удовлетворяющая
    /// `row_pred` (например, «кнопка „Загрузить“ в строке программы `gamma — …`»).
    /// Ряд — общий `Ui::horizontal` или строка `Grid`; если рядов нет — та же высота
    /// центра (±4 точки).
    pub fn in_row_of(
        &self,
        row_pred: impl Fn(&WidgetInfo) -> bool,
        label: &str,
    ) -> Option<&WidgetInfo> {
        let anchor = self
            .widgets
            .iter()
            .find(|w| w.kind == WidgetKind::Label && row_pred(w))?;
        self.widgets
            .iter()
            .filter(|w| w.label == label)
            .find(|w| match (anchor.row, w.row) {
                (Some(a), Some(b)) => a == b,
                _ => (w.rect.center().y - anchor.rect.center().y).abs() < 4.0,
            })
    }

    /// Виджет `label` в ряду, где есть подпись, начинающаяся с `prefix`.
    pub fn in_row_with_prefix(&self, prefix: &str, label: &str) -> Option<&WidgetInfo> {
        self.in_row_of(|w| w.label.starts_with(prefix), label)
    }

    /// Прямоугольник по умолчанию для поиска (центр видимой части).
    pub fn click_point(w: &WidgetInfo) -> Option<crate::gfx::Pos2> {
        let r: Rect = w.visible_rect;
        r.is_positive().then(|| r.center())
    }
}
