//! Игра «Тетрис» (programs/tetris.bin, 1989-09): проверка управления и правил через API.
//!
//! Клавиши подаются так, как их нажимает человек: клавиша держится, пока МОНИТОР-«F» не
//! примет символ (подпрограмма F803H щёлкает динамиком FE4BH — по первому перепаду звука
//! клавиша отпускается). Так одно нажатие — ровно один ход, без автоповтора.

mod common;

use common::*;
use ut88_core::{CPU_HZ, Config, KbKey, Machine};

/// Ширина и высота стакана.
const W: usize = 10;
const H: usize = 20;
/// Столбец экрана левой клетки стакана (клетка — два знакоместа).
const COL0: usize = 34;

type Grid = [[bool; W]; H];

/// Стакан с экрана: строки экрана 1–20, клетка занята, если в ней «[».
fn grid(m: &Machine) -> Grid {
    let s = m.screen().unwrap();
    let mut g = [[false; W]; H];
    for (r, row) in g.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = s.code(r + 1, COL0 + 2 * c) == b'[';
        }
    }
    g
}

fn cells(g: &Grid) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    for (r, row) in g.iter().enumerate() {
        for (c, &x) in row.iter().enumerate() {
            if x {
                v.push((r, c));
            }
        }
    }
    v
}

/// Форма фигуры без учёта положения.
fn shape(v: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let r0 = v.iter().map(|p| p.0).min().unwrap();
    let c0 = v.iter().map(|p| p.1).min().unwrap();
    let mut s: Vec<_> = v.iter().map(|&(r, c)| (r - r0, c - c0)).collect();
    s.sort();
    s
}

