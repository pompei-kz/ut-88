//! Файловые диалоги — через трейт, чтобы в тестах окна вместо настоящего диалога `rfd`
//! подставлять заранее заданные пути.

use std::collections::VecDeque;
use std::path::PathBuf;

/// Выбор файла для открытия или сохранения.
pub trait FileDialogs {
    /// Выбрать существующий файл. `name` — подпись фильтра, `exts` — расширения.
    fn pick_open(&mut self, name: &str, exts: &[&str]) -> Option<PathBuf>;
    /// Выбрать файл для записи; `default` — предлагаемое имя.
    fn pick_save(&mut self, name: &str, exts: &[&str], default: &str) -> Option<PathBuf>;
}

/// Настоящие системные диалоги (`rfd`), по возможности привязанные к окну эмулятора.
#[derive(Default)]
pub struct RfdDialogs {
    parent: Option<std::rc::Rc<winit::window::Window>>,
}

impl RfdDialogs {
    /// Диалоги поверх окна `parent`.
    pub fn with_parent(parent: std::rc::Rc<winit::window::Window>) -> Self {
        Self {
            parent: Some(parent),
        }
    }

    fn dialog(&self) -> rfd::FileDialog {
        let d = rfd::FileDialog::new();
        match &self.parent {
            Some(w) => d.set_parent(w.as_ref()),
            None => d,
        }
    }
}

impl FileDialogs for RfdDialogs {
    fn pick_open(&mut self, name: &str, exts: &[&str]) -> Option<PathBuf> {
        self.dialog()
            .add_filter(name, exts)
            .add_filter("Все файлы", &["*"])
            .set_directory(std::env::current_dir().unwrap_or_default())
            .pick_file()
    }

    fn pick_save(&mut self, name: &str, exts: &[&str], default: &str) -> Option<PathBuf> {
        self.dialog()
            .add_filter(name, exts)
            .set_file_name(default)
            .set_directory(std::env::current_dir().unwrap_or_default())
            .save_file()
    }
}

/// Запрос к диалогу, запомненный [`ScriptedDialogs`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogRequest {
    /// `true` — открыть, `false` — сохранить.
    pub open: bool,
    /// Подпись фильтра.
    pub name: String,
    /// Предлагаемое имя (для сохранения).
    pub default: String,
}

/// Диалоги «по сценарию»: отдают пути из очереди (пустая очередь — «Отмена») и
/// запоминают запросы. Для тестов окна.
#[derive(Default, Clone)]
pub struct ScriptedDialogs {
    /// Ответы по порядку.
    pub answers: std::rc::Rc<std::cell::RefCell<VecDeque<Option<PathBuf>>>>,
    /// Запросы по порядку.
    pub requests: std::rc::Rc<std::cell::RefCell<Vec<DialogRequest>>>,
}

impl ScriptedDialogs {
    /// Следующий диалог вернёт `p` (`None` — «Отмена»).
    pub fn push(&self, p: Option<PathBuf>) {
        self.answers.borrow_mut().push_back(p);
    }

    /// Сколько ответов ещё не использовано.
    pub fn pending(&self) -> usize {
        self.answers.borrow().len()
    }

    fn answer(&mut self, open: bool, name: &str, default: &str) -> Option<PathBuf> {
        self.requests.borrow_mut().push(DialogRequest {
            open,
            name: name.into(),
            default: default.into(),
        });
        self.answers.borrow_mut().pop_front().flatten()
    }
}

impl FileDialogs for ScriptedDialogs {
    fn pick_open(&mut self, name: &str, _exts: &[&str]) -> Option<PathBuf> {
        self.answer(true, name, "")
    }

    fn pick_save(&mut self, name: &str, _exts: &[&str], default: &str) -> Option<PathBuf> {
        self.answer(false, name, default)
    }
}
