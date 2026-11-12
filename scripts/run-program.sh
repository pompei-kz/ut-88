#!/usr/bin/env bash
# Запуск программы из programs/ по имени файла.
#   scripts/run-program.sh ИМЯ [-t]   — запустить (с -t — в терминале)
#   scripts/run-program.sh --list     — список программ
# Адрес пуска и нужная конфигурация берутся из таблицы programs/README.md.
set -euo pipefail
cd "$(dirname "$0")/.."

UT88=${UT88:-target/release/ut88}
TABLE=programs/README.md

list() {
    echo "Программы (make prog P=ИМЯ):"
    grep -E '^\| `[a-z-]+`' "$TABLE" | awk -F'|' '{gsub(/[` ]/, "", $2); sub(/^ +/, "", $3); sub(/ +$/, "", $3); print "  " $2 "\t— " $3}'
}

if [[ ${1:-} == "" || ${1:-} == "--list" ]]; then
    list
    exit 0
fi

name=$1
tui=${2:-}

row=$(grep -F "| \`$name\` " "$TABLE" || true)
if [[ -z $row || ! -f programs/$name.bin ]]; then
    echo "Нет программы «$name»." >&2
    list >&2
    exit 1
fi

# Столбцы таблицы: файл | название | выпуск | конфигурация | загрузка | конец | пуск | …
start=$(echo "$row" | awk -F'|' '{gsub(/[ H]/, "", $8); print $8}')
config=minimal
[[ $row == *дисплейный* ]] && config=full

args=(--config "$config" --load "programs/$name.bin")
# Данные к программе (например, поле «Лабиринта») — строки «данные к `ИМЯ`».
while read -r data; do
    args+=(--load "programs/$data.bin")
done < <(grep -F "данные к \`$name\`" "$TABLE" | awk -F'|' '{gsub(/[` ]/, "", $2); print $2}')

[[ $start =~ ^[0-9A-F]+$ ]] && args+=(--run "$start")
[[ $tui == -t ]] && args+=(--tui)

exec "$UT88" "${args[@]}"
