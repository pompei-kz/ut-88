# Эмулятор ЮТ-88. Справка: make help

# BASH_EXE — чем запускать скрипты scripts/*.sh. В Linux и в Git Bash это bash из PATH.
# В Windows вне Git Bash (PowerShell, cmd) bash из PATH — это WSL
# (C:\Windows\System32\bash.exe), а make запускает скрипт сам и спотыкается о строку
# «#!/usr/bin/env bash». Поэтому там берётся bash из Git для Windows, он же становится
# оболочкой рецептов. Корень Git вычисляется из `git --exec-path`
# (…/Git/mingw64/libexec/git-core), так что место установки Git не важно; bin/bash.exe
# из корня Git сам добавляет в PATH утилиты grep, awk и т. п.
BASH_EXE := bash
ifeq ($(OS),Windows_NT)
ifndef MSYSTEM
GIT_EXEC_PATH := $(subst \,/,$(shell git --exec-path))
ifeq ($(GIT_EXEC_PATH),)
$(error Не найден Git для Windows: установите его (https://git-scm.com/download/win) или запускайте make из Git Bash)
endif
GIT_ROOT := $(GIT_EXEC_PATH)
$(foreach d,mingw64 ucrt64 clang64 clangarm64 mingw32,$(eval GIT_ROOT := $(subst /$(d)/libexec/git-core,,$(GIT_ROOT))))
BASH_EXE := $(GIT_ROOT)/bin/bash.exe
SHELL := $(BASH_EXE)
endif
endif

BIN := target/release/ut88
ASM := target/release/asm-k580vm80a
LINK := target/release/link-k580vm80a

.PHONY: compile help base base-t full full-t tetris tetris-t cpm cpm-t os os-t \
        prog prog-t programs asm asm-run asm-load link asm-examples test test-all lint fmt align-md clean

## Собрать бинарники (по умолчанию)
compile:
	cargo build --release

## Список команд
help:
	@"$(BASH_EXE)" scripts/make-help.sh

## Базовый вариант: модуль 1 (индикаторы и кнопки)
base: compile
	$(BIN) --config minimal

## Базовый вариант в терминале
base-t: compile
	$(BIN) --config minimal --tui

## Полный вариант: дисплейный модуль, МОНИТОР-F, ОЗУ 64 Кбайт
full: compile
	$(BIN) --config full

## Полный вариант в терминале
full-t: compile
	$(BIN) --config full --tui

## Игра «Тетрис»
tetris: compile
	"$(BASH_EXE)" scripts/run-program.sh tetris

## Игра «Тетрис» в терминале
tetris-t: compile
	"$(BASH_EXE)" scripts/run-program.sh tetris -t

## CP/M-64 с квазидиском (образ диска — ut88-disk.img)
cpm: compile
	$(BIN) --boot cpm64 --kvazidisk ut88-disk.img

## CP/M-64 в терминале
cpm-t: compile
	$(BIN) --boot cpm64 --kvazidisk ut88-disk.img --tui

## ОС ЮТ-88
os: compile
	$(BIN) --boot os

## ОС ЮТ-88 в терминале
os-t: compile
	$(BIN) --boot os --tui

## Программа из programs/: make prog P=gamma
prog: compile
	"$(BASH_EXE)" scripts/run-program.sh $(P)

## Программа из programs/ в терминале: make prog-t P=gamma
prog-t: compile
	"$(BASH_EXE)" scripts/run-program.sh $(P) -t

## Список программ из programs/
programs:
	@"$(BASH_EXE)" scripts/run-program.sh --list

## Собрать программу на ассемблере: make asm F=файл.asm [O=файл.hex]
asm: compile
	$(ASM) -f $(F) -o $(or $(O),$(F:.asm=.hex))

## Собрать и запустить в эмуляторе: make asm-run F=файл.asm [C=minimal|full]
asm-run: compile
	"$(BASH_EXE)" scripts/asm-run.sh $(F) $(C)

## Собрать и загрузить, не запуская (запуск — G адрес): make asm-load F=файл.asm [C=full|minimal]
asm-load: compile
	NORUN=1 "$(BASH_EXE)" scripts/asm-run.sh $(F) $(or $(C),full)

## Скомпоновать модули: make link O=файл.hex M="a.link b.link" [B=адрес]
link: compile
	$(LINK) $(M) -o $(O) -m $(basename $(O)).map $(if $(B),--base $(B))

## Собрать все примеры из examples/asm/
asm-examples: compile
	"$(BASH_EXE)" scripts/asm-examples.sh

## Тесты
test:
	cargo test

## Все тесты, включая долгие (8080EXM, «Тетрис»)
test-all:
	cargo test
	cargo test --release -- --ignored

## Проверки clippy и форматирования
lint:
	cargo clippy --all-targets -- -D warnings
	cargo fmt --check

## Отформатировать код
fmt:
	cargo fmt

## Выровнять таблицы во всех .md
align-md:
	git ls-files '*.md' | grep -v '^CLAUDE.md$$' | xargs scripts/align-md.py
	scripts/align-md.py --code CLAUDE.md

## Удалить результаты сборки
clean:
	cargo clean
