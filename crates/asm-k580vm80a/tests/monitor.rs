//! monitor/monitor.asm собирается байт-в-байт и совпадает с листингом monitor/monitor.lst.

use std::path::PathBuf;

use asm_k580vm80a::{Options, assemble_file, ir::Placement};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn monitor_byte_for_byte() {
    let a = assemble_file(&root().join("monitor/monitor.asm"), &Options::default())
        .unwrap_or_else(|e| panic!("{}", e[0]));
    let img = a.to_image(&Placement::default()).unwrap();
    let bin = img.to_bin(0);
    // monitor.asm — исправленный текст (INX SP вместо DCX SP по адресам 006FH–0070H),
    // ему соответствует monitor-fixed.bin.
    let fixed = std::fs::read(root().join("monitor/monitor-fixed.bin")).unwrap();
    assert_eq!(bin, fixed);
    let orig = std::fs::read(root().join("monitor/monitor.bin")).unwrap();
    let diff: Vec<usize> = (0..orig.len()).filter(|&i| orig[i] != bin[i]).collect();
    assert_eq!(diff, vec![0x6F, 0x70]);
    // Дамп в формате журнала: monitor.hex — дамп оригинального ПЗУ, отличие в тех же байтах.
    let hex = std::fs::read_to_string(root().join("monitor/monitor.hex")).unwrap();
    let ours = img.to_dump();
    assert_eq!(ours.lines().count(), hex.lines().count());
    let changed: Vec<&str> = ours
        .lines()
        .zip(hex.lines())
        .filter(|(a, b)| a != b)
        .map(|(a, _)| &a[..4])
        .collect();
    assert_eq!(changed, vec!["0060", "0070"]);
}

/// Адреса и коды каждой строки совпадают с monitor.lst (формат: адрес, коды по 4 в
/// строке с переносом, исходная строка с 21-го столбца).
#[test]
fn monitor_matches_listing() {
    let a = assemble_file(&root().join("monitor/monitor.asm"), &Options::default()).unwrap();
    let lst = std::fs::read_to_string(root().join("monitor/monitor.lst")).unwrap();
    // (номер исходной строки, адрес, байты)
    let mut expected: Vec<(usize, u32, Vec<u8>)> = Vec::new();
    let mut src_no = 0;
    for l in lst.lines() {
        let chars: Vec<char> = l.chars().collect();
        let head: String = chars.iter().take(20).collect();
        let has_src = chars.len() > 20;
        if has_src || head.trim().is_empty() {
            src_no += 1;
        }
        let mut it = head.split_whitespace();
        let Some(first) = it.next() else { continue };
        if first.starts_with('=') || first.len() != 4 {
            continue;
        }
        let addr = u32::from_str_radix(first, 16).unwrap();
        let bytes: Vec<u8> = it.map(|b| u8::from_str_radix(b, 16).unwrap()).collect();
        if has_src || head.trim().is_empty() {
            expected.push((src_no, addr, bytes));
        } else {
            expected.last_mut().unwrap().2.extend(bytes);
        }
    }
    let ours: Vec<(usize, u32, Vec<u8>)> = a
        .lines
        .iter()
        .filter(|r| !r.bytes.is_empty())
        .map(|r| (r.src.line, r.offset, r.bytes.clone()))
        .collect();
    assert_eq!(ours.len(), expected.len());
    for (o, e) in ours.iter().zip(&expected) {
        assert_eq!(o, e);
    }
}
