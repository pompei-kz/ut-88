//! Буфер обмена для полей ввода.

use std::cell::RefCell;
use std::rc::Rc;

/// Буфер обмена: текст.
pub trait Clipboard {
    /// Текст из буфера (`None` — пусто или недоступно).
    fn get_text(&mut self) -> Option<String>;
    /// Положить текст в буфер (ошибки молча пропускаются).
    fn set_text(&mut self, text: &str);
}

/// Системный буфер обмена через arboard. Создаётся лениво при первом обращении; если
/// буфер недоступен (нет X-сервера и т. п.), копирование и вставка просто не работают.
#[derive(Default)]
pub struct SystemClipboard {
    inner: Option<arboard::Clipboard>,
    failed: bool,
}

impl SystemClipboard {
    /// Новый (arboard ещё не создан).
    pub fn new() -> Self {
        Self::default()
    }

    fn get(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.inner.is_none() && !self.failed {
            match arboard::Clipboard::new() {
                Ok(c) => self.inner = Some(c),
                Err(_) => self.failed = true,
            }
        }
        self.inner.as_mut()
    }
}

impl Clipboard for SystemClipboard {
    fn get_text(&mut self) -> Option<String> {
        self.get()?.get_text().ok()
    }

    fn set_text(&mut self, text: &str) {
        if let Some(c) = self.get() {
            let _ = c.set_text(text.to_owned());
        }
    }
}

/// Буфер обмена в памяти — для тестов. Клоны делят одно содержимое.
#[derive(Debug, Clone, Default)]
pub struct MemoryClipboard(Rc<RefCell<Option<String>>>);

impl MemoryClipboard {
    /// Пустой буфер.
    pub fn new() -> Self {
        Self::default()
    }

    /// Содержимое.
    pub fn text(&self) -> Option<String> {
        self.0.borrow().clone()
    }

    /// Положить текст.
    pub fn set(&self, text: &str) {
        *self.0.borrow_mut() = Some(text.to_owned());
    }
}

impl Clipboard for MemoryClipboard {
    fn get_text(&mut self) -> Option<String> {
        self.text()
    }

    fn set_text(&mut self, text: &str) {
        self.set(text);
    }
}