/// Число полных строк со счётчика «ЧИСЛО ПОЛНЫХ СТРОК».
fn lines(m: &Machine) -> u32 {
    let t = screen_text(m);
    let l = t
        .lines()
        .find(|l| l.starts_with("ЧИСЛО ПОЛНЫХ СТРОК"))
        .unwrap_or_else(|| panic!("нет счётчика:\n{t}"));
    l["ЧИСЛО ПОЛНЫХ СТРОК".len()..]
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

/// Уровень, показанный после «ВАШ УРОВЕНЬ(0-7) ?».
fn level(m: &Machine) -> u32 {
    let t = screen_text(m);
    let l = t.lines().find(|l| l.starts_with("ВАШ УРОВЕНЬ")).unwrap();
    l.split('?')
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

/// Нажать клавишу с кодом `code` и держать, пока программа её не примет (щелчок
/// F803H), но не дольше 3 с. Возвращает `true`, если символ принят.
fn press(m: &mut Machine, code: u8) -> bool {
    let (key, modifier) = KbKey::for_code(code).unwrap();
    m.set_sound_capture(true);
    m.drain_sound_edges();
    if let Some(md) = modifier {
        m.kb_down(md);
    }
    m.kb_down(key);
    let limit = m.cycles() + 3 * CPU_HZ as u64;
    let mut ok = false;
    while m.cycles() < limit {
        m.run_seconds(0.005);
        if !m.drain_sound_edges().is_empty() {
            ok = true;
            break;
        }
    }
    m.kb_up(key);
    if let Some(md) = modifier {
        m.kb_up(md);
    }
    // Дождаться конца щелчка и дать программе перерисовать фигуру.
    let mut quiet = 0;
    while quiet < 4 {
        m.run_seconds(0.01);
        if m.drain_sound_edges().is_empty() {
            quiet += 1;
        } else {
            quiet = 0;
        }
    }
    settle(m);
    ok
}

/// Дождаться, пока программа дорисует экран (вывод через F809H медленный: перерисовка
/// фигуры занимает около 0,1 с): экранное ОЗУ не меняется 0,06 с.
fn settle(m: &mut Machine) {
    let mut prev = m.save_bin(0xE800, 0xEFFF);
    let mut same = 0;
    while same < 3 {
        m.run_seconds(0.02);
        let now = m.save_bin(0xE800, 0xEFFF);
        if now == prev {
            same += 1;
        } else {
            same = 0;
            prev = now;
        }
    }
}

/// Загрузить «Тетрис», ввести уровень и запустить игру.
fn start(lv: u8) -> Machine {
    let mut m = Machine::new(Config::full());
    m.run_seconds(0.5);
    m.load_bin_file(repo("programs/tetris.bin"), 0x3000)
        .unwrap();
    kb_type(&mut m, "G3000\r");
    m.run_seconds(1.5);
    let t = screen_text(&m);
    assert!(t.contains("ВАШ УРОВЕНЬ(0-7) ?"), "{t}");
    assert!(press(&mut m, b'0' + lv));
    m.run_seconds(0.3);
    let t = screen_text(&m);
    assert!(t.contains("НАЖМИТЕ ЛЮБУЮ КЛАВИШУ"), "{t}");
    assert!(press(&mut m, b' '));
    m.run_seconds(0.3);
    let t = screen_text(&m);
    assert!(!t.contains("НАЖМИТЕ ЛЮБУЮ КЛАВИШУ"), "{t}");
    assert_eq!(level(&m), u32::from(lv), "{t}");
    assert_eq!(lines(&m), 0);
    m
}

/// Падающая фигура (пока стакан пуст, это все занятые клетки).
fn piece(m: &Machine) -> Vec<(usize, usize)> {
    let v = cells(&grid(m));
    assert_eq!(v.len(), 4, "{}", screen_text(m));
    v
}

#[test]
fn tetris_moves_rotation_and_drop() {
    let mut m = start(0);
    let p0 = piece(&m);
    let min_c = |v: &[(usize, usize)]| v.iter().map(|p| p.1).min().unwrap();
    let max_r = |v: &[(usize, usize)]| v.iter().map(|p| p.0).max().unwrap();

    // 7 — влево на клетку, 9 — вправо на клетку; форма не меняется.
    assert!(press(&mut m, b'7'));
    let p1 = piece(&m);
    assert_eq!(min_c(&p1), min_c(&p0) - 1, "{}", screen_text(&m));
    assert_eq!(shape(&p1), shape(&p0));
    assert!(press(&mut m, b'9'));
    assert!(press(&mut m, b'9'));
    let p2 = piece(&m);
    assert_eq!(min_c(&p2), min_c(&p0) + 1, "{}", screen_text(&m));

    // Фигура падает сама.
    m.run_seconds(2.0);
    let p3 = piece(&m);
    assert!(max_r(&p3) > max_r(&p2), "{}", screen_text(&m));

    // 8 — поворот: форма меняется, четыре поворота возвращают исходную. Берём
    // следующую фигуру — не квадрат — и ждём, пока она целиком войдёт в стакан
    // (появляется она частично выше верхней строки).
    let top = |m: &Machine| -> Vec<(usize, usize)> {
        cells(&grid(m)).into_iter().filter(|p| p.0 < 10).collect()
    };
    let square = vec![(0, 0), (0, 1), (1, 0), (1, 1)];
    let mut before = Vec::new();
    for _ in 0..5 {
        assert!(press(&mut m, b' '));
        while top(&m).len() < 4 || top(&m).iter().any(|p| p.0 == 0) {
            m.run_seconds(0.1);
        }
        before = top(&m);
        if shape(&before) != square {
            break;
        }
    }
    assert_ne!(shape(&before), square);
    let mut shapes = vec![shape(&before)];
    for _ in 0..4 {
        assert!(press(&mut m, b'8'));
        let now: Vec<_> = cells(&grid(&m)).into_iter().filter(|p| p.0 < 10).collect();
        assert_eq!(now.len(), 4, "{}", screen_text(&m));
        shapes.push(shape(&now));
    }
    assert_ne!(shapes[1], shapes[0], "поворот не изменил фигуру");
    assert_eq!(shapes[4], shapes[0], "четыре поворота — полный круг");

    // Пробел — сброс: фигура сразу на дне, появляется следующая.
    assert!(press(&mut m, b' '));
    m.run_seconds(0.3);
    let g = grid(&m);
    assert!(g[H - 1].iter().any(|&x| x), "{}", screen_text(&m));
    assert_eq!(lines(&m), 0);

    // 6 — увеличить скорость: уровень на экране растёт.
    let lv = level(&m);
    assert!(press(&mut m, b'6'));
    m.run_seconds(0.2);
    assert_eq!(level(&m), lv + 1, "{}", screen_text(&m));
}

#[test]
fn tetris_pause() {
    let mut m = start(0);
    assert!(press(&mut m, b'S'));
    m.run_seconds(0.3);
    let t = screen_text(&m);
    assert!(t.contains("ВОДИТЕЛЬ, УСТАЛ - ОТДОХНИ !"), "{t}");
    // Пока пауза, фигура стоит.
    let g = grid(&m);
    m.run_seconds(3.0);
    assert_eq!(grid(&m), g, "{}", screen_text(&m));
    // Любая клавиша — продолжение: надпись стирается, фигура снова падает.
    assert!(press(&mut m, b'A'));
    m.run_seconds(3.0);
    let t = screen_text(&m);
    assert!(!t.contains("ВОДИТЕЛЬ"), "{t}");
    assert_ne!(grid(&m), g, "{t}");
}

/// УС+C — конец игры; «N» — выход в МОНИТОР-«F».
#[test]
fn tetris_ctrl_c_and_exit() {
    let mut m = start(0);
    assert!(press(&mut m, 0x03));
    m.run_seconds(0.3);
    let t = screen_text(&m);
    assert!(t.contains("ИГРА ЗАКОНЧЕНА"), "{t}");
    assert!(t.contains("(Y/N)"), "{t}");
    assert!(press(&mut m, b'N'));
    m.run_seconds(0.5);
    let s = m.screen().unwrap();
    assert_eq!(s.line(1), "*ЮТ/88*", "{}", s.text());
}

/// Сбрасывать фигуры в середину, пока стакан не заполнится: «ИГРА ЗАКОНЧЕНА …»;
/// «Y» — новая игра с вопросом об уровне.
#[test]
fn tetris_game_over_and_restart() {
    let mut m = start(7);
    let mut over = false;
    for _ in 0..40 {
        assert!(press(&mut m, b' '));
        m.run_seconds(0.3);
        if screen_text(&m).contains("ИГРА ЗАКОНЧЕНА") {
            over = true;
            break;
        }
    }
    let t = screen_text(&m);
    assert!(over, "{t}");
    assert!(t.contains("ЖЕЛАЕТЕ ПОВТОРИТЬ"), "{t}");
    // Стакан заполнен доверху.
    assert!(grid(&m)[1].iter().any(|&x| x), "{t}");
    assert!(press(&mut m, b'Y'));
    m.run_seconds(1.5);
    let t = screen_text(&m);
    assert!(t.contains("ВАШ УРОВЕНЬ(0-7) ?"), "{t}");
}

// ----- заполнение строки -----

/// Оценка стакана после хода (больше — лучше): полные строки, дыры, высота.
fn score(m: &Machine, base_lines: u32) -> i64 {
    // Строки 0–2 не считаем: там уже появилась следующая фигура.
    let g = grid(m);
    let mut holes = 0;
    let mut height = 0;
    for col in (0..W).map(|c| g.iter().map(|row| row[c]).collect::<Vec<_>>()) {
        if let Some(t) = (3..H).find(|&r| col[r]) {
            height += (H - t) as i64;
            holes += col[t..].iter().filter(|&&x| !x).count() as i64;
        }
    }
    let bottom = g[H - 1].iter().filter(|&&x| x).count() as i64;
    i64::from(lines(m) - base_lines) * 10_000 - holes * 100 - height * 3 + bottom * 5
}

/// Положить текущую фигуру: `rot` поворотов, сдвиг `dx` (отрицательный — влево), сброс.
fn place(m: &mut Machine, rot: u8, dx: i32) {
    for _ in 0..rot {
        press(m, b'8');
    }
    let k = if dx < 0 { b'7' } else { b'9' };
    for _ in 0..dx.unsigned_abs() {
        press(m, k);
    }
    press(m, b' ');
    m.run_seconds(0.4);
}

/// Игра «жадным» перебором: для каждой фигуры все повороты и сдвиги пробуются на
/// снимке машины, выбирается лучший. Через несколько фигур нижняя строка заполняется
/// и исчезает, счётчик полных строк растёт.
#[test]
#[ignore = "долгий: перебор ходов (≈ 1 мин в debug); cargo test -- --ignored"]
fn tetris_full_line_disappears() {
    let mut m = start(0);
    let mut moves = Vec::new();
    for n in 1..=30 {
        let base = lines(&m);
        let snap = m.snapshot();
        let mut best = (i64::MIN, 0, 0);
        for rot in 0..4 {
            for dx in -5..=5 {
                m.restore(&snap).unwrap();
                place(&mut m, rot, dx);
                let s = score(&m, base);
                if s > best.0 {
                    best = (s, rot, dx);
                }
            }
        }
        m.restore(&snap).unwrap();
        let before = grid(&m);
        place(&mut m, best.1, best.2);
        moves.push((best.1, best.2));
        if lines(&m) > base {
            // Строка исчезла: стакан сдвинулся вниз, в нём стало меньше клеток, чем было
            // до хода плюс четыре клетки фигуры.
            let n_before = cells(&before).len();
            let n_after = cells(&grid(&m)).iter().filter(|p| p.0 >= 3).count();
            assert!(n_after + W <= n_before + 4, "{}", screen_text(&m));
            assert!(
                !grid(&m)[H - 1].iter().all(|&x| x),
                "полная строка осталась:\n{}",
                screen_text(&m)
            );
            println!("фигур: {n}, ходы: {moves:?}\n{}", screen_text(&m));
            return;
        }
        assert!(
            !screen_text(&m).contains("ИГРА ЗАКОНЧЕНА"),
            "{}",
            screen_text(&m)
        );
    }
    panic!("за 30 фигур ни одной полной строки:\n{}", screen_text(&m));
}

/// На вопрос «(Y/N)» другая клавиша не повторяет вопрос: после проверки Y и N программа
/// проходит по `NOP` (3227H–322EH) в подпрограмму паузы 322FH.
#[test]
fn tetris_other_key_at_question_is_pause() {
    let mut m = start(0);
    assert!(press(&mut m, 0x03));
    m.run_seconds(0.3);
    assert!(press(&mut m, b'A'));
    m.run_seconds(0.3);
    let t = screen_text(&m);
    assert!(t.contains("ВОДИТЕЛЬ, УСТАЛ - ОТДОХНИ !"), "{t}");
}
