Русская версия: [../manual.md](../manual.md)

# UT-88 emulator: user manual

This manual is for those who want to **use** the UT-88 («ЮТ-88») computer in the emulator:
start it, type in and run programs, play the games from the magazine, work with the tape
recorder and the debugger. How the computer itself works is described in detail in the
translation of the magazine articles — [doc/README.md](../doc/README.md) (in Russian).
A project overview is in [README.md](../README.md).

**About the language of the machine and the emulator.** The UT-88 is a Soviet computer: its
keyboard has Russian key labels, and its programs print messages in Russian. The emulator's
own interface (menus, status line, messages, `--help` text) is in Russian as well. In this
manual, key names, menu items and on-screen messages are given in the original Russian,
with an English translation in parentheses at least on first mention, so that you can
recognize them on the screen. All documents in [doc/](../doc/) (the magazine translation)
are in Russian.

## Contents

1. [What it is](#1-what-it-is)
2. [Building and running](#2-building-and-running)
3. [Graphical interface](#3-graphical-interface)
4. [Terminal interface](#4-terminal-interface)
5. [Working with the module 1 MONITOR](#5-working-with-the-module-1-monitor)
6. [Working with MONITOR-F](#6-working-with-monitor-f)
7. [Tape recorder](#7-tape-recorder)
8. [Snapshots](#8-snapshots)
9. [Debugger](#9-debugger)
10. [Programs from the magazine](#10-programs-from-the-magazine)
11. [RAM disk and CP/M](#11-ram-disk-and-cpm)
12. [Common problems](#12-common-problems)

---

## 1. What it is

The UT-88 is a home-built personal computer based on the KR580VM80A processor (a Soviet
clone of the Intel 8080). It was published piece by piece in 1989–1990 by the magazine
«ЮТ для умелых рук» ("UT for Skillful Hands", a supplement to «Юный техник», "Young
Technician"). The computer is modular, and the emulator supports four of its
configurations:

| Configuration    | Option                    | What it includes                                                                                         | What you see after power-on                                 |
|------------------|---------------------------|----------------------------------------------------------------------------------------------------------|-------------------------------------------------------------|
| minimal          | `--config minimal`        | processor module (module 1): 1 KB MONITOR ROM, RAM C000H–C3FFH, 6 LED digits, 16 buttons                 | «11» on the LED display — the MONITOR waits for a command   |
| full             | `--config full` (default) | module 1 + display module (64×28 screen, full keyboard) + 64 KB RAM                                      | `*ЮТ/88*` on the screen and the MONITOR-F prompt `=>`       |
| full + MONITOR 1 | `--config full-monitor0`  | the same, but with the module 1 MONITOR ROM enabled                                                      | «11» on the LED display; MONITOR-F is started with `7 F800` |
| CP/M             | `--config cpm`            | full + an electronic RAM disk («квазидиск») of 64–256 KB (port 40H) — for CP/M and UT-88 OS (section 11) | as in full: `*ЮТ/88*` and `=>`                              |

More about the modes of the full configuration:

- **full** (`full`) — the "clock in MONITOR-F" mode from the magazine (doc/25): the module 1
  MONITOR ROM is disabled, RAM occupies 0000H–DFFFH (56 KB), and at power-on the emulator
  puts `DI; JMP F800H` and `JMP FFC1H` at addresses 0000H and 0038H, so the display
  module's MONITOR-F starts immediately. This is the main mode for display module programs
  («Тетрис» — Tetris, the ROM programmer);
- **full + module 1 MONITOR** (`full-monitor0`) — the first mode from doc/25: after power-on
  the module 1 MONITOR runs (the magazine calls it MONITOR-"0"), RAM is 1000H–DFFFH;
  MONITOR-F is started with the command `7 F800`. It is needed, for example, for the display
  module test program and for the MONITOR-F command `B`;
- **CP/M** (`cpm`) — full plus the RAM disk (doc/32): the whole address space except the
  screen at E000H–EFFFH and the display module segment F000H–FFFFH is RAM, and the
  processor's stack accesses go to the RAM disk banks while a bank is open. CP/M-64,
  CP/M-35 and UT-88 OS run in this configuration (section 11).

The processor runs at 16 MHz / 9 ≈ 1.78 MHz (as with the KR580GF24 clock generator chip and
a 16 MHz crystal). Once per second the K176IE5 clock chip raises an `RST 7` interrupt
request.

Memory map and ports: [doc/02-arhitektura.md](../doc/02-arhitektura.md),
[doc/15-modul-ozu.md](../doc/15-modul-ozu.md),
[doc/29-displey-dlya-emulyatora.md](../doc/29-displey-dlya-emulyatora.md) (in Russian).

---

## 2. Building and running

### Requirements

- Rust with `cargo` (Rust 1.89 or newer: edition 2024, let-chains and dependencies), for example via
  [rustup](https://rustup.rs/);
- on Linux, for the window and sound — the usual X11/Wayland and ALSA libraries (for
  building — the ALSA development package, `libasound2-dev` on Debian/Ubuntu); file dialogs
  use xdg-desktop-portal.

### Building

From the repository root:

```sh
cargo build --release -p ut88
```

The resulting executable is `target/release/ut88`. You can run it directly or via
`cargo run`:

```sh
cargo run --release -p ut88 -- --config minimal
target/release/ut88 --config minimal          # the same
```

Everything after `--` is passed to the emulator as options. Below, `ut88` in the examples
means `target/release/ut88`. It is most convenient to run it from the repository root:
then the emulator finds the `programs/` folder with the programs from the magazine by
itself (it looks for it in the current directory, next to the executable and further up
the directory tree).

### Command-line options

`ut88 --help` prints the full list (in Russian).

| Option                  | What it does                                                                                                                              |
|-------------------------|-------------------------------------------------------------------------------------------------------------------------------------------|
| `--config CONFIG`       | `minimal`, `full` (default), `full-monitor0` or `cpm` (default with `--boot`) — see section 1                                             |
| `--kvazidisk-size N`    | RAM disk size for `--config cpm`: `64`, `128`, `192` or `256` (default) KB                                                                |
| `--kvazidisk FILE`      | RAM disk image: loaded at startup (if the file exists) and saved on exit (section 11)                                                     |
| `--boot SYSTEM`         | load and start `cpm64` (CP/M-64), `cpm35` (CP/M-35) or `os` (UT-88 OS) — section 11                                                       |
| `--cpm-format`          | with `--boot cpm64`/`cpm35`: start with a clean CP/M disk (otherwise the files on the disk are kept)                                      |
| `--cpm-put FILE[=NAME]` | write a host (PC) file to the CP/M disk under its own name or as `NAME.EXT` (after `--boot`); may be repeated                             |
| `--rom FILE`            | your own module 1 MONITOR ROM image (at most 1 KB) instead of the built-in `monitor-fixed.bin`                                            |
| `--calculator`          | attach the calculator library ROM at 0800H–0FFFH ([doc/11-kalkulyator.md](../doc/11-kalkulyator.md))                                      |
| `--tui`                 | terminal interface instead of a window                                                                                                    |
| `--load FILE[@ADDR]`    | load a program into memory; may be repeated                                                                                               |
| `--run ADDR`            | after loading, jump to the address (start the program)                                                                                    |
| `--tape FILE.wav`       | insert a "cassette" — a WAV file to be read with the `A` or `I` command; the tape stands still until a program starts reading (section 7) |
| `--snapshot FILE`       | restore the state from a snapshot; the configuration is taken from the snapshot                                                           |
| `--speed N`             | speed: `0.25`, `0.5`, `1`, `2`, `4`, `8`, `16` or `max` (`0` also means maximum); the same series for the window and terminal             |
| `--no-sound`            | no sound                                                                                                                                  |
| `--programs DIR`        | where to look for the programs from the magazine (default — `programs/`)                                                                  |
| `-h`, `--help`          | help                                                                                                                                      |
| `-V`, `--version`       | version                                                                                                                                   |

Addresses are hexadecimal: `C000`, `C000H`, `0xC000` or `$C000`.

**Formats for `--load`:**

- `.bin` — a binary image. The load address is given after `@` (`--load prog.bin@C000`);
  if it is omitted, it is taken from the table in `programs/README.md` by file name (this
  works for all programs from the magazine), otherwise it is an error;
- `.hex`, `.ihx` with `:LLAAAATT…` records — Intel HEX;
- `.hex` of the form `AAAA XX XX XX …` — a text dump, like `programs/*.hex` and
  `monitor/monitor.hex` (the line address and bytes; lines with `#` and `;` are comments).

If an `@ADDR` is given for a HEX file, the image is shifted so that its start lands at that
address. The programs from the magazine are not relocatable: a shifted program will most
likely not work at its new location.

**How loading works.** With `--load` and `--run`, the emulator first runs the machine for
1 s — the MONITOR has time to set up the stack and its work cells — then puts the programs
into memory and, if `--run` is given, jumps to the address. Without `--run` the program
just sits in memory, and you can start it as in the magazine — with a MONITOR command
(`6`, `7 ADDR`, `G ADDR`). The emulator prints to the terminal what was loaded and what
happens next (the messages are in Russian):

```
ut88: programs/gamma.hex — 131 байт с C000, адрес пуска по programs/README.md C03E
ut88: программа загружена, но не запущена (нет --run): запустите директивой МОНИТОРА или ключом --run АДРЕС
```

That is: "programs/gamma.hex — 131 bytes at C000, start address according to
programs/README.md C03E" and "the program is loaded but not started (no --run): start it
with a MONITOR command or with the --run ADDR option".

The start address is printed only if it is known: from the `programs/README.md` table — when
the program is loaded at its address from the table, or from a type 03/05 record in Intel
HEX. If the image is shifted (`--load programs/gamma.bin@C100`), the line says «сдвинуто …
адрес пуска неизвестен» ("shifted … start address unknown"). With `--run`, instead of the
second line, `ut88: пуск с C03E (--run)` ("starting at C03E") is printed.

Examples:

```sh
ut88                                               # full configuration, MONITOR-F
ut88 --config minimal                              # module 1 only
ut88 --tui --config minimal                        # the same in the terminal
ut88 --load programs/tetris.bin --run 3000         # straight into Tetris
ut88 --config minimal --load my.bin@C000           # your own program, start with command 6
ut88 --config minimal --load programs/gamma.hex --run C03E --speed 2
ut88 --snapshot ut88.snap                          # continue from a saved point
ut88 --boot cpm64                                  # CP/M-64 with a 256 KB RAM disk
ut88 --boot cpm64 --kvazidisk disk.img --cpm-put roms/cpm-ch-com.bin=CH.COM  # section 11
ut88 --boot os                                     # UT-88 OS
```

---

## 3. Graphical interface

### Appearance

The window draws the computer "as in the magazine's photographs":

- **minimal configuration** — the case of the processor module: six red seven-segment LED
  digits (labels H, L — the HL pair, the four left digits; A — the two right digits) and a
  4×4 keypad with the «ШАГ НАЗАД» (STEP BACK) and «УСТ» (RESET) buttons;
- **full configuration** — additionally a TV set with the display module screen (64×28
  characters) and the full keyboard, drawn after the figure in the magazine (1989, issue
  No. 5). The processor module with its LED digits and buttons is drawn next to it — it can
  be used in the full configuration as well. The magazine's figure has no ↖ (home), СТР
  (clear screen) and arrow keys — they are added as a separate block on the right.

You can click the drawn keys with the mouse (see "Mouse" below).

### Menus

Menu items are shown here in Russian, as in the program, with a translation.

**Машина** (Machine)

| Item                                                       | What it does                                                                                                                                                                   |
|------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| УСТ — сброс (F2) — RESET                                   | the «УСТ» button: processor reset; memory and the clock are preserved                                                                                                          |
| Холодный старт (F3) — Cold start                           | switch the power off and on: a new machine of the same configuration, memory is cleared                                                                                        |
| Пауза / Продолжить (F4) — Pause / Continue                 | stop or resume emulation                                                                                                                                                       |
| Скорость: ×0,25 … ×16, максимум — Speed: ×0.25 … ×16, max  | speed relative to a real UT-88 (F5 — faster, Shift+F5 — slower); at "maximum" the sound is turned off                                                                          |
| Конфигурация (холодный старт) — Configuration (cold start) | minimal / full (MONITOR-F, clock in it) / full + module 1 MONITOR / CP/M and UT-88 OS (full + 256K RAM disk); switching is a cold start, memory is lost                        |
| Прерывания от часов — Clock interrupts                     | the "toggle switch" from doc/25: disables the interrupt request from the seconds generator (needed when working with the tape recorder through the module 1 MONITOR)           |
| ПЗУ МОНИТОРА-F — MONITOR-F ROM                             | (only with the display module) the "toggle switch" from doc/30: a disabled ROM gives F000H–FFFFH to the RAM module — this is how UT-88 OS works; a cold start enables it again |
| Клавиатура ПК → кнопки модуля 1 — PC keyboard → module 1   | (only in the full configuration) the PC keyboard controls the processor module buttons instead of the full keyboard                                                            |

**Магнитофон** (Tape recorder)

| Item                                                                                      | What it does                                                                                             |
|-------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------|
| Вставить кассету (WAV)… — Insert cassette (WAV)…                                          | choose a WAV file; the tape stands still and will start moving when a program begins reading (section 7) |
| Воспроизведение / Стоп — Play / Stop                                                      | start or stop the tape manually (the position is kept)                                                   |
| Перемотать в начало — Rewind                                                              | the tape goes to the beginning and stands still; it starts again when read                               |
| Вынуть кассету — Eject cassette                                                           | remove the cassette                                                                                      |
| Начать запись / Остановить запись и сохранить WAV… — Start recording / Stop and save WAV… | record the tape output (port A1H); on stop, save it to WAV                                               |
| Магнитофон и файлы… (F8) — Tape recorder and files…                                       | the «Магнитофон и файлы» window: tape buttons, fast loading, WAV (section 7)                             |

**Файл** (File)

| Item                                                                    | What it does                                                                                                               |
|-------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------|
| Загрузить программу (.bin/.hex/.ihx)… — Load program…                   | load a file into memory (address rules as for `--load`; a `.bin` without an address opens the «Магнитофон и файлы» window) |
| Загрузить и запустить… — Load and run…                                  | the same plus a jump to the start address (from the program table, from Intel HEX or the first address of the file)        |
| Быстрый снимок (F6) — Quick snapshot                                    | save the state to `ut88-quick.snap` in the current directory                                                               |
| Загрузить быстрый снимок (F7) — Load quick snapshot                     | load `ut88-quick.snap`                                                                                                     |
| Сохранить снимок… / Загрузить снимок… — Save snapshot… / Load snapshot… | the same with a file chooser (`.snap`)                                                                                     |

**CP/M** — loading the systems from the listings of the 1990-02 issue and files on the CP/M
disk (section 11):

| Item                                                                                                | What it does                                                                                                                   |
|-----------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------|
| Загрузить CP/M-64 (файлы на квазидиске сохраняются) — Load CP/M-64 (files on the RAM disk are kept) | CP/M-64 from the RAM disk; if the RAM disk already contains CP/M with a directory (for example, from an image), the files stay |
| Загрузить CP/M-64 с чистым квазидиском — Load CP/M-64 with a clean RAM disk                         | as in the magazine (`G3100`): the system and an empty directory are written to the RAM disk                                    |
| Загрузить CP/M-35 (диск в ОЗУ 5000H–DBFFH) — Load CP/M-35 (disk in RAM 5000H–DBFFH)                 | CP/M-35, which does not need the RAM disk                                                                                      |
| Загрузить ОС ЮТ-88 — Load UT-88 OS                                                                  | the UT-88 OS starter (`G3000`); the emulator disables the MONITOR-F ROM and clock interrupts itself — just press any key       |
| Диск CP/M: файлы… — CP/M disk: files…                                                               | a window with the file list: «Сохранить на ПК…» (Save to PC…), «Удалить» (Delete), «Записать файл с ПК…» (Write file from PC…) |
| Записать файл на диск CP/M… — Write file to CP/M disk…                                              | choose a PC file — it is put on the CP/M disk under its own name                                                               |
| Сохранить образ квазидиска… / Загрузить образ… — Save RAM disk image… / Load image…                 | the whole RAM disk to a file (`.img`) and back                                                                                 |

If the configuration does not fit (for example, minimal), the loading items first do a
cold start into the "CP/M" configuration.

**Программы** (Programs) — the list of programs from `programs/` (section 10); the same list
with buttons opens in a separate window with F9. For each program it shows the magazine
issue, the required configuration, the load and start addresses and the buttons
«Загрузить» (Load) and «Загрузить и запустить» (Load and run). Programs for the display
module are not available in the minimal configuration (hint «нужна полная конфигурация» —
"full configuration required"). A program's data (the «Лабиринт» (Labyrinth) maze field)
is loaded together with it. «Загрузить» only puts the program into memory — the status line
will say «… — не запущено» ("… — not started").

**Вид** (View) — «Звук» (Sound, on/off), «громкость» (volume), the screen phosphor colour
(green or white), «Отладчик (F12)» (Debugger).

**Справка** (Help) — «Клавиши (F11)» (Keys: the key mapping tables, the same as below) and
«О программе» (About). The help window can also be opened without the keyboard: with this
menu item or with the «F11 Справка» button on the hotkey bar.

### Hotkey bar

Above the status line there is a row of buttons, as in Norton / Midnight Commander:
«F2 УСТ» (reset), «F3 Холод» (cold start), «F4 Пауза» (pause), «F5 Скор+» (speed+),
«Shift+F5 Скор-» (speed−), «F6 Снимок» (snapshot), «F7 Загр» (load), «F8 Лента» (tape),
«F9 Прогр» (programs), «F10 Выход» (exit), «F11 Справка» (help), «F12 Отладчик»
(debugger). Clicking a button is the same as pressing the key; the tooltip explains the
action. This way all hotkeys are available even if the window manager intercepts an
F-key.

### Status line

At the bottom of the window, from left to right: the state (`▶ работа` — running, or
`⏸ пауза` — paused), the configuration, the speed (set and actual, in percent), machine
time in seconds since power-on, the tape recorder state (`кассеты нет` — no cassette;
`⏹ лента 0.0 с — тронется при чтении` — a cassette is inserted and waiting, "will start on
read"; `▶ лента 12.3 с` — the tape is moving; `⏹ … — стоп` — stopped manually;
`лента кончилась` — end of tape; `● запись` — recording), the sound icon (🔊 / 🔇) and the
last message (kept for 8 s). Errors are also shown in a separate «Ошибка» (Error) window.

### PC keyboard → module 1 buttons

Active in the minimal configuration, and in the full one if «Машина → Клавиатура ПК →
кнопки модуля 1» (Machine → PC keyboard → module 1 buttons) is enabled.

| PC        | UT-88                 |
|-----------|-----------------------|
| 0–9       | 0–9                   |
| A–F       | A–F                   |
| Backspace | ШАГ НАЗАД (STEP BACK) |
| F2        | УСТ (RESET)           |

### PC keyboard → full keyboard of the display module

Keys are taken **by physical position** (as on the US layout), so the PC layout switch does
not matter: the key labelled "A/Ф" is always "A".

The UT-88 keyboard has three modifier keys: **СС** («специальный символ», "special
symbol" — acts like Shift), **УС** («управляющий символ», "control symbol" — like Ctrl) and
**РУС** ("Russian" — switches to Cyrillic letters). **ВК** («возврат каретки», carriage
return) is the Enter key; **ПРОБЕЛ** is Space; **СТР** («стереть», erase) clears the screen;
**↖** moves the cursor home.

| PC                  | UT-88               |
|---------------------|---------------------|
| Shift               | СС (SHIFT)          |
| Ctrl                | УС (CTRL)           |
| Alt                 | РУС (RUS)           |
| 0–9, A–Z            | 0–9, A–Z            |
| `;` `,` `.` `/` `-` | `;` `,` `.` `/` `-` |
| `'`                 | `:`                 |
| `=`                 | `_`                 |
| `` ` ``             | `@`                 |
| `[` `\` `]`         | `[` `\` `]`         |
| Insert              | `^`                 |
| Space               | ПРОБЕЛ (Space)      |
| Enter               | ВК (CR)             |
| Backspace, ←        | ←                   |
| →, ↑, ↓             | →, ↑, ↓             |
| Home                | ↖                   |
| Delete, End         | СТР (clear screen)  |

The modifiers work as on the UT-88:

- **СС** (Shift) with digits and punctuation gives the upper symbol of the key: СС+1 → `!`,
  СС+2 → `"`, … СС+`;` → `+`, СС+`:` (Shift+`'`) → `*`, СС+`,` → `<`, СС+`-` → `=`,
  СС+`.` → `>`, СС+`/` → `?`. СС has no effect on letters (the UT-88 has only upper-case
  Latin letters);
- **РУС** (Alt) with a Latin letter gives a Russian one according to KOI-7: A→А, B→Б, C→Ц,
  D→Д, E→Е, F→Ф, G→Г, H→Х, I→И, J→Й, K→К, L→Л, M→М, N→Н, O→О, P→П, Q→Я, R→Р, S→С, T→Т,
  U→У, V→Ж, W→В, X→Ь, Y→Ы, Z→З, `[`→Ш, `\`→Э, `]`→Щ, `^`→Ч, `@`→Ю;
- **УС** (Ctrl) with a letter gives a control code (УС+C = 03H, which, for example, ends
  Tetris). Ctrl+C, Ctrl+X, Ctrl+V are handled specially and also reach the machine as
  УС+C/X/V.

The complete UT-88 keyboard layout — [doc/22-klaviatura-i-direktivy.md](../doc/22-klaviatura-i-direktivy.md)
(in Russian).

**Holding keys.** A UT-88 key is pressed as long as the PC key is pressed, but for at least
0.06 s. Programs poll the keyboard differently (Tetris — rarely), so in games it is better
to hold a key rather than just tap it. If the window loses focus, all keys are released.
While the cursor is in an input field (for example, in the debugger), PC keys do not go to
the machine.

### Hotkeys

The scheme is **the same for the window and the terminal** (section 4); in the window it is
shown by «Справка → Клавиши (F11)» (Help → Keys). Every key is also available as a button
on the bar at the bottom (in the window — the "Hotkey bar", in the terminal — the bottom
line); clicking a button equals pressing the key. F1 is not used.

| Key      | Action                                              | Differences                                                                          |
|----------|-----------------------------------------------------|--------------------------------------------------------------------------------------|
| F2       | the «УСТ» button (processor reset)                  |                                                                                      |
| F3       | cold start (switch the power off and on)            |                                                                                      |
| F4       | pause / continue                                    |                                                                                      |
| F5       | faster                                              |                                                                                      |
| Shift+F5 | slower                                              |                                                                                      |
| F6       | quick snapshot to `./ut88-quick.snap`, no questions |                                                                                      |
| F7       | load the quick snapshot                             |                                                                                      |
| F8       | tape recorder                                       | window: «Магнитофон и файлы»; terminal: tape recorder menu                           |
| F9       | programs from the magazine                          | window: «Программы из журналов» (Programs from the magazine); terminal: program list |
| F10      | exit                                                | the window closes                                                                    |
| F11      | help / menu                                         | window: «Справка: клавиши» (Help: keys); terminal: menu of all actions with help     |
| F12      | debugger                                            | window: debugger window; terminal: debugger screen                                   |

Speeds for F5 / Shift+F5 (and in the «Машина → Скорость» menu): ×0.25, ×0.5, ×1, ×2, ×4,
×8, ×16, maximum — `--speed` accepts the same series. F10 closes the window immediately,
without asking: save a snapshot (F6) before exiting if you need the state.

### Mouse

- Clicking a drawn key presses it for at least 0.3 s (longer while you hold the mouse
  button).
- РУС, УС and СС "stick" when clicked: click the modifier, then click a key — the modifier
  is released together with it. Clicking the modifier again cancels it.
- «УСТ» and «ШАГ НАЗАД» on the processor module can be clicked too — in the full
  configuration as well.
- All hotkeys are available as buttons on the bar above the status line; the help is also
  available via the «Справка → Клавиши» menu.

---

## 4. Terminal interface

Start: `ut88 --tui` (with any other options). The terminal must be at least 80×32
characters (the 64×28 screen in a frame, the status line and the button bar), larger is
better. In a narrower terminal the bar buttons that do not fit on the right are not drawn.
There is no sound in the terminal interface.

- In the minimal configuration the screen shows six LED digits as large numerals (labels
  «АДРЕС (HL)» — ADDRESS and «ДАННЫЕ (A)» — DATA) and a module 1 keypad hint; the pressed
  button is highlighted.
- In the full configuration — the 64×28 display module screen in a frame; at the bottom of
  the frame you can see which key is currently "pressed" (for example, `нажато УС+03H` —
  "pressed CTRL+03H").
- The second-to-last line is the status: configuration, speed (and the actual one),
  `ПАУЗА` (PAUSE), the cycle counter, the tape, the last message or the hint
  «F11 — меню и справка» ("F11 — menu and help").
- The bottom line is the hotkey button bar in the Midnight Commander style (F-key number
  and label on a cyan background):
  `2УСТ 3Холод 4Пауза 5Скор+ ⇧5Скор− 6Снимок 7Загр 8Лента 9Прогр 10Выход 11Справка 12Отладка`
  (reset, cold start, pause, speed+, speed−, snapshot, load, tape, programs, exit, help,
  debug; on the screen there are no spaces, the buttons are separated by colour). A left
  mouse click on a button is the same as pressing the key (`⇧5` is Shift+F5). If a window
  is open (menu, tape recorder, programs, input line), a click closes it and performs the
  button's action; clicking «11Справка» while the menu or help is open only closes them.

### Hotkeys

The same as in the window — see the table in section 3 ("Hotkeys"): F2 — «УСТ» (reset),
F3 — cold start, F4 — pause, F5 / Shift+F5 — speed, F6 / F7 — quick snapshot, F8 — tape
recorder, F9 — programs, F10 — exit, F11 — menu and help, F12 — debugger. F1 is not used.
Speeds: ×0.25, ×0.5, ×1, ×2, ×4, ×8, ×16, maximum.

Many terminals (GNOME Terminal, Konsole, xfce4-terminal) take F11 for themselves — it
toggles full-screen mode, and the key never reaches the emulator. In that case open the
menu and help by clicking the «11Справка» button on the bottom bar (or disable F11 in the
terminal's keyboard settings). The emulator enables terminal mouse capture and disables it
on exit (including abnormal termination); while it is running, select text in the terminal
with the mouse while holding Shift.

### F11 menu

Opened with F11 or by clicking «11Справка». Items are selected by letter or with the arrows
and Enter; Esc or F11 closes the menu. Below the items there is a short help on input, and
the `?` item opens the full help: all hotkeys, input keys and how to use the button bar
(closed by any key).

| Letter | Item                                 | Same as |
|--------|--------------------------------------|---------|
| u      | УСТ — processor reset                | F2      |
| h      | cold start                           | F3      |
| p      | pause / continue                     | F4      |
| +      | faster                               | F5      |
| -      | slower                               | Sh+F5   |
| s      | quick snapshot                       | F6      |
| l      | load quick snapshot                  | F7      |
| a      | save snapshot to a file…             | —       |
| o      | load snapshot from a file…           | —       |
| t      | tape recorder                        | F8      |
| g      | programs from the magazine           | F9      |
| d      | debugger                             | F12     |
| k      | view: module 1 ↔ display module      | —       |
| i      | clock interrupts on/off              | —       |
| r      | MONITOR-F ROM on/off (toggle switch) | —       |
| c      | CP/M, UT-88 OS, RAM disk…            | —       |
| ?      | help: all keys                       | —       |
| q      | exit                                 | F10     |

Item **c** opens the CP/M menu (section 11):

| Letter | Item                                                 |
|--------|------------------------------------------------------|
| 6      | load CP/M-64 (files on the RAM disk are kept)        |
| f      | load CP/M-64 with a clean RAM disk                   |
| 3      | load CP/M-35 (disk in RAM 5000H–DBFFH)               |
| o      | load UT-88 OS                                        |
| l      | list of files on the CP/M disk (in the status line)  |
| w      | write a PC file to the CP/M disk (`FILE[=NAME.EXT]`) |
| x      | save a file from the CP/M disk to the PC             |
| d      | delete a file from the CP/M disk                     |
| s      | save the RAM disk image                              |
| i      | load the RAM disk image                              |

The menu items are shown in Russian on the screen; the letters are the same.

Item **k** in the full configuration switches the screen between the display and the
processor module: in the "module 1" view the PC keyboard controls the module 1 buttons
(for example, to work with the module 1 MONITOR in the `full-monitor0` configuration).
Terminals intercept some F-keys, so everything is duplicated in the menu.

### Input

- **Module 1:** 0–9 and A–F (any case) — digits, Backspace — «ШАГ НАЗАД» (STEP BACK),
  F2 — «УСТ» (RESET).
- **Full keyboard:** typed characters are converted to KOI-7 codes: Latin letters — as
  upper case, punctuation — as is (the emulator presses the needed СС itself), Russian
  letters of any case — codes 60H–7FH (with РУС); Alt+Latin letter — РУС, Ctrl+letter — УС;
  Enter — ВК, arrows and Backspace — ←→↑↓, Home — ↖, Delete and End — СТР, Tab — 09H,
  Esc — 1BH.

**Holding keys.** A terminal usually does not report key releases, so the emulator holds
each press itself: a module 1 button — until the program reads the keyboard port (plus
20 ms more, but no longer than 1 s), a full keyboard key — 0.4 s (when typing fast —
0.15 s with a 0.1 s pause between keys). Key presses are queued, so you can type fast.
Terminal auto-repeat extends the hold. In terminals with the kitty keyboard protocol
(kitty, WezTerm, foot and others) a key is held until it is really released.

### Tape recorder (F8)

| Letter | Item                                                                                                     |
|--------|----------------------------------------------------------------------------------------------------------|
| w      | insert a cassette (WAV) — asks for the file path; the tape stands still and starts on read (section 7)   |
| p      | play / stop — start or stop the tape manually                                                            |
| n      | rewind (the tape stands still and starts again on read)                                                  |
| s      | eject the cassette                                                                                       |
| r      | record the output to WAV: the first press starts recording, the second stops it and asks for a file name |
| b      | fast-load a `.bin` at an address — asks for the file and the address                                     |
| m      | save a memory range to `.bin` — asks for `start,end` and the file                                        |
| o      | write a memory range to WAV as a MONITOR block — asks for `start,end` and the file                       |

The block format for **o** is chosen by the current view: if the display is shown — the
MONITOR-F format (for the `I` command), if module 1 — the module 1 MONITOR format (for the
`A` command). In the input line Backspace deletes a character, Ctrl+U — the whole line,
Esc — cancel.

### Programs (F9)

The list of programs from `programs/` with load and start addresses. **Enter** — load and
run (a direct jump to the start address), **L** — load only (the message will say «не
запущена» — "not started"). The Labyrinth maze field is loaded together with
«Лабиринт» (Labyrinth). The configuration is checked as in the window: programs that need
the display module or 64 KB RAM (Tetris, «Программатор» — the ROM programmer) are greyed
out in the minimal configuration, the reason is shown below the list («нужен дисплейный
модуль — запустите эмулятор с --config full» — "display module required — start the
emulator with --config full"), and Enter/L do not load them. In the terminal the
configuration can only be changed by restarting with a different `--config`.

### Limitations

- no sound;
- registers cannot be changed in the debugger;
- no file dialogs — paths are typed manually (relative to the current directory);
- snapshots with a file name — via the F11 menu (`a` / `o`), the name is typed manually
  (default `ut88.snap`); F6 / F7 — quick snapshot without questions.

---

## 5. Working with the module 1 MONITOR

Full description — [doc/06-monitor.md](../doc/06-monitor.md) (in Russian). Here is a cheat
sheet.

After power-on (or «УСТ»/RESET) the right LED digits show «11»: the MONITOR waits for a
command (the magazine calls them «директивы», "directives"). A command is one button,
followed by its parameters — hexadecimal numbers (an address is four digits, a byte is
two). The digits being typed are shown on the A display, an entered address moves to the
HL display.

|  Button   | Parameters       | What it does                                                       |
|:---------:|------------------|--------------------------------------------------------------------|
|     0     | ADDR             | write to RAM starting at ADDR                                      |
|     1     | —                | write to RAM starting at C000H                                     |
|     2     | —                | view RAM starting at C000H (any button — next cell)                |
|     3     | —                | LED display test                                                   |
|     4     | —                | RAM test: «C400» on HL — RAM is OK                                 |
|     5     | ADDR             | view RAM starting at ADDR                                          |
|     6     | —                | run the program at C000H                                           |
|     7     | ADDR             | run the program at ADDR                                            |
|     8     | ADDR1 ADDR2      | checksum of the area (result on HL)                                |
|     9     | ADDR1 ADDR2      | write the area to tape                                             |
|     A     | OFFSET           | read from tape (0000 — to the same addresses)                      |
|     B     | —                | show the time: HL — hours and minutes, A — seconds                 |
|     C     | C3FD s s m m h h | set the time (seconds, minutes, hours); the clock runs after «УСТ» |
| ШАГ НАЗАД | —                | while writing/viewing — go back one cell and enter it again        |

When writing (commands 0 and 1) you type the two digits of a byte; after about a second the
MONITOR moves to the next cell by itself. To leave a command, press «УСТ».

**Utility programs** (called with the command `7 ADDR`, then the parameters):

| Address | Program                          | Parameters                                                         |
|:-------:|----------------------------------|--------------------------------------------------------------------|
|  0200   | copy                             | start, end of the source; start of the copy                        |
|  03B2   | compare two areas                | start, end of the source; start of the copy                        |
|  025F   | corrector (copy with correction) | start, end of the source; start of the area being corrected        |
|  02E5   | supercorrector                   | start, end of the working program; start of the one being prepared |
|  035E   | insert a byte                    | insertion address; address of the program end                      |
|  0388   | delete a byte                    | address of the byte to delete; address of the program end          |
|  0309   | replace addresses                | start, end of the working program; old address; new address        |
|  03DD   | register display                 | called from a program: `CD DD 03` (`CALL 03DDH`)                   |

For example, `7 0200 0100 0120 C100` copies 0100H–0120H to C100H.

**Subroutines for your own programs:** `RST 2` (code D7) — input a byte (two digits) into
A, `RST 3` (DF) — delay ≈ 1 s, `RST 4` (E7) — wait for a key press, button code in A,
`RST 5` (EF) — show HL and A on the LED display, `RST 6` (F7) — input an address (four
digits) into DE, `RST 0` (C7) — exit to the MONITOR, `CALL 0259H` — compare HL and DE. The
full table is in doc/06.

### Example: entering and running a program

The program "Fill memory with a constant" («Заполнение памяти константой») from the
magazine ([doc/07-programmy.md](../doc/07-programmy.md)):

```
C000  F7 EB F7 D7 47 70 CD 59 02 23 C2 05 C0 C7
```

1. Start the minimal configuration: `ut88 --config minimal`. The LED display shows «11».
2. Press **1**: HL shows «C000», A shows «00».
3. Type the bytes two digits at a time: `F 7`, wait until HL changes to «C001», then
   `E B`, `F 7`, `D 7`, `4 7`, `7 0`, `C D`, `5 9`, `0 2`, `2 3`, `C 2`, `0 5`, `C 0`,
   `C 7`. Made a mistake — press «ШАГ НАЗАД» (Backspace) and type the byte again.
4. Press «УСТ» (F2) — «11» again.
5. Check: **2** — view from C000H (each press — the next cell), or the checksum: **8**,
   `C000`, `C00D` — HL must show «0800».
6. Run: **6**, then the start address `C100`, the end address `C17F` and the constant `A5`.
   The program fills the area and returns to the MONITOR — A shows «11» again.
7. Look at the result: **5**, `C100` — the cells contain A5.

To avoid typing it in by hand: `ut88 --config minimal --load programs/fill-const.bin` — and
go straight to step 5.

---

## 6. Working with MONITOR-F

Description of the commands — [doc/22-klaviatura-i-direktivy.md](../doc/22-klaviatura-i-direktivy.md)
and [doc/25-monitor-f-chasy-i-magnitofon.md](../doc/25-monitor-f-chasy-i-magnitofon.md),
analysis of the program — [doc/27-monitor-f.md](../doc/27-monitor-f.md) (in Russian).

In the full configuration MONITOR-F starts by itself; in `full-monitor0` — with the module 1
MONITOR command `7 F800`. The screen shows `*ЮТ/88*` and the prompt `=>`. A command is a
Latin letter, the parameters are hexadecimal numbers separated by commas (`H` and leading
zeros are not needed), terminated with **ВК** (Enter). An error in a command gives «?».

| Command               | What it does                                                                  |
|-----------------------|-------------------------------------------------------------------------------|
| `D ADDR1,ADDR2`       | hexadecimal memory dump                                                       |
| `L ADDR1,ADDR2`       | dump as characters                                                            |
| `K ADDR1,ADDR2`       | checksum of the area                                                          |
| `M ADDR`              | view and modify cells: new value + ВК or just ВК; exit — `.` (period)         |
| `F ADDR1,ADDR2,BYTE`  | fill the area with a byte                                                     |
| `T ADDR1,ADDR2,ADDR3` | move the area ADDR1–ADDR2 to ADDR3                                            |
| `C ADDR1,ADDR2,ADDR3` | compare the area ADDR1–ADDR2 with the area at ADDR3                           |
| `S ADDR1,ADDR2,BYTE`  | search for a byte in the area                                                 |
| `G ADDR[,BRK]`        | run from the address (with an optional breakpoint)                            |
| `W`                   | run from C000H                                                                |
| `X`                   | view and modify registers (like `M`)                                          |
| `O ADDR1,ADDR2[,SC]`  | write the area to tape (SC — speed constant)                                  |
| `I [OFFS][,SC]`       | read from tape                                                                |
| `V`                   | measure the tape reading constant                                             |
| `B`                   | show the module 1 MONITOR time on the LED display (only in `full-monitor0`)   |
| `R ADDR1,ADDR2,ADDR3` | read an external ROM via the PPI at ports F8H–FBH (FFH there in the emulator) |

Subroutines for your own programs: F803H — input a character into A, F809H — output the
character from C, F812H — poll the keyboard (A = 00 / FF), F815H — output byte A in hex,
F818H — output a string (HL, terminated with 00H), F86CH — warm start (return to the
prompt). The full vector table is in doc/27. Screen control codes: 08H/18H/19H/1AH — cursor
←→↑↓, 0CH — cursor home, 1FH — clear screen, 0DH 0AH — new line.

### Example: a program that prints a letter

```
C000  0E 41       MVI  C,41H     ; code of the letter "A"
C002  CD 09 F8    CALL F809H     ; output the character
C005  C3 6C F8    JMP  F86CH     ; back to the MONITOR
```

1. `MC000` ВК, then in turn `0E` ВК, `41` ВК, `CD` ВК, `09` ВК, `F8` ВК, `C3` ВК, `6C` ВК,
   `F8` ВК and `.` — leave the command.
2. `DC000,C007` ВК — check the dump; `KC000,C007` ВК — the checksum is 0444.
3. `GC000` ВК — after `=>GC000` an `A` and a new prompt appear.

### The clock in MONITOR-F

In the full configuration the clock is maintained by MONITOR-F: seconds, minutes and hours
in cells F6FDH, F6FEH, F6FFH (binary-coded decimal); every second the time is shown on the
processor module LED display (A — seconds, L — minutes, H — hours). But MONITOR-F **does
not enable interrupts** on a cold start, and the clock stands still until some command
enables them (`O`, `I`, `V` — at the end of tape operations, `B` — see below). This is the
behaviour of the ROM from the magazine, not of the emulator.

The MONITOR-F command `B` is meant for the first mode (`full-monitor0`): it takes the module
1 MONITOR time from C3FDH–C3FFH and calls its subroutines `RST 5` (0028H) and `RST 3`
(0018H). In the `full` configuration there is no module 1 ROM, those addresses hold RAM
filled with zeros, and the processor "slides" over `NOP` instructions up to 0038H — into
the clock handler FFC1H. Therefore `B` in `full` shows the MONITOR-F time plus 2 s and
enables interrupts — the clock runs. If a program has corrupted RAM at 0000H–0037H, `B`
jumps to an unknown place.

**A bug in the clock handler.** Seconds are counted with `INR A; DAA`, and `INR` does not
affect the carry flag: if an interrupt arrives while the interrupted program has CY = 1
(which happens often in the MONITOR-F keyboard polling loop), `DAA` adds 60H — the seconds
jump, for example, from 27 to 88, the carry into minutes is lost, and the LED display shows
"garbage". A real KR580VM80A behaves the same way; the same bug is in the module 1 MONITOR
clock (00CFH). It can be worked around with a stub in RAM: at vector 0038H, instead of
`JMP FFC1H` —
`PUSH PSW; ORA A; CALL FFC1H; POP PSW; RET`.

How to start the clock correctly:

1. `M38` ВК, then `F5` ВК, `B7` ВК, `CD` ВК, `C1` ВК, `FF` ВК, `F1` ВК, `C9` ВК, `.` —
   the stub that fixes the bug (only a cold start erases it);
2. `MF6FD` ВК, seconds ВК, minutes (for example `30`) ВК, hours (`12`) ВК, `.` — enter
   seconds 2 less than needed;
3. `B` ВК — the time is on the LED display, the clock is running. Instead of `B` you can
   also use `O0,0` ВК (writing one byte "to tape", ≈ 1.5 s; the time shifts by a second or
   two).

After «УСТ» the processor also passes through the clock handler (the `JMP F800H` stub at
0000H is corrupted, see section 12), and the time gains a second.

In `full-monitor0` the clock is maintained by the module 1 MONITOR (commands C and B,
section 5); its time can be shown from MONITOR-F with the `B` command.

---

## 7. Tape recorder

The tape recorder output and input are bit 0 of port A1H of the processor module. The same
output is the "speaker": the sound of «Гамма» (Gamma — musical scale), the clicks of
«Лабиринт» (Labyrinth), the MONITOR-F key click are heard through the sound card (in the
window; at "maximum" speed the sound is off).

Recording formats: the module 1 MONITOR uses the same format as the Micro-80 computer
(commands 9 and A), MONITOR-F — as the Radio-86RK (commands O and I). The emulator writes
WAV files as mono, 16 bit, 44,100 Hz, and reads any WAV files (the first channel is taken
from multichannel ones).

### Writing to tape

1. **Window:** «Магнитофон → Начать запись» (Tape recorder → Start recording);
   **terminal:** F8, `r`.
2. Give the write command: in the module 1 MONITOR — `9`, the start and end address; in
   MONITOR-F — `OC000,C0FF` ВК.
3. Wait until it finishes (the module 1 MONITOR shows the checksum on HL, MONITOR-F — the
   addresses and the checksum on the screen).
4. **Window:** «Магнитофон → Остановить запись и сохранить WAV…» (Stop recording and save
   WAV…); **terminal:** F8, `r`, file name.

### Reading from tape

The tape recorder behaves like a real one with a cassette inserted and the tape standing
still. The tape starts moving by itself as soon as a program reads the tape input (port
A1H) for the first time after the cassette was inserted — that is, when the MONITOR starts
reading (as if you pressed "Play" at that moment). So the cassette can be inserted in
advance — via the menu, with the `--tape` option at startup, or before typing the command:

1. Insert the cassette: «Магнитофон → Вставить кассету (WAV)…» in the window, F8 `w` in the
   terminal, or `ut88 --tape file.wav` at startup. The status line shows «⏹ лента 0.0 с —
   тронется при чтении» ("tape 0.0 s — will start on read"; in the terminal —
   «⏹ 0.0 с, ждёт чтения», "waiting for read").
2. Give the read command: in the module 1 MONITOR — `A` and four offset digits (`0000`); in
   MONITOR-F — `I` ВК. The tape starts, the state changes to «▶».
3. At the end of reading the module 1 MONITOR shows the checksum on HL, MONITOR-F — the
   addresses and the checksum (if the checksum does not match — a second line with «?»).

The tape can also be started and stopped manually: «Магнитофон → Воспроизведение / Стоп»
(Play / Stop) and «Перемотать в начало» (Rewind) in the window (the buttons are also in the
«Магнитофон и файлы» window, F8), F8 `p` and F8 `n` in the terminal. A tape stopped
manually no longer starts by itself — start it with the button or rewind it (rewinding
re-enables auto-start). To read the same cassette again, rewind it.

### Clock interrupts and the tape recorder

The module 1 MONITOR tape drivers do not disable interrupts, and the once-per-second clock
interrupt can corrupt writing or reading of a long block (the magazine, doc/25, advises
installing a toggle switch that disables the interrupt request). In the emulator this
switch is «Машина → Прерывания от часов» (Machine → Clock interrupts) in the window and
F11 `i` in the terminal. Before writing or reading large programs with commands 9/A,
disable the interrupts, and enable them afterwards (the clock will of course fall behind
during that time). MONITOR-F disables interrupts by itself for the duration of each byte
and does not need the switch.

### Fast loading and WAV from a file

The **«Магнитофон → Магнитофон и файлы…»** (Tape recorder and files…) window (F8):

- **Кассета** (Cassette) — the buttons «▶ Воспроизведение» (Play) / «⏹ Стоп» (Stop),
  «⏮ В начало» (Rewind) and the tape state;
- **Быстрая загрузка .bin** (Fast .bin loading) — file and address, the buttons
  «Загрузить» (Load) and «Загрузить и запустить» (Load and run, a jump to this address) —
  without the tape recorder;
- **Сохранить диапазон памяти в .bin** (Save memory range to .bin) — file and the addresses
  «с»/«по» (from/to);
- **WAV из .bin** (WAV from .bin) — make a "cassette" from a binary file: the `.bin` file,
  the `.wav` file, the start address and the format («МОНИТОР модуля 1» — module 1 MONITOR,
  for the A command; «МОНИТОР-F» — for the I command), the «Создать WAV» (Create WAV)
  button. The «Вставить WAV как кассету» (Insert WAV as cassette) button inserts the file
  from the `.wav` field (the tape stands still until reading starts).

In the terminal the same is available as F8, `b` (fast loading), `m` (save range), `o` (WAV
from a memory range).

---

## 8. Snapshots

A snapshot saves everything: the processor, memory, screen, keyboard state, clock,
configuration and the inserted cassette (entirely, with its position and whether the tape
is moving or not), and in the CP/M configuration also the whole contents of the RAM disk.
The file starts with the signature `UT88SNAP`. Snapshots in older format versions (1 —
before the "standing" tape, 2 — before the RAM disk) cannot be loaded.

| Action                             | Window                                             | Terminal                     |
|------------------------------------|----------------------------------------------------|------------------------------|
| quick snapshot                     | F6 (file `./ut88-quick.snap`)                      | F6 (the same file)           |
| load quick snapshot                | F7                                                 | F7                           |
| save snapshot to a file            | «Файл → Сохранить снимок…» (File → Save snapshot…) | F11 `a`, file name           |
| load snapshot from a file          | «Файл → Загрузить снимок…» (File → Load snapshot…) | F11 `o`, file name           |
| start the emulator from a snapshot | `ut88 --snapshot FILE`                             | `ut88 --tui --snapshot FILE` |

The quick snapshot is written to the current directory (where the emulator was started
from). The configuration is taken from the snapshot; the `--config` option has no effect
then. Snapshots are handy in games: save before a difficult part — and you can come back.

---

## 9. Debugger

### In the window (F12 or «Вид → Отладчик», View → Debugger)

The debugger is a separate window. When a breakpoint is hit, emulation pauses and the
debugger window opens by itself.

- **Buttons:** «▶ Продолжить» (Continue) / «⏸ Пауза» (Pause), «Шаг» (Step — one
  instruction), «Шаг с обходом» (Step over — CALL and RST are executed as a whole), the
  «до адреса» (to address) field + «Пуск до» (Run to) (or Enter).
- **Registers** PSW, BC, DE, HL, SP, PC — they can be edited while paused (type a new value
  and leave the field). Below — the flags S Z AC P C, INTE, HLT, the word at the top of the
  stack, the cycle counter and cycles "since the mark" (the «отметка» (mark) button resets
  it — handy for timing subroutines).
- **Breakpoints:** by address (the «адрес» (address) field and «+», «✖» — delete; or click
  a disassembler line) and by port — the port number and direction IN, OUT or IN/OUT. The
  port is checked for IN/OUT instructions before they execute.
- **Disassembler** around PC (the current instruction is highlighted, ● — breakpoint).
- **Memory:** 256 bytes in hex and as characters, the «адрес» (address) field and
  «Показать» (Show), paging ◀◀ ◀ ▶ ▶▶ (by 100H and 10H), the buttons PC, SP, HL — go to
  the address from the register; the line «Записать с адреса … байты: 3E 55 C9» ("Write at
  address … bytes: 3E 55 C9") — write bytes to memory.

### In the terminal (F12)

The debugger screen: registers and stack, disassembler («Код», Code), memory dump,
breakpoints, a key hint. On a breakpoint the debugger opens by itself.

| Key               | Action                                               |
|-------------------|------------------------------------------------------|
| s                 | step one instruction                                 |
| o                 | step over CALL                                       |
| g, Space, F4      | run / stop                                           |
| r                 | run to address                                       |
| b                 | set / clear a breakpoint at an address               |
| p                 | set / clear a port breakpoint (IN and OUT)           |
| x                 | clear all breakpoints                                |
| m                 | memory dump from an address                          |
| a                 | disassembler from an address (empty — from PC)       |
| e                 | edit memory under the cursor (hex digits, Enter/Esc) |
| arrows, PgUp/PgDn | cursor in the memory dump                            |
| c                 | reset the «Δ такты» (Δ cycles) counter               |
| Esc, F12          | return to the machine                                |

---

## 10. Programs from the magazine

Ready-made images are in `programs/`; their description and checksums are in
[programs/README.md](../programs/README.md), the listings — in
[doc/07-programmy.md](../doc/07-programmy.md) (in Russian).

Any program can be started in three ways:

- **with a command** — given for each program below (run from the repository root;
  `--run` jumps straight to the start address);
- **from the menu:** in the window — «Программы → name → Загрузить и запустить» (Programs →
  name → Load and run) or the F9 window, in the terminal — F9, select, Enter (first choose
  the right configuration — programs for another configuration will not load);
- **as in the magazine:** load without `--run` (or «Загрузить» in the menu) and start with
  a MONITOR command (`6`, `7 ADDR`, `G ADDR`).

The «УСТ» (RESET) button is F2 (in the window and in the terminal).

### Fill memory with a constant (`fill-const`, 1989-02)

```sh
ut88 --config minimal --load programs/fill-const.bin --run C000
```

The program immediately waits for input: the start address (4 digits), the end address (4
digits), the constant (2 digits), for example `C100` `C17F` `A5`. After filling the area it
exits to the MONITOR («11»). Check the result with the command `5 C100`. A detailed
walkthrough is in section 5.

### Game «Реакция» — Reaction (`reaction`, 1989-02)

```sh
ut88 --config minimal --load programs/reaction.bin --run C000
```

Press «0», then the person being tested presses any key — all LED digits show «FF». As
soon as the digits start running, the person must press **«УСТ»**. The code on the display
characterizes the reaction: the smaller, the faster.

### Program «Гамма» — Gamma, a musical scale (`gamma`, 1989-02)

```sh
ut88 --config minimal --load programs/gamma.bin --run C03E
```

Sound must be on (only in the window). Enter four digits: two for the note duration, two
for the pause between notes (01–FF), for example `4010`. The scale plays in a loop; stop it
with «УСТ». In the magazine the scale was listened to through a tape recording; in the
emulator you hear it right away.

### Game «Крестики-нолики» — Tic-tac-toe (`tic-tac-toe`, 1989-02)

```sh
ut88 --config minimal --load programs/tic-tac-toe.bin --run C000
```

The cells are numbered like this:

```
01  02  03
08  09  04
07  06  05
```

The microcomputer moves first — to the centre, the right display shows «09». Enter your
move as the cell number — **two digits** (`0` `1` … `0` `8`); the machine's reply appears
on the A display. A loss is code **73**, a draw — **11**.

### Game «Лабиринт» — Labyrinth (`labyrinth` + `labyrinth-field`, 1989-02)

```sh
ut88 --config minimal --load programs/labyrinth.bin --load programs/labyrinth-field.bin --run C000
```

(From the menu the field is loaded automatically.) After starting, press **«0»** — HL
shows «C2FF»: the game uses the field at C200H. The magazine says "any key", but according
to the program code any other key selects the field at C100H (there is none there — HL
will show «C1FF»). You have to get from cell FFH to cell 00H, imagining the 16×16 field in
your head. Keys «1», «2» decrease and increase the high digit of the coordinate, «3», «4» —
the low one. The magazine calls them "left", "right", "up", "down"; but if you look at the
field as the dump `C200`–`C2FF` (16 rows of 16 bytes, byte 01 is a wall), then «1» is one
row up, «2» — down, «3» — left, «4» — right. HL shows the number of steps, A — the
coordinate. Each successful move is accompanied by a click; into a wall — no click, the
coordinate does not change, but the step is counted. A step off the edge of the field is
not performed either, but due to a quirk of the program (the carry flag before DAA) the
step counter adds 161 instead of 1. You can write your own field starting at C200H.

### Display module test (`display-test`, 1989-09)

```sh
ut88 --config full-monitor0 --load programs/display-test.bin --run C000
```

The program writes 55H to cell F400H of the display module RAM in an endless loop and reads
it back — nothing changes on the screen. In the magazine the buses were checked with a
multimeter in single-step mode; in the emulator, open the debugger (F12) instead: while
paused you can see that F400H = 55, and «Шаг» (Step) goes through the instructions
`MVI A,55H` / `STA F400H` / `LDA F400H` / `JMP C000H`. The magazine suggests changing the
code to AAH (byte C001H) and the address to E800H (bytes C003H–C004H and C006H–C007H) —
then a character with that code appears in the top left corner of the screen (55H — «U»).
Exit — «УСТ».

### Game «Тетрис» — Tetris (`tetris`, 1989-09)

The full configuration (`full`) is required. Start:

```sh
make tetris                                      # in the window (make tetris-t — in the terminal)
ut88 --load programs/tetris.bin --run 3000       # the same directly
```

Or in the window: the «Программы» menu (F9) → `tetris` → «Загрузить и запустить». If the
program is only loaded, type `G3000` in MONITOR-F and press **Enter** (this is ВК).

1. «ВАШ УРОВЕНЬ (0-7) ?» ("YOUR LEVEL (0-7)?") — press a digit 0–7: the higher, the faster
   the pieces fall.
2. «НАЖМИТЕ ЛЮБУЮ КЛАВИШУ !» ("PRESS ANY KEY!") — press any key; the moment of the press
   determines the random sequence of pieces.
3. Play. The digits are in the main row of the keyboard:

|      Key      | Action                                                                                 |
|:-------------:|----------------------------------------------------------------------------------------|
|       7       | move the piece left                                                                    |
|       9       | move the piece right                                                                   |
|       8       | rotate the piece                                                                       |
|     Space     | drop the piece                                                                         |
|       6       | speed up: level +1                                                                     |
|       S       | pause («ВОДИТЕЛЬ, УСТАЛ - ОТДОХНИ !» — "DRIVER, TIRED? TAKE A REST!"), any key resumes |
| УС+C (Ctrl+C) | end the game                                                                           |

4. When the well fills up (or after Ctrl+C) — «ИГРА ЗАКОНЧЕНА, ЖЕЛАЕТЕ ПОВТОРИТЬ? (Y/N)»
   ("GAME OVER, PLAY AGAIN? (Y/N)"): `Y` — a new game (the level question again), `N` —
   exit to MONITOR-F.

The score is the line «ЧИСЛО ПОЛНЫХ СТРОК» ("NUMBER OF FULL LINES"): a filled row
disappears and the counter increases.

**Hold a key for about half a second:** Tetris polls the keyboard rarely, a short press may
be lost.

Pieces are drawn with `[[` characters, walls with `##`, empty cells with ` .`: the program
outputs the cell code twice (subroutine 3408H) — this is in the program itself, not a dump
error.

### ROM programmer (`programmator`, 1990-05)

```sh
ut88 --load programs/programmator.bin --run 100
```

Or in MONITOR-F: `G100` ВК. The screen shows the menu «ПРОГРАММАТОР ПЗУ 573РФ2-РФ5» ("ROM
PROGRAMMER 573RF2-RF5"): `R` — read a ROM into RAM, `W` — write to a ROM, `E` — exit to the
MONITOR. The programmer add-on itself (a PPI at ports F8H–FBH) is not emulated — the ports
read as FFH, so "reading" gives an empty ROM (all FF), and writing fails. You can look at
the program and play with it; description — [doc/42-programmator.md](../doc/42-programmator.md)
(in Russian).

### Other

- **Calculator** (1989-03): `ut88 --config minimal --calculator` attaches the library ROM at
  0800H–0FFFH; subroutines and their addresses — [doc/11-kalkulyator.md](../doc/11-kalkulyator.md).
- **«Болотуду» (Bolotudu), «Сенегальские шашки» (Senegalese draughts), «Жизнь» (Life)** —
  the magazine only gives the rules, there are no programs:
  [doc/40-bolotudu.md](../doc/40-bolotudu.md), [doc/41-zhizn.md](../doc/41-zhizn.md)
  (in Russian).

---

## 11. RAM disk and CP/M

The 1990-02 issue of the magazine published two operating systems: **UT-88 OS** («ОС
ЮТ-88») and **CP/M 2.2** in two variants — **CP/M-64** for the electronic **RAM disk**
(«квазидиск», "quasi-disk") and **CP/M-35** without it. Their listings were transcribed
into `roms/` and are built into the emulator. The description from the magazine:
[doc/30-os-ut88.md](../doc/30-os-ut88.md), [doc/31-cpm.md](../doc/31-cpm.md),
[doc/32-kvazidisk.md](../doc/32-kvazidisk.md), [doc/33-cpm-komandy.md](../doc/33-cpm-komandy.md);
code analysis — [doc/35-dampy.md](../doc/35-dampy.md) (all in Russian).

### RAM disk

The RAM disk is 1–4 banks of dynamic RAM, 64 KB each, which are switched in instead of the
main memory **only for stack accesses** of the processor (`PUSH`, `POP`, `CALL`, `RET`,
`RST`, `XTHL`, interrupt acknowledge). A bank is selected by writing to port **40H**:

| Byte to port 40H | What is open                                                  |
|------------------|---------------------------------------------------------------|
| FEH              | bank 1 (CP/M disk tracks 0–63)                                |
| FDH              | bank 2 (tracks 64–127)                                        |
| FBH              | bank 3 (tracks 128–191)                                       |
| F7H              | bank 4 (tracks 192–255)                                       |
| FFH              | RAM disk closed, the stack is in main RAM (as after power-on) |

While a bank is open, the stack access address (the SP value) is an address inside the
bank; this is how CP/M reads a sector with `POP` instructions and writes it with `PUSH`.
The size is set with the `--kvazidisk-size` option (64, 128, 192, 256 KB; in the window
menu the "CP/M" configuration is always 256 KB). The RAM disk contents are included in
snapshots (section 8).

**RAM disk image.** The RAM disk is RAM, and its contents are lost when the "power is
switched off" (the magazine advised powering it from a battery). In the emulator it can be
saved to a file (the raw banks one after another, 64–256 KB) and loaded back:

- `ut88 --boot cpm64 --kvazidisk disk.img` — at startup the image is loaded (if the file
  exists), on exit it is saved to the same file; CP/M-64 boots keeping the files, so the
  disk "survives" an emulator restart;
- in the window — «CP/M → Сохранить образ квазидиска… / Загрузить образ квазидиска…»
  (Save / Load RAM disk image…), in the terminal — F11 `c`, then `s` / `i`. After loading an
  image, start «CP/M-64 (файлы на квазидиске сохраняются)» (files on the RAM disk are
  kept).

### CP/M-64

Start: `ut88 --boot cpm64`, in the window — «CP/M → Загрузить CP/M-64…», in the terminal —
F11 `c` `6`. The emulator restarts MONITOR-F, puts `roms/cpm64.bin` at 3100H and runs the
magazine's loader (`G3100`): the system is moved to C400H–DBFFH and to the RAM disk, the
screen driver — to F500H, then BOOT (DA00H). The screen shows:

```
 CPM V - 2.2  DISK RAM - 256K.

A>
```

Before starting, the emulator patches the cell 4A50H of the listing for the RAM disk size,
as the magazine instructs: 39H for 64 KB, 79H — 128, B9H — 192, **F9H — 256** (the listing
has 39H). The CP/M disk: a track is 8 sectors of 128 bytes (1 KB), a block is 1 KB,
32 directory entries, 6 system tracks; 57, 121, 185 or 249 KB for files.

*A BIOS quirk at 256 KB:* the disk allocation vector (ALV, DC76H) with DSM = F9H takes
32 bytes, and its last byte overlaps the first byte of the directory checksums (CSV,
DC95H). This matters only on an almost full disk (blocks 248–249 in use, more than 247 KB):
CP/M may declare the disk read-only (`BDOS ERR ON A: R/O`) or miscount these two blocks.
At 64–192 KB the tables do not overlap.

The "**with a clean RAM disk**" variant (`--cpm-format`) is as in the magazine: the system
and an empty directory are written to the RAM disk. The normal variant keeps the files if
the RAM disk already contains CP/M with a directory (for example, a loaded image); otherwise
the disk is cleared too.

Input goes through MONITOR-F (the UT-88 keyboard: Latin letters are upper case, lower-case
codes are shown as Russian letters on the screen). Useful keys (doc/33):

| UT-88 keys | PC        | Action                                   |
|------------|-----------|------------------------------------------|
| ВК         | Enter     | execute the command                      |
| ← or УС+H  | Backspace | delete a character                       |
| УС+X       | Ctrl+X    | delete the line                          |
| УС+C       | Ctrl+C    | "warm start": re-read the disk directory |
| УС+S       | Ctrl+S    | pause / resume output                    |

Built-in CP/M commands:

| Command               | What it does                                       |
|-----------------------|----------------------------------------------------|
| `DIR`                 | directory (`NO FILE` if there are no files)        |
| `DIR *.COM`           | directory by mask (`*` and `?`)                    |
| `TYPE NAME.EXT`       | print a text file                                  |
| `ERA NAME.EXT`        | delete file(s), a mask is allowed                  |
| `REN NEW.EXT=OLD.EXT` | rename                                             |
| `SAVE N NAME.EXT`     | save N pages of 256 bytes starting at 100H to disk |
| `NAME`                | run `NAME.COM` from the disk                       |

### Putting files on the CP/M disk

**With the emulator's tools** (the most convenient way): a PC file is written directly into
the CP/M disk directory under its own name (8 + 3 characters, converted to upper case; the
last record is padded with 1AH). The running CP/M is then restarted ("warm start") to
re-read the directory:

- `ut88 --boot cpm64 --cpm-put game.com --cpm-put readme.txt`; a different name on the disk
  goes after `=`: `--cpm-put roms/cpm-ch-com.bin=CH.COM`;
- window: «CP/M → Записать файл на диск CP/M…» (Write file to CP/M disk…) or «CP/M → Диск
  CP/M: файлы…» (CP/M disk: files…) — there files can also be saved back to the PC and
  deleted;
- terminal: F11 `c` `w` and the file path (or `FILE=NAME.EXT`); `x` — save a file from the
  disk to the PC, `d` — delete, `l` — list.

The file goes to the disk the loaded CP/M works with: the CP/M-64 RAM disk or the CP/M-35
disk in RAM.

**As in the magazine — with the SAVE command.** Put the program into RAM starting at 100H
(the «Файл → Загрузить программу…» window, `--load file.bin@100` or with MONITOR
commands) and in CP/M type `SAVE N NAME.COM`, where N is the number of 256-byte pages. This
is how the magazine puts CH.COM on the disk: `SAVE 3 CH.COM`. With CP/M running, loading
into memory does not disturb it as long as the program fits into 0100H–C3FFH.

**From tape — with the CH.COM program** ("CHANGER VERS 1.1", `roms/cpm-ch-com.bin`).
`CH NAME.EXT`: if the file is on the disk, CH writes it to tape (`READY TR FOR OUTPUT, PRESS
CR.`) and verifies the recording (`READY TR FOR VERIFY, PRESS CR.`); if there is no such
file — it reads it from tape (`READY TR FOR INPUT, PRESS CR.`). The tape recorder is the
same as for MONITOR-F (section 7): before pressing ВК, start recording («Магнитофон → Начать
запись») or insert a cassette. This way CP/M files can be stored in WAV files.

### CP/M-35

Start: `ut88 --boot cpm35`, «CP/M → Загрузить CP/M-35…», F11 `c` `3`. The RAM disk is not
needed (it also works in the `full` configuration): the CP/M-35 disk is 35 KB of main RAM
at 5000H–DBFFH, programs get 0100H–33FFH. The banner is `35K CP/M VERS 2.2`. Quirks of the
listing:

- the CP/M-35 BIOS enables interrupts and puts `JMP F800H` at 0038H, so the emulator
  disables clock interrupts (otherwise CP/M would drop into the MONITOR once a second);
- the "warm start" (УС+C and the end of any program) in CP/M-35 exits to MONITOR-F (`=>`);
  return with the command `G4A00`, the files on the disk are kept;
- the emulator clears the directory on loading (5000H–53FFH ← E5H) — the magazine says
  nothing about it, but without it DIR would show garbage; when reloading with files kept,
  the directory stays.

### UT-88 OS

Start: `ut88 --boot os`, «CP/M → Загрузить ОС ЮТ-88», F11 `c` `o`. As in the magazine, the
starter (`G3000`) prints:

```
СТАРТЕР "ОС ЮТ*88"
ВЫКЛЮЧИТЕ ПЗУ МОНИТОРА И НАЖМИТЕ ЛЮБУЮ КЛАВИШУ
```

("STARTER «UT*88 OS»" / "TURN OFF THE MONITOR ROM AND PRESS ANY KEY".)

On the real computer, toggle switches are used at this point to disable the monitor ROMs
(the F000H–FFFFH segment is given to the RAM module) and the clock interrupts. The emulator
does this itself — just press any key. The starter moves the OS monitor to F800H–FFFFH, the
editor, assembler and debugger — to C000H–DFFFH, and the prompt `ЮT*88>` appears. The OS
commands — [doc/30-os-ut88.md](../doc/30-os-ut88.md) (in Russian).

The «ПЗУ МОНИТОРА-F» (MONITOR-F ROM) toggle switch is available manually as «Машина → ПЗУ
МОНИТОРА-F» in the window and F11 `r` in the terminal. A cold start enables the ROM again
(otherwise there would be nothing to run after power-on).

**A fix to the listing.** At address 3320H the OS listing has byte 4DH: the screen scroll
loop of the OS monitor would jump into the middle of an instruction, and the OS would crash
at the very first scroll. The emulator puts 68H there on loading (a jump to the beginning
of the loop) — details in [doc/35-dampy.md](../doc/35-dampy.md) and in
`crates/ut88-core/src/cpm.rs` (`UT88_OS_PATCHES`).

---

## 12. Common problems

**No sound.** Sound is available only in the window (there is none in the terminal
interface). Check: whether the emulator was started with `--no-sound`; whether «Вид → Звук»
(View → Sound) is on and the volume is up; whether the speed is set to "maximum" (sound is
off there). If the sound card is unavailable, the status line shows «Звук недоступен: …»
("Sound unavailable: …") and the 🔇 icon. The sound is the tape output: only programs that
write to port A1H produce it (Gamma, Labyrinth, MONITOR-F key clicks); the module 1 MONITOR
is silent on key presses.

**Keys "don't press".**

- In the full configuration the PC keyboard controls the full keyboard, not the module 1
  buttons. For the module 1 MONITOR (`full-monitor0`), enable «Машина → Клавиатура ПК →
  кнопки модуля 1» (in the terminal — F11 `k`) or press the drawn buttons with the mouse.
- The cursor is in an input field (debugger, tape recorder window) — click on the picture
  of the computer.
- The program polls the keyboard rarely (Tetris) — hold the key longer.
- Alt (РУС) or an F-key is intercepted by the window manager or the terminal — use the
  mouse (РУС "sticks" on click) or the hotkey button bar at the bottom (in the window and in
  the terminal); the F11 menu in the terminal opens with a click on «11Справка».
- In the minimal configuration only 0–9, A–F, Backspace and F2 («УСТ») work.
- Emulation is paused (⏸ in the status line, «ПАУЗА» in the terminal) — press F4.

**The clock does not run or shows "garbage" in the full configuration.** This is how the
MONITOR-F from the magazine behaves: it does not enable interrupts at startup, and its
clock handler corrupts the time if an interrupt arrives with the carry flag CY = 1. The `B`
command in `full` shows the MONITOR-F time (+2 s), not the module 1 MONITOR time. The stub,
setting the time and starting the clock — section 6, "The clock in MONITOR-F". Also check
that «Прерывания от часов» (Clock interrupts) is not turned off.

**«УСТ» in the full configuration does not clear the screen.** MONITOR-F corrupts the byte
of the `JMP F800H` stub at address 0001H while running, and after «УСТ» it does not restart
with a screen clear (ROM behaviour, see
`plans/01-create-emulator/04-deviations-create-emulator.md`, No. 37, in Russian). Commands
are still accepted. A full restart — `GF800` or a cold start.

**A program does not load from tape.** Look at the tape state in the status line: if it is
«⏹ … — стоп» (stop) or «лента кончилась» (end of tape), the tape will not start by itself —
rewind it (section 7) and give the command again. For commands 9/A, disable clock
interrupts. Check the format: a WAV for the A command is made in the «МОНИТОР модуля 1»
(module 1 MONITOR) format, for I — in the «МОНИТОР-F» format.

**Tetris (the ROM programmer) does not work in the minimal configuration.** They need the
display module and RAM — switch the configuration to full («Машина → Конфигурация»,
Machine → Configuration; in the terminal — restart with `--config full`). Switching clears
memory. The program lists (menu, F9) do not load such programs in the minimal
configuration.

**A `.bin` does not load: «нужен адрес загрузки» ("load address required").** Give the
address: `--load file.bin@C000` (in the window — via «Магнитофон и файлы»). Without an
address only the programs from the `programs/README.md` table are loaded.

**The program loaded but did not start.** `--load` only puts the program into memory (the
emulator says so: «загружена, но не запущена» — "loaded but not started"); start it with
`--run ADDR` or with a MONITOR command.

**The «Программы» (Programs) menu is empty («папка programs/ не найдена» — "programs/
folder not found").** Start the emulator from the repository root or give
`--programs path/to/programs`.

**The terminal interface looks cut off.** Enlarge the terminal window to 80×32 or more; the
debugger needs it even wider.
