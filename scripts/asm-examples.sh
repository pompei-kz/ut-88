#!/usr/bin/env bash
# Собрать все примеры examples/asm/: каждый *.asm — в дамп .hex рядом с исходником;
# пример из модулей examples/asm/hello-link/ — в объектные файлы .link и компоновкой с
# адреса 3000H в hello-link.hex (с картой hello-link.map).
set -euo pipefail
cd "$(dirname "$0")/.."

ASM=${ASM:-target/release/asm-k580vm80a}
LINK=${LINK:-target/release/link-k580vm80a}

for src in examples/asm/*.asm; do
    echo "  $src → ${src%.asm}.hex"
    "$ASM" -f "$src" -o "${src%.asm}.hex"
done

dir=examples/asm/hello-link
for m in main print; do
    echo "  $dir/$m.asm → $dir/$m.link"
    "$ASM" -f "$dir/$m.asm" -o "$dir/$m.link"
done
echo "  $dir/main.link $dir/print.link → $dir/hello-link.hex"
"$LINK" "$dir/main.link" "$dir/print.link" -o "$dir/hello-link.hex" -m "$dir/hello-link.map" --base 3000
