//! Общий для GUI и TUI ряд скоростей эмуляции.

/// Ряд скоростей — множители реального времени; `0.0` — максимум (насколько хватает
/// процессора ПК). Порядок — от медленной к быстрой.
pub const SPEEDS: [f64; 8] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 0.0];

/// Подпись скорости: «×0,25», «×1», «×16», «максимум».
pub fn speed_label(k: f64) -> String {
    if k <= 0.0 {
        "максимум".into()
    } else {
        format!("×{k}").replace('.', ",")
    }
}

/// Индекс скорости в [`SPEEDS`] (если её там нет — ближайшей).
pub fn speed_index(k: f64) -> usize {
    let k = snap_speed(k);
    SPEEDS.iter().position(|&s| s == k).unwrap_or(2)
}

/// Ближайшая к множителю скорость ряда (`0` и меньше — максимум; больше ×16 — ×16).
pub fn snap_speed(k: f64) -> f64 {
    if !k.is_finite() || k <= 0.0 {
        return 0.0;
    }
    SPEEDS[..SPEEDS.len() - 1]
        .iter()
        .copied()
        .min_by(|a, b| (a.ln() - k.ln()).abs().total_cmp(&(b.ln() - k.ln()).abs()))
        .unwrap_or(1.0)
}

/// Следующая скорость ряда (`up` — быстрее); на краях остаётся та же.
pub fn speed_step(k: f64, up: bool) -> f64 {
    let i = speed_index(k);
    let j = if up {
        (i + 1).min(SPEEDS.len() - 1)
    } else {
        i.saturating_sub(1)
    };
    SPEEDS[j]
}

/// Разобрать скорость: `0.25`, `0,5`, `x2`, `×16`, `max` / `0` — максимум. Принимаются
/// только значения ряда [`SPEEDS`].
pub fn parse_speed(s: &str) -> Result<f64, String> {
    let t = s.trim();
    if t.eq_ignore_ascii_case("max") || t == "максимум" {
        return Ok(0.0);
    }
    let v: Option<f64> = t
        .trim_start_matches(['x', 'X', '×'])
        .replace(',', ".")
        .parse()
        .ok();
    match v {
        Some(v) if v == 0.0 || SPEEDS.contains(&v) => Ok(v),
        _ => Err(format!(
            "неверная скорость «{s}»: нужно 0.25, 0.5, 1, 2, 4, 8, 16 или max"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds() {
        assert_eq!(parse_speed("max"), Ok(0.0));
        assert_eq!(parse_speed("0,25"), Ok(0.25));
        assert_eq!(parse_speed("x16"), Ok(16.0));
        assert!(parse_speed("3").is_err());
        assert!(parse_speed("32").is_err());
        assert_eq!(snap_speed(3.9), 4.0);
        assert_eq!(snap_speed(100.0), 16.0);
        assert_eq!(snap_speed(0.0), 0.0);
        assert_eq!(speed_step(1.0, true), 2.0);
        assert_eq!(speed_step(16.0, true), 0.0);
        assert_eq!(speed_step(0.0, true), 0.0);
        assert_eq!(speed_step(0.0, false), 16.0);
        assert_eq!(speed_step(0.25, false), 0.25);
        assert_eq!(speed_label(0.5), "×0,5");
        assert_eq!(speed_label(0.0), "максимум");
    }
}
