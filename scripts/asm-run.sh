#!/usr/bin/env bash
# Собрать программу на ассемблере и запустить её в эмуляторе.
#   scripts/asm-run.sh ФАЙЛ.asm [minimal|full] [КЛЮЧИ ut88...]
# Дамп ФАЙЛ.hex пишется рядом с исходником. Адрес пуска — из «END адрес» (запись 03
# Intel HEX), без него — адрес первого байта программы. Конфигурация — из второго
# аргумента, иначе угадывается по тексту: обращения к МОНИТОРУ-F (F800H–FFFFH) или к
# экрану (E000H–EFFFH) — full, иначе minimal.
# Переменные окружения: UT88 (по умолчанию target/release/ut88), ASM
# (target/release/asm-k580vm80a); UT88=echo — только показать команду запуска;
# NORUN=1 — только загрузить программу, не запуская (запуск — директивой МОНИТОРА:
# G адрес у МОНИТОРА-F, «7 адрес» у МОНИТОРА модуля 1).
set -euo pipefail
cd "$(dirname "$0")/.."

UT88=${UT88:-target/release/ut88}
ASM=${ASM:-target/release/asm-k580vm80a}

src=${1:-}
if [[ -z $src || ! -f $src ]]; then
    echo "Использование: scripts/asm-run.sh ФАЙЛ.asm [minimal|full] [КЛЮЧИ ut88...]" >&2
    [[ -n $src ]] && echo "Нет файла «$src»." >&2
    exit 1
fi
shift
config=${1:-}
[[ $# -gt 0 ]] && shift

hex=${src%.asm}.hex
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

"$ASM" -f "$src" -o "$hex"
"$ASM" -f "$src" -o "$tmp/prog.ihx"

# Адрес пуска: запись 03 (:04000003 CCCC IIII KK) — последние 4 цифры данных; иначе
# адрес первой строки дампа.
start=$(grep -E '^:04000003' "$tmp/prog.ihx" | cut -c14-17 || true)
[[ -z $start ]] && start=$(head -n1 "$hex" | cut -d' ' -f1)

if [[ -z $config ]]; then
    config=minimal
    # Числа вида 0F809H, F86CH, 0E800H, $F809, 0xF809 вне комментариев.
    if sed 's/;.*//' "$src" | grep -qiE '(\b0?[EF][0-9A-F]{3}H\b|\$[EF][0-9A-F]{3}\b|\b0X[EF][0-9A-F]{3}\b)'; then
        config=full
    fi
fi
case $config in
    minimal | full) ;;
    *) echo "Неизвестная конфигурация «$config» (minimal или full)." >&2; exit 1 ;;
esac

if [[ ${NORUN:-} == 1 ]]; then
    if [[ $config == full ]]; then how="G$start и Enter"; else how="7 $start"; fi
    echo "asm-load: $src → $hex, конфигурация $config, загружено с $start, не запущено; запуск — $how" >&2
    "$UT88" --config "$config" --load "$hex" "$@"
else
    echo "asm-run: $src → $hex, конфигурация $config, пуск с $start" >&2
    "$UT88" --config "$config" --load "$hex" --run "$start" "$@"
fi
