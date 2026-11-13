# КР580ВМ80А assembler and linker

The repository contains a cross-assembler and a linker for the КР580ВМ80А CPU (the
Soviet clone of the Intel 8080) aimed at the UT-88: write a program in Intel-style
assembly language on the PC, build it into a file the emulator loads, and run it.

- **`asm-k580vm80a`** — the assembler: one source file → flat binary, a hex dump in
  the magazine format, Intel HEX, or a relocatable object file `.link`. It has a
  preprocessor (`INCLUDE`, `IF`, `MACRO`, `REPT`) and writes a listing.
- **`link-k580vm80a`** — the linker: several `.link` modules → one bin/hex/Intel HEX
  image with a link map.

The full reference is built into the programs: `asm-k580vm80a -he` and
`link-k580vm80a -he` print it in English, `-h` in Russian. This page is an overview
with examples; when in doubt, the built-in help is authoritative.

Messages, the listing and the link map are in Russian (`ошибка` — error,
`примечание` — note).

## Contents

1. [Building](#1-building)
2. [Quick start](#2-quick-start)
3. [Assembler options](#3-assembler-options)
4. [Source syntax](#4-source-syntax)
5. [Directives](#5-directives)
6. [Preprocessor](#6-preprocessor)
7. [Listing](#7-listing)
8. [Output formats](#8-output-formats)
9. [Modules and linking](#9-modules-and-linking)
10. [Examples and make commands](#10-examples-and-make-commands)
11. [Errors](#11-errors)

## 1. Building

Both programs are part of the Cargo workspace (crate `crates/asm-k580vm80a`) and are
built together with the emulator:

```sh
make            # cargo build --release
target/release/asm-k580vm80a -he
target/release/link-k580vm80a -he
```

Every `make` command below builds them first.

## 2. Quick start

`examples/asm/hello.asm` prints two lines on the display module screen through the
MONITOR-F routine F809H and returns to the MONITOR-F prompt:

```asm
PUTCH   EQU     0F809H          ; MONITOR-F: print the character in C
WARM    EQU     0F86CH          ; MONITOR-F: warm start (prompt "=>")

        ORG     3000H
START:  LXI     H,MSG
NEXT:   MOV     A,M
        ORA     A               ; 0 ends the string
        JZ      DONE
        MOV     C,A
        PUSH    H
        CALL    PUTCH
        POP     H
        INX     H
        JMP     NEXT
DONE:   JMP     WARM

MSG:    DB      0DH,0AH,'HELLO, WORLD!'
        DB      0DH,0AH,'ПРИВЕТ, ЮТ-88!',0
        END     START
```

Build it and run it in the emulator:

```sh
make asm-run F=examples/asm/hello.asm
```

This writes `examples/asm/hello.hex`, picks the full configuration (the program calls
F809H) and starts the emulator with `--load examples/asm/hello.hex --run 3000`. The same
by hand:

```sh
target/release/asm-k580vm80a -f examples/asm/hello.asm -o hello.hex
target/release/ut88 --config full --load hello.hex --run 3000
```

## 3. Assembler options

```
asm-k580vm80a -f INPUT.asm -o OUTPUT [-t bin|hex|ihex|link] [-l LISTING.lst]
              [-D NAME[=VALUE]]... [-I DIR]... [--fill XX] [--charset koi7|ascii]
              [--base ADDRESS]
```

| Option            | Meaning                                                                                          |
|-------------------|--------------------------------------------------------------------------------------------------|
| `-f FILE`         | source file (required), UTF-8                                                                    |
| `-o FILE`         | output file (required); not created if there are errors                                          |
| `-t TYPE`         | output type `bin`, `hex`, `ihex`, `link`; overrides the extension of `-o`                        |
| `-l FILE`         | write a listing                                                                                  |
| `-D NAME[=VALUE]` | define a symbol as if by `EQU` at the top (without a value — 1); repeatable                      |
| `-I DIR`          | search directory for `INCLUDE` / `INCBIN`; repeatable                                            |
| `--fill XX`       | filler byte for gaps in a bin image (hex, default 00)                                            |
| `--charset CS`    | encoding of strings: `koi7` (default, the UT-88 character set) or `ascii`                        |
| `--base ADDR`     | load address of `CSEG` when building straight to bin/hex/ihex (default 0000); not with `-t link` |
| `-h`, `-he`, `-V` | help in Russian, help in English, version                                                        |

Without `-t` the type comes from the output extension: `.bin`/`.rom` — bin, `.hex` —
magazine dump, `.ihx`/`.ihex` — Intel HEX, `.link` — object file. Any other extension
without `-t` is an error. Exit code: 0 — success, 1 — assembly errors, 2 — command line
error.

```sh
asm-k580vm80a -f prog.asm -o prog.hex                 # dump for the emulator
asm-k580vm80a -f prog.asm -o prog.bin -l prog.lst     # binary and listing
asm-k580vm80a -f prog.asm -o prog.ihx -D DEBUG        # Intel HEX, symbol DEBUG = 1
asm-k580vm80a -f prog.asm -o prog.txt -t hex          # type given explicitly
```

## 4. Source syntax

```
[label[:]]  [mnemonic|directive  [operands]]  [; comment]
```

- A **label** is a name followed by a colon anywhere in the line, or a name without a
  colon starting in column 1. The name before `EQU`, `SET` and `MACRO` is always a
  label.
- **Names**: Latin letters, digits, `_ ? @ .`, not starting with a digit. Case does not
  matter anywhere — `Loop`, `LOOP` and `loop` are the same name. Register names
  (`A B C D E H L M SP PSW`) and operator words (`MOD SHL SHR AND OR XOR NOT EQ NE LT
  LE GT GE HIGH LOW`) are reserved.
- **Numbers**: `0C000H` (hex with suffix, must start with a digit), `$C000`, `0xC000`,
  `1010B`, `17O`/`17Q`, `99`/`99D`; character constants `'A'`, `'AB'` (a word, the first
  character is the high byte). `$` is the address of the current line.
- **Expressions** (highest priority first): `( )`; unary `+ - HIGH LOW`;
  `* / MOD SHL SHR`; `+ -`; comparisons `EQ NE LT LE GT GE` (also `= <> < <= > >=`, true
  is 0FFFFH); `NOT`; `AND`; `OR XOR`. A byte operand accepts −256…255, a word
  −65535…65535.
- **Relocatable values**: addresses of labels in `CSEG`/`DSEG` and `EXTRN` symbols are
  known only after placement. Allowed: `label ± constant`, `label1 − label2` of the same
  segment, `HIGH label`, `LOW label`, comparison of labels of one segment. Anything else
  is the error «выражение нельзя переместить» (expression cannot be relocated).
- **Instructions**: all 244 documented КР580ВМ80А opcodes with Intel mnemonics (`MOV`,
  `MVI`, `LXI`, …, `RST 0…7`). Undocumented opcodes are not accepted.

## 5. Directives

| Directive                 | Meaning                                                                      |
|---------------------------|------------------------------------------------------------------------------|
| `ORG addr`                | address of the following bytes (in `CSEG`/`DSEG` — offset in the segment)    |
| `name EQU expr`           | constant symbol                                                              |
| `name SET expr`           | redefinable symbol                                                           |
| `DB` / `DEFB`             | bytes and strings: `DB 'TEXT',0DH,0AH,0`; a quote inside a string is doubled |
| `DW` / `DEFW`             | 16-bit words, low byte first                                                 |
| `DS n[,fill]` / `DEFS`    | reserve n bytes, or n copies of `fill`                                       |
| `END [addr]`              | end of the source; `addr` is the start address                               |
| `INCBIN "file"`           | insert a binary file                                                         |
| `CSEG` / `DSEG` / `ASEG`  | code segment, data segment (relocatable), absolute segment (default)         |
| `NAME name`               | module name (for object files)                                               |
| `PUBLIC` / `GLOBAL` names | export symbols to other modules                                              |
| `EXTRN` / `EXTERN` names  | import symbols from other modules (only when building a `.link`)             |

Strings use the UT-88 KOI-7 encoding by default: 20H–5FH as ASCII, Russian letters
60H–7EH (lower-case Russian letters get the same codes, Ё = Е, there is no Ъ).
`--charset ascii` rejects Russian letters.

## 6. Preprocessor

| Directive                      | Meaning                                                                              |
|--------------------------------|--------------------------------------------------------------------------------------|
| `INCLUDE "file"`               | insert a source file (searched next to the including file, then in `-I` directories) |
| `IF expr` / `ELSE` / `ENDIF`   | conditional assembly; symbols in `expr` must be defined above or by `-D`             |
| `IFDEF name` / `IFNDEF name`   | condition: symbol defined (not defined) above or by `-D`                             |
| `name MACRO [p1,...]` … `ENDM` | define a macro; call it like an instruction: `name a1,a2`                            |
| `LOCAL n1,...`                 | labels unique to each expansion of a macro or `REPT` (`??0001`, …)                   |
| `EXITM`                        | leave the macro expansion (or the rest of a `REPT`)                                  |
| `REPT n` … `ENDM`              | repeat the lines n times                                                             |

Macro parameters are replaced word by word; inside quotes only `&p` is replaced; `&`
outside quotes glues words (`L&N`). An argument containing a comma is written in
`<…>`. Nesting depth is limited (macros and `REPT` — 64, `INCLUDE` — 16).

```asm
SHOW    MACRO   POS,VAL         ; show VAL on LED pair POS
        IF      VAL = 0
        XRA     A
        ELSE
        MVI     A,VAL
        ENDIF
        STA     9000H+POS
        ENDM

SQUARES:
I       SET     0
        REPT    5
I       SET     I+1
        DB      I*I             ; 1, 4, 9, 16, 25
        ENDM
```

The complete example is `examples/asm/macro.asm`.

## 7. Listing

`-l FILE` writes a listing in the format of `monitor/monitor.lst`:

```
     =9000          DISP    EQU     9000H
                            ORG     0C000H
C000  3E 11         START:  MVI     A,11H
C002  32 00 90              STA     DISP
C005  21 01 90              LXI     H,DISP+1
```

Columns 1–4 are the address (`=value` for `EQU`/`SET`); column 5 is blank for an
absolute address, `'` for a `CSEG` offset, `"` for a `DSEG` offset; up to four code
bytes follow (longer data continue on the next lines); column 19 is the `INCLUDE`
depth; column 20 is `+` for lines produced by a macro or `REPT` and `-` for lines of a
false `IF` branch; the source line starts at column 21. A symbol table sorted by name
ends the listing. The listing is not written if there are errors.

## 8. Output formats

| Format | Extension       | Contents                                                                              |
|--------|-----------------|---------------------------------------------------------------------------------------|
| bin    | `.bin`, `.rom`  | bytes from the lowest to the highest address; gaps filled with `--fill`; no address   |
| hex    | `.hex`          | text dump as printed in the magazine and in `programs/*.hex`: `C000 3E 11 32 …`       |
| ihex   | `.ihx`, `.ihex` | Intel HEX: data records (16 bytes), record 03 with the `END` start address, record 01 |
| link   | `.link`         | relocatable object file for `link-k580vm80a`                                          |

The **hex** dump has an address and up to 16 bytes per line; lines start at 16-byte
boundaries, gaps start a new line. The emulator loads both hex flavours with
`--load FILE.hex` (Intel HEX is recognized by the leading `:`), and a `.bin` with
`--load FILE.bin@ADDRESS`.

```
C000 3E 11 32 00 90 21 01 90 77 11 02 90 12 C7          (hex)
:0EC000003E113200902101907711029012C77C                  (ihex)
:040000030000C00039                                      (start address C000H)
:00000001FF
```

The **link** object file is plain text: a signature line `UT88LINK 1`, the module name,
segments with sizes, data, `PUBLIC`/`EXTERN` symbols, relocations (`RELOC`), the entry
point and a final `END`. The format is described in `link-k580vm80a -he`.

## 9. Modules and linking

A larger program can be split into modules. Each module puts its code in `CSEG` and
data in `DSEG` (absolute parts may stay in `ASEG` with `ORG`), exports names with
`PUBLIC` and imports names with `EXTRN`. Every module is assembled into a `.link`
file, then the linker places and joins them:

```
link-k580vm80a MODULE.link... -o OUTPUT [-t bin|hex|ihex] [-m MAP.map]
               [--base ADDR] [--cseg ADDR] [--dseg ADDR] [-e SYMBOL] [--fill XX]
```

| Option            | Meaning                                                                     |
|-------------------|-----------------------------------------------------------------------------|
| `-o FILE`         | output file, type from the extension (`.bin`, `.hex`, `.ihx`) or `-t`       |
| `-m FILE`         | link map: segments of each module and all `PUBLIC` symbols with addresses   |
| `--base ADDR`     | address of the first module's `CSEG` (default C000H); `--cseg` is a synonym |
| `--dseg ADDR`     | address of the first module's `DSEG` (default — right after all the code)   |
| `-e SYMBOL`       | start address from a `PUBLIC` symbol (otherwise from the one `END address`) |
| `--fill XX`       | filler byte for gaps in bin                                                 |
| `-h`, `-he`, `-V` | help in Russian, help in English, version                                   |

Layout: the `CSEG` parts of all modules go one after another in command-line order,
then all the `DSEG` parts, `ASEG` parts stay at their addresses. Overlaps, unresolved
or duplicated symbols and out-of-range bytes are errors that name the module and the
source line. The result is byte-for-byte the same as assembling the concatenated source
with `asm-k580vm80a --base` at the same address.

The example `examples/asm/hello-link/` consists of `main.asm` (the main program, strings
in `DSEG`) and `print.asm` (a library with `PRINT` and `NEWLINE`):

```asm
; main.asm                              ; print.asm
        NAME    MAIN                            NAME    PRINT
        EXTRN   PRINT,NEWLINE                   PUBLIC  PRINT,NEWLINE
        PUBLIC  START                           CSEG
        CSEG                            PRINT:  MOV     A,M
START:  CALL    NEWLINE                         ORA     A
        LXI     H,MSG1                          RZ
        CALL    PRINT                           ...
        ...                                     DSEG
        JMP     0F86CH                  CRLF:   DB      0DH,0AH,0
        DSEG
MSG1:   DB      'HELLO FROM TWO MODULES!',0
        END     START
```

```sh
asm-k580vm80a -f examples/asm/hello-link/main.asm  -o main.link
asm-k580vm80a -f examples/asm/hello-link/print.asm -o print.link
link-k580vm80a main.link print.link -o hello-link.hex -m hello-link.map --base 3000
target/release/ut88 --config full --load hello-link.hex --run 3000
```

The map (`-m`) lists where everything went:

```
  Модуль  Файл        Сегмент  Начало  Конец  Размер
  MAIN    main.link   CSEG     3000H   3014H  0015H
  MAIN    main.link   DSEG     302BH   305AH  0030H
  PRINT   print.link  CSEG     3015H   302AH  0016H
  PRINT   print.link  DSEG     305BH   305DH  0003H
```

## 10. Examples and make commands

The examples live in `examples/asm/`; every file starts with a comment (in Russian)
saying what it does and how to build and run it.

| File                                          | Configuration | What it does                                                                                                                            |
|-----------------------------------------------|---------------|-----------------------------------------------------------------------------------------------------------------------------------------|
| `ones.asm`                                    | minimal       | the first program of the magazine: «11» on all six LEDs, then `RST 0` to the MONITOR                                                    |
| `fill.asm`                                    | minimal       | fill memory with a constant (magazine 1989-02) — byte-for-byte `programs/fill-const.bin`                                                |
| `gamma.asm`                                   | minimal       | «Гамма», a musical scale — byte-for-byte `programs/gamma.bin`, source recovered from the dump                                           |
| `hello.asm`                                   | full          | print two lines through MONITOR-F F809H, return to MONITOR-F (F86CH)                                                                    |
| `draw.asm`                                    | full          | drawing straight into the screen RAM (E800H + row × 40H + column): frame, diagonals, inverse box, caption; any key returns to MONITOR-F |
| `macro.asm`                                   | minimal       | macros with `LOCAL`, `REPT`, `IF`/`IFDEF`: sum of squares on the LEDs                                                                   |
| `hello-link/main.asm`, `hello-link/print.asm` | full          | two modules with `PUBLIC`/`EXTRN`, `CSEG`/`DSEG`, linked at 3000H                                                                       |

| Command                                           | What it does                                                                                                                |
|---------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------|
| `make asm F=file.asm [O=file.hex]`                | assemble; the default output is `file.hex` next to the source                                                               |
| `make asm-run F=file.asm [C=minimal\|full]`       | assemble to `file.hex` and run it in the emulator                                                                           |
| `make asm-load F=file.asm [C=full\|minimal]`      | assemble and load without running (default `full`); start it with `G address` (MONITOR-F) or «7 address» (module 1 MONITOR) |
| `make link O=file.hex M="a.link b.link" [B=addr]` | link modules; the map is written next to the output (`file.map`)                                                            |
| `make asm-examples`                               | build all examples (and link `hello-link/hello-link.hex`)                                                                   |

`make asm-run` (script `scripts/asm-run.sh`) takes the start address from `END address`;
without it — the lowest address of the program. The configuration is `C=` or a guess:
the full configuration if the source refers to addresses E000H–FFFFH (MONITOR-F
routines, screen memory), otherwise the minimal one. `UT88=echo make asm-run F=…` only
prints the emulator command line.

```sh
make asm-run F=examples/asm/ones.asm        # «11 11 11» on the LEDs
make asm-run F=examples/asm/fill.asm        # then type C100 C17F A5 on the keypad
make asm-run F=examples/asm/gamma.asm       # then type four digits, e.g. 1002
make asm-run F=examples/asm/hello.asm       # full configuration, text on the screen
make asm F=examples/asm/macro.asm O=macro.bin
make asm-examples
```

The tests in `crates/asm-k580vm80a/tests/examples.rs` check that `ones.asm`,
`fill.asm` and `gamma.asm` match the magazine bytes and run all examples in the
emulator core.

## 11. Errors

Errors are printed as `file:line:column: ошибка: text`, all errors of a pass at once;
the output file is not created:

```
prog.asm:2:15: ошибка: неизвестный символ X
prog.asm:3:13: ошибка: неизвестный символ LOOP
asm-k580vm80a: ошибок: 2, выходной файл не создан
```

(«неизвестный символ» — unknown symbol; «ошибок: 2, выходной файл не создан» — 2
errors, no output file.) An error inside a macro or an included file is followed by
notes («примечание») showing where the macro was called or the file was included.
Linker errors name the module, its `.link` file and the source position of the symbol.
