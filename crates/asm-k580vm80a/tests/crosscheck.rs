//! Перекрёстная проверка с дизассемблером `crates/i8080`: для каждого документированного
//! кода текст дизассемблера собирается обратно в те же байты.

use asm_k580vm80a::{Options, assemble, ir::Placement};

fn asm_bytes(text: &str) -> Vec<u8> {
    let src = format!("        ORG 1000H\n        {text}\n");
    let a = assemble("cross.asm", &src, &Options::default())
        .unwrap_or_else(|e| panic!("«{text}»: {}", e[0]));
    let img = a.to_image(&Placement::default()).unwrap();
    assert_eq!(img.chunks.len(), 1, "{text}");
    assert_eq!(img.chunks[0].0, 0x1000, "{text}");
    img.chunks[0].1.clone()
}

#[test]
fn all_documented_opcodes_round_trip() {
    let mut documented = 0;
    // Разные операнды: с ведущей буквой (0C3H, 0FEDCH) и с цифрой (12H, 3456H).
    for operands in [[0x34u8, 0x12], [0xC3, 0xFE], [0x00, 0x00], [0xFF, 0xFF]] {
        documented = 0;
        for op in 0..=255u8 {
            let bytes = [op, operands[0], operands[1]];
            let (text, len) = i8080::disasm(&bytes, 0x1000);
            if text.starts_with('*') {
                continue;
            }
            documented += 1;
            let mn = text.split_whitespace().next().unwrap();
            let form = asm_k580vm80a::opcodes::lookup(mn).unwrap();
            assert_eq!(usize::from(form.size()), len, "длина {mn}");
            assert_eq!(asm_bytes(&text), &bytes[..len], "код {op:02X}: «{text}»");
            // Мнемоника строчными буквами — тот же результат.
            assert_eq!(
                asm_bytes(&text.to_lowercase()),
                &bytes[..len],
                "код {op:02X}: «{}»",
                text.to_lowercase()
            );
        }
    }
    assert_eq!(documented, 244);
}
