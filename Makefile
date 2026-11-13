# Эмулятор ЮТ-88. Справка: make help

BIN := target/release/ut88

.PHONY: compile help base base-t full full-t tetris tetris-t cpm cpm-t os os-t \
        prog prog-t programs test test-all lint fmt align-md clean

## Собрать бинарники (по умолчанию)
compile:
	cargo build --release

## Список команд
help:
	@scripts/make-help.sh

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
	scripts/run-program.sh tetris

## Игра «Тетрис» в терминале
tetris-t: compile
	scripts/run-program.sh tetris -t

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
	scripts/run-program.sh $(P)

## Программа из programs/ в терминале: make prog-t P=gamma
prog-t: compile
	scripts/run-program.sh $(P) -t

## Список программ из programs/
programs:
	@scripts/run-program.sh --list

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
