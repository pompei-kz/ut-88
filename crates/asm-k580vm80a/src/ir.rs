//! Промежуточное представление: модуль = сегменты + перемещения + символы.
//!
//! Ассемблер всегда собирает [`Module`]. Из него получаются и готовые образы памяти
//! ([`Module::to_image`] → bin/hex/ihex), и объектный файл `.link` (его пишет/читает модуль
//! `object`, а компоновщик раскладывает несколько модулей через [`Module::place`]).
//!
//! Сегменты: [`SegKind::Abs`] (ASEG — абсолютные адреса по ORG), [`SegKind::Code`] (CSEG) и
//! [`SegKind::Data`] (DSEG) — перемещаемые, смещения в них от начала сегмента.
//! Перемещения бывают трёх видов ([`Part`]): `WORD` (2 байта, младший первым), `LOW` и
//! `HIGH` (1 байт). Цель — начало сегмента этого модуля или внешний символ, плюс
//! константа `addend`. В данных на месте перемещения лежит значение при базе 0
//! (`part.apply(addend)`), при размещении оно перезаписывается.

use crate::error::{AsmError, Loc};
use crate::expr::{Part, RelocTarget, Value};
use crate::output::{Image, ImageBuilder};

/// Вид сегмента.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SegKind {
    /// ASEG — абсолютный.
    Abs,
    /// CSEG — код (перемещаемый).
    Code,
    /// DSEG — данные (перемещаемый).
    Data,
}

impl SegKind {
    /// Все виды по порядку индексов.
    pub const ALL: [SegKind; 3] = [SegKind::Abs, SegKind::Code, SegKind::Data];

    /// Индекс в [`Module::segments`].
    pub fn index(self) -> usize {
        match self {
            SegKind::Abs => 0,
            SegKind::Code => 1,
            SegKind::Data => 2,
        }
    }

    /// Имя директивы: `ASEG`, `CSEG`, `DSEG`.
    pub fn name(self) -> &'static str {
        match self {
            SegKind::Abs => "ASEG",
            SegKind::Code => "CSEG",
            SegKind::Data => "DSEG",
        }
    }

    /// Разобрать имя сегмента (`ASEG`/`CSEG`/`DSEG`, без учёта регистра).
    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.name().eq_ignore_ascii_case(s))
    }
}

/// Непрерывный кусок данных сегмента.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    /// Смещение от начала сегмента (в ASEG — адрес).
    pub offset: u16,
    /// Байты.
    pub bytes: Vec<u8>,
}

/// Сегмент модуля.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    /// Вид.
    pub kind: SegKind,
    /// Размер: наибольшее достигнутое смещение (с учётом DS). Для ASEG не используется
    /// при размещении.
    pub size: u32,
    /// Данные по возрастанию смещений, куски не перекрываются и не соприкасаются.
    pub chunks: Vec<Chunk>,
}

impl Segment {
    /// Пустой сегмент.
    pub fn new(kind: SegKind) -> Self {
        Self {
            kind,
            size: 0,
            chunks: Vec::new(),
        }
    }

    /// Есть ли в сегменте данные или резерв.
    pub fn is_empty(&self) -> bool {
        self.size == 0 && self.chunks.is_empty()
    }
}

/// Перемещение: место в сегменте, куда при размещении записывается адрес цели.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reloc {
    /// Сегмент, в котором лежит исправляемое место.
    pub seg: SegKind,
    /// Смещение места в сегменте.
    pub offset: u16,
    /// Вид: WORD (2 байта) / LOW / HIGH (1 байт).
    pub part: Part,
    /// Цель: начало сегмента или внешний символ.
    pub target: RelocTarget,
    /// Константа, прибавляемая к адресу цели (до взятия LOW/HIGH).
    pub addend: i64,
    /// Строка исходного текста (для сообщений; в `.link` не хранится).
    pub loc: Option<Loc>,
}

impl Reloc {
    /// Сколько байт исправляется.
    pub fn width(&self) -> u16 {
        if self.part == Part::Word { 2 } else { 1 }
    }
}

/// Вид символа.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    /// Метка.
    Label,
    /// `EQU`.
    Equ,
    /// `SET` (переопределяемый).
    Set,
    /// `EXTRN` — внешний.
    Extern,
    /// `-D` из командной строки.
    Define,
}

/// Символ модуля (для листинга, карты и отладки).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    /// Имя как написано при определении.
    pub name: String,
    /// Значение (абсолютное, относительное к сегменту или внешнее).
    pub value: Value,
    /// Вид.
    pub kind: SymbolKind,
    /// Где определён (`None` — из командной строки).
    pub loc: Option<Loc>,
    /// Объявлен PUBLIC.
    pub public: bool,
}

/// Экспортируемый символ (`PUBLIC`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Public {
    /// Имя.
    pub name: String,
    /// Значение: абсолютное или относительное к CSEG/DSEG (часть всегда WORD).
    pub value: Value,
    /// Где объявлен.
    pub loc: Option<Loc>,
}

/// Импортируемый символ (`EXTRN`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extern {
    /// Имя.
    pub name: String,
    /// Где объявлен.
    pub loc: Option<Loc>,
}

