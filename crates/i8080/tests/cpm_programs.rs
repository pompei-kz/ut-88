//! Прогон известных тестовых программ 8080 (CP/M .COM) — см. `tests/roms/README.md`.

mod common;

use common::run_com;

const TST8080: &[u8] = include_bytes!("roms/TST8080.COM");
const PRE8080: &[u8] = include_bytes!("roms/8080PRE.COM");
const CPUTEST: &[u8] = include_bytes!("roms/CPUTEST.COM");
const EXM8080: &[u8] = include_bytes!("roms/8080EXM.COM");

#[test]
fn tst8080() {
    let r = run_com(TST8080, 10_000_000);
    println!("{}\n[тактов: {}]", r.output, r.cycles);
    assert!(r.output.contains("CPU IS OPERATIONAL"), "{}", r.output);
}

#[test]
fn pre8080() {
    let r = run_com(PRE8080, 10_000_000);
    println!("{}\n[тактов: {}]", r.output, r.cycles);
    assert!(
        r.output.contains("8080 Preliminary tests complete"),
        "{}",
        r.output
    );
    assert!(!r.output.to_lowercase().contains("error"), "{}", r.output);
}

#[test]
fn cputest() {
    let r = run_com(CPUTEST, 1_000_000_000);
    println!("{}\n[тактов: {}]", r.output, r.cycles);
    assert!(r.output.contains("CPU TESTS OK"), "{}", r.output);
}

/// Полный прогон 8080EXM: CRC каждой группы команд сравнивается с эталонами настоящего
/// 8080. Идёт ≈15 с в release и минуты в debug, поэтому по умолчанию пропускается:
/// `cargo test --release -p i8080 -- --ignored`.
#[test]
#[ignore = "долгий тест (≈15 с в release, минуты в debug): cargo test --release -- --ignored"]
fn exm8080() {
    let r = run_com(EXM8080, u64::MAX);
    println!("{}\n[тактов: {}]", r.output, r.cycles);
    assert!(r.output.contains("Tests complete"), "{}", r.output);
    assert!(!r.output.contains("ERROR"), "{}", r.output);
    // В 8080EXM 25 групп команд; у каждой должно быть «PASS! crc is:…».
    assert_eq!(r.output.matches("PASS!").count(), 25, "{}", r.output);
}
