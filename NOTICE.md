# Notice: third-party materials

The source code of this emulator is licensed under MIT OR Apache-2.0 (see
[LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE)). **That license does
not apply to the materials listed below**: they belong to their respective right
holders and are included only for research, educational and historical purposes —
to document and preserve a computer that was published for amateurs to build, and to
let the emulator run the software that was printed for it.

If you are a right holder and object to any of these materials being here, please open
an issue or contact the repository owner, and they will be removed.

## Magazine «ЮТ для умелых рук» (1989–1990)

The UT-88 computer, its schematics, ROMs and programs were published in the magazine
«ЮТ для умелых рук» («Young Technician — for Skilful Hands»), a supplement to the
magazine «Юный техник» (*Young Technician*), Moscow, issues of 1989 No. 2 – 1990 No. 5.
The computer was designed by V. Bartenev and the radio electronics club of the
Ramenskoye House of Pioneers on request of the editorial office. All rights to these
publications belong to the editorial office of «Юный техник» and to the authors of
the articles.

Materials in this repository derived from the magazine:

| Path                                 | What it is                                                                                                                   |
|--------------------------------------|------------------------------------------------------------------------------------------------------------------------------|
| `doc/ЮТ для умелых рук 1989-02.djvu` | scan of the 1989 No. 2 issue                                                                                                 |
| `doc/*.md`                           | the articles converted to Markdown (Russian), with editorial notes and analysis added for this project                       |
| `doc/img/`                           | illustrations, schematics and photos from the magazine pages (some redrawn or rendered for this project)                     |
| `roms/`                              | dumps typed in from the printed listings: MONITOR-F, character generator, calculator ROM, CP/M-64, CP/M-35, CH.COM, UT-88 OS |
| `monitor/`                           | the module 1 MONITOR ROM: photos of the printed dump, the dump, a fixed image and a disassembly                              |
| `programs/`                          | programs typed in from the printed dumps (games, tests, ROM programmer)                                                      |

The printed listings were transcribed and checked against the checksums where the
magazine gave them; deviations and fixes are documented in `doc/` and
`programs/README.md`. The disassemblies and annotations are the project's own work, but
they describe code that belongs to the original authors.

The ROM and system images from `roms/` and `monitor/` are embedded into the `ut88`
binary (`crates/ut88-core/src/roms.rs`), so binaries built from this repository contain
them too; the same terms apply.

### CP/M 2.2

The CP/M-64 and CP/M-35 images in `roms/` contain CP/M 2.2 (CCP and BDOS) by Digital
Research, adapted for the UT-88 as printed in the magazine. CP/M is a trademark of
Digital Research / its successors. The CP/M 2.2 code is included on the same terms as
the rest of the magazine material.

## 8080 test programs (`crates/i8080/tests/roms/`)

Classic CP/M test programs for the Intel 8080, used by the CPU integration tests. They
were downloaded from Nicolas Allemand's repository
[superzazu/8080](https://github.com/superzazu/8080) (folder `cpu_tests/`); the MIT
license of that repository covers its emulator code, not these programs. Details and
checksums are in [crates/i8080/tests/roms/README.md](crates/i8080/tests/roms/README.md).

| File          | Program and authors                                                                                                             | License                                                                               |
|---------------|---------------------------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------|
| `TST8080.COM` | 8080/8085 CPU Diagnostic v1.0, Microcosm Associates (Kelly Smith), 1980, via the SIG/M library; additions by Mike Douglas, 2012 | freely distributed (public domain, SIG/M)                                             |
| `8080PRE.COM` | preliminary test for the 8080 Exerciser; Frank D. Cringle, Ian Bartholomew                                                      | GNU GPL v2 or later                                                                   |
| `8080EXM.COM` | 8080 Instruction Exerciser, port of zexlax.z80 (Frank D. Cringle, 1994) by Ian Bartholomew, 2009                                | GNU GPL v2 or later                                                                   |
| `CPUTEST.COM` | DIAGNOSTICS II V1.2 — CPU TEST, SuperSoft Associates, 1981                                                                      | commercial program, widely distributed with emulator test suites; no explicit license |

These files are test data only: they are not compiled into the emulator binary.
