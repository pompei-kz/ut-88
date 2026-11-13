//! Кодировка символов в строках и символьных константах.
//!
//! По умолчанию — КОИ-7 ЮТ-88 (как знакогенератор дисплейного модуля,
//! `doc/26-znakogenerator.md`): коды 20H–5FH совпадают с ASCII, русские буквы (прописные и
//! строчные) переводятся в коды 60H–7EH (КОИ-7 Н2: Ю А Б Ц Д Е Ф Г Х И Й К Л М Н О П Я Р С Т
//! У Ж В Ь Ы З Ш Э Щ Ч), «Ё» — как «Е». Остальные символы ASCII (в том числе строчные
//! латинские 61H–7AH) записываются своими кодами. Буквы «Ъ» в знакогенераторе нет — ошибка.
//!
//! `--charset ascii` — только ASCII, русские буквы — ошибка.

/// Кодировка строк.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Charset {
    /// КОИ-7 ЮТ-88 (по умолчанию).
    #[default]
    Koi7,
    /// Только ASCII.
    Ascii,
}

/// Русские буквы в порядке кодов 60H–7EH.
const KOI7_RU: &str = "ЮАБЦДЕФГХИЙКЛМНОПЯРСТУЖВЬЫЗШЭЩЧ";

impl Charset {
    /// Название для сообщений и справки.
    pub fn name(self) -> &'static str {
        match self {
            Charset::Koi7 => "koi7",
            Charset::Ascii => "ascii",
        }
    }

    /// Разобрать имя кодировки (`koi7`, `ascii`).
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "koi7" | "koi-7" | "кои7" | "кои-7" => Some(Charset::Koi7),
            "ascii" => Some(Charset::Ascii),
            _ => None,
        }
    }

    /// Код символа или текст ошибки.
    pub fn encode(self, c: char) -> Result<u8, String> {
        if c.is_ascii() {
            return Ok(c as u8);
        }
        match self {
            Charset::Ascii => Err(format!(
                "символ «{c}» не входит в ASCII (для русских букв уберите --charset ascii)"
            )),
            Charset::Koi7 => {
                let up = match c {
                    'ё' | 'Ё' => 'Е',
                    _ => c.to_uppercase().next().unwrap_or(c),
                };
                if up == 'Ъ' {
                    return Err("буквы «Ъ» нет в знакогенераторе ЮТ-88 (КОИ-7)".to_string());
                }
                KOI7_RU
                    .chars()
                    .position(|r| r == up)
                    .map(|i| 0x60 + i as u8)
                    .ok_or_else(|| format!("символ «{c}» не представим в КОИ-7 ЮТ-88"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn koi7() {
        let k = Charset::Koi7;
        assert_eq!(k.encode('A'), Ok(0x41));
        assert_eq!(k.encode('Ю'), Ok(0x60));
        assert_eq!(k.encode('а'), Ok(0x61));
        assert_eq!(k.encode('Ц'), Ok(0x63));
        assert_eq!(k.encode('З'), Ok(0x7A));
        assert_eq!(k.encode('Ч'), Ok(0x7E));
        assert_eq!(k.encode('ё'), Ok(0x65));
        assert!(k.encode('Ъ').is_err());
        assert!(k.encode('€').is_err());
        assert!(Charset::Ascii.encode('Я').is_err());
        assert_eq!(KOI7_RU.chars().count(), 31);
    }
}
