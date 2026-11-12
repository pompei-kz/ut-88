# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-12

First public release.

### Added

- KR580VM80A / Intel 8080 CPU emulator, cycle-counted, passing TST8080, 8080PRE,
  CPUTEST and 8080EXM.
- UT-88 machine in four configurations: processor module only (MONITOR ROM, six LEDs,
  hex keypad), full (display module 64×28, full keyboard, MONITOR-F, 64 KB RAM), full
  with the module 1 MONITOR enabled, and CP/M (quasi-disk 64–256 KB on port 40H).
- Window interface (egui) drawing the computer as in the magazine photos, with menus,
  hot-key button bar, mouse input, sound and a debugger window.
- Terminal interface (ratatui) with the same hot keys, menus and debugger.
- Tape recorder: WAV recording and playback in both MONITOR formats, fast load and save
  of memory ranges.
- Snapshots of the whole machine state (quick snapshot F6/F7 and files).
- Debugger: step, step over, run to address, address and port breakpoints, registers,
  disassembly, memory view and editing.
- Programs from the magazine in `programs/` (fill memory, Reaction, Gamma, Tic-tac-toe,
  Labyrinth, display test, Tetris, ROM programmer) with a program list (F9) and
  `make prog P=NAME`.
- CP/M-64, CP/M-35 and UT-88 OS from the 1990 No. 2 issue, quasi-disk images, putting PC
  files onto the CP/M disk (`--cpm-put`).
- Magazine articles converted to Markdown in `doc/` (Russian), user manual in Russian
  (`manual.md`) and English (`docs/MANUAL.md`), quick start in `README.md`.
- Makefile with build, run and test commands; GitHub Actions CI.

[Unreleased]: https://github.com/pompei-kz/ut-88/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/pompei-kz/ut-88/releases/tag/v0.1.0
