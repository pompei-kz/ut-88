//! Экран дисплейного модуля: 28 строк × 64 знакоместа (doc/29).
//!
//! Растр: знакоместо 6 × 10 точек — 8 строк образа из знакогенератора и 2 погашенные
//! строки снизу; бит 5 байта знакогенератора — левая точка, 0 в бите — точка светится.
//! Инверсия («курсор»): знакоместо N инвертируется, если бит 7 установлен у ячейки N + 1.

/// Символов в строке.
pub const COLS: usize = 64;
/// Видимых строк.
pub const ROWS: usize = 28;
/// Ширина знакоместа, точек.
pub const CELL_W: usize = 6;
/// Высота знакоместа, строк растра.
pub const CELL_H: usize = 10;
/// Строк образа символа (из знакогенератора).
pub const GLYPH_H: usize = 8;
/// Ширина растра, точек.
pub const WIDTH: usize = COLS * CELL_W;
/// Высота растра, строк.
pub const HEIGHT: usize = ROWS * CELL_H;
/// Смещение видимой страницы в экранном ОЗУ: адрес E800H, A11 не декодируется → 0.
pub const PAGE_OFFSET: usize = 0;

/// Снимок текстового экрана.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    /// Коды символов (биты 0–6) построчно, `ROWS × COLS`.
    pub codes: Vec<u8>,
    /// Знакоместо показывается инверсно.
    pub inverse: Vec<bool>,
}

impl Screen {
    /// Построить по экранному ОЗУ (2 Кбайта; строка × 40H + столбец).
    pub fn from_vram(vram: &[u8]) -> Self {
        let n = ROWS * COLS;
        let cell = |i: usize| vram[(PAGE_OFFSET + i) & 0x7FF];
        Self {
            codes: (0..n).map(|i| cell(i) & 0x7F).collect(),
            inverse: (0..n).map(|i| cell(i + 1) & 0x80 != 0).collect(),
        }
    }

    /// Код символа в строке `row`, столбце `col`.
    pub fn code(&self, row: usize, col: usize) -> u8 {
        self.codes[row * COLS + col]
    }

    /// Текст строки (КОИ-7 → Unicode, коды 00H–1FH — пробелы), без хвостовых пробелов.
    pub fn line(&self, row: usize) -> String {
        let s: String = self.codes[row * COLS..(row + 1) * COLS]
            .iter()
            .map(|&c| koi7_to_char(c))
            .collect();
        s.trim_end().to_string()
    }

    /// Весь текст экрана, строки через `\n`.
    pub fn text(&self) -> String {
        (0..ROWS)
            .map(|r| self.line(r))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Символ КОИ-7 Н2 знакогенератора «ЮТ-88» (doc/26) в Unicode.
pub fn koi7_to_char(c: u8) -> char {
    const RUS: [char; 31] = [
        'Ю', 'А', 'Б', 'Ц', 'Д', 'Е', 'Ф', 'Г', 'Х', 'И', 'Й', 'К', 'Л', 'М', 'Н', 'О', 'П', 'Я',
        'Р', 'С', 'Т', 'У', 'Ж', 'В', 'Ь', 'Ы', 'З', 'Ш', 'Э', 'Щ', 'Ч',
    ];
    match c & 0x7F {
        0x00..=0x1F => ' ',
        0x24 => '¤',
        c @ 0x20..=0x5F => char::from(c),
        0x7F => '■',
        c => RUS[usize::from(c - 0x60)],
    }
}

/// Нарисовать экран в буфер RGBA `WIDTH × HEIGHT × 4` байта.
///
/// `fg` — цвет светящейся точки, `bg` — фон.
pub fn render(vram: &[u8], chargen: &[u8], rgba: &mut [u8], fg: [u8; 4], bg: [u8; 4]) {
    assert!(rgba.len() >= WIDTH * HEIGHT * 4, "мал буфер RGBA");
    let scr = Screen::from_vram(vram);
    for row in 0..ROWS {
        for col in 0..COLS {
            let i = row * COLS + col;
            let code = usize::from(scr.codes[i]);
            let inv = scr.inverse[i];
            for line in 0..CELL_H {
                // Строки 8–9 погашены (точки тёмные до «исключающего ИЛИ» инверсии).
                let bits = if line < GLYPH_H {
                    chargen.get(code * 8 + line).copied().unwrap_or(0xFF)
                } else {
                    0xFF
                };
                let y = row * CELL_H + line;
                for dot in 0..CELL_W {
                    let lit = (bits >> (5 - dot)) & 1 == 0;
                    let px = if lit != inv { fg } else { bg };
                    let x = col * CELL_W + dot;
                    let o = (y * WIDTH + x) * 4;
                    rgba[o..o + 4].copy_from_slice(&px);
                }
            }
        }
    }
}