/// Собранный модуль.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    /// Имя модуля (`NAME`), если задано.
    pub name: Option<String>,
    /// Сегменты по индексу [`SegKind::index`]: ASEG, CSEG, DSEG.
    pub segments: [Segment; 3],
    /// Перемещения в порядке появления.
    pub relocs: Vec<Reloc>,
    /// Экспорт (PUBLIC), в порядке объявления.
    pub publics: Vec<Public>,
    /// Импорт (EXTRN), в порядке объявления.
    pub externs: Vec<Extern>,
    /// Адрес пуска из `END адрес` (абсолютный или относительный к сегменту).
    pub entry: Option<Value>,
    /// Все символы модуля, по алфавиту.
    pub symbols: Vec<Symbol>,
}

impl Default for Module {
    fn default() -> Self {
        Self {
            name: None,
            segments: SegKind::ALL.map(Segment::new),
            relocs: Vec::new(),
            publics: Vec::new(),
            externs: Vec::new(),
            entry: None,
            symbols: Vec::new(),
        }
    }
}

/// Где разместить перемещаемые сегменты при сборке сразу в образ памяти.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Placement {
    /// Начало CSEG (по умолчанию 0 — тогда смещения CSEG совпадают с адресами).
    pub cseg: Option<u16>,
    /// Начало DSEG (по умолчанию — сразу после CSEG).
    pub dseg: Option<u16>,
}

/// Базы сегментов одного модуля при размещении: адрес начала ASEG (всегда 0), CSEG, DSEG.
pub type SegBases = [i64; 3];

impl Module {
    /// Сегмент вида `k`.
    pub fn segment(&self, k: SegKind) -> &Segment {
        &self.segments[k.index()]
    }

    /// Базы сегментов по правилу [`Placement`].
    pub fn bases(&self, p: &Placement) -> SegBases {
        let code = i64::from(p.cseg.unwrap_or(0));
        let data = p
            .dseg
            .map(i64::from)
            .unwrap_or(code + i64::from(self.segment(SegKind::Code).size));
        [0, code, data]
    }

    /// Собрать образ памяти из одного модуля (вывод bin/hex/ihex без компоновщика).
    /// Внешние символы здесь не разрешаются — ссылка на них ошибка.
    pub fn to_image(&self, p: &Placement) -> Result<Image, Vec<AsmError>> {
        let bases = self.bases(p);
        let mut img = ImageBuilder::new();
        let mut errors = self.place(&bases, &|_| None, &mut img);
        let mut entry = None;
        if let Some(v) = &self.entry {
            match resolve_value(v, &bases, &|_| None) {
                Some(a) => entry = Some((a & 0xFFFF) as u16),
                None => errors.push(AsmError::plain("адрес пуска в END — внешний символ")),
            }
        }
        if errors.is_empty() {
            Ok(img.finish(entry))
        } else {
            Err(errors)
        }
    }

    /// Разместить модуль в образе: скопировать данные сегментов по базам `bases` и применить
    /// перемещения. `externs` — адрес внешнего символа по имени (`None` — не найден).
    /// Возвращает ошибки (перекрытие, выход за 64 Кбайт, неразрешённая ссылка).
    pub fn place(
        &self,
        bases: &SegBases,
        externs: &dyn Fn(&str) -> Option<i64>,
        img: &mut ImageBuilder,
    ) -> Vec<AsmError> {
        let mut errors = Vec::new();
        let module = self.name.as_deref().map(|n| format!("модуль {n}: "));
        let module = module.unwrap_or_default();
        for seg in &self.segments {
            let base = bases[seg.kind.index()];
            for ch in &seg.chunks {
                let addr = base + i64::from(ch.offset);
                if let Err(e) = img.put(addr, &ch.bytes) {
                    errors.push(AsmError::plain(format!(
                        "{module}сегмент {}: {e}",
                        seg.kind.name()
                    )));
                }
            }
        }
        for r in &self.relocs {
            let target = match &r.target {
                RelocTarget::Segment(k) => Some(bases[k.index()]),
                RelocTarget::Extern(n) => externs(n),
            };
            let Some(target) = target else {
                let name = match &r.target {
                    RelocTarget::Extern(n) => n.as_str(),
                    RelocTarget::Segment(k) => k.name(),
                };
                let msg = format!(
                    "{module}внешний символ {name} не определён (при сборке в bin/hex/ihex EXTRN \
                     недопустим: соберите объектные файлы -t link и скомпонуйте link-k580vm80a)"
                );
                errors.push(match &r.loc {
                    Some(l) => AsmError::at(l, msg),
                    None => AsmError::plain(msg),
                });
                continue;
            };
            let v = r.part.apply(target + r.addend);
            let at = bases[r.seg.index()] + i64::from(r.offset);
            let bytes = match r.part {
                Part::Word => vec![(v & 0xFF) as u8, (v >> 8) as u8],
                _ => vec![v as u8],
            };
            img.patch(at, &bytes);
        }
        errors
    }
}

/// Значение при заданных базах сегментов и адресах внешних символов.
pub fn resolve_value(
    v: &Value,
    bases: &SegBases,
    externs: &dyn Fn(&str) -> Option<i64>,
) -> Option<i64> {
    match &v.rel {
        None => Some(v.num),
        Some(r) => {
            let base = match &r.target {
                RelocTarget::Segment(k) => bases[k.index()],
                RelocTarget::Extern(n) => externs(n)?,
            };
            Some(v.resolve(base))
        }
    }
}
