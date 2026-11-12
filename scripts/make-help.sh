#!/usr/bin/env bash
# Список команд Makefile: строка «## описание» перед целью «имя:».
set -euo pipefail
cd "$(dirname "$0")/.."

awk '
    /^## / { desc = substr($0, 4); next }
    /^[a-z-]+:/ && desc != "" { split($0, a, ":"); printf "  make %-10s — %s\n", a[1], desc; desc = "" }
' Makefile
