# Contributing

Thank you for your interest in the UT-88 emulator! Bug reports, fixes of the magazine
transcriptions and new features are welcome.

## Building and running

You need a Rust toolchain with `cargo` ([rustup](https://rustup.rs)), GNU make and, on
Linux, the ALSA development package — see [Requirements](README.md#requirements).

```sh
make            # cargo build --release
make help       # list of all make commands
make full       # run the emulator
```

## Before sending a change

```sh
make test       # cargo test
make lint       # cargo clippy --all-targets -- -D warnings; cargo fmt --check
make fmt        # format the code
```

- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` must be clean; CI
  runs both together with the build and the tests.
- New behaviour should come with a test. The machine is tested headless through
  `ut88-core` (`crates/ut88-core/tests/`), the window through `egui_kittest`.
- Long tests (the 8080 instruction exerciser, the Tetris move search) are marked
  `#[ignore]`; run them with `make test-all` when you touch the CPU or the keyboard.
- Behaviour of the original hardware and ROMs is reproduced as printed in the
  magazine, including its bugs. If you change it, describe the reason and the source
  (magazine issue and page, `doc/` file).

## Conventions

The project's working rules are in [CLAUDE.md](CLAUDE.md) (in Russian). In short:

- **Commit messages:** `TYPE(short-comment) : short description` on the first line,
  an empty line, then the details. `TYPE` is one of `doc`, `feat`, `fix`, `test`,
  `build`, `plan`, `rules`, `refactor`, `prompt`, `devi`, `todo` (see the table in CLAUDE.md); `short-comment`
  is a few Latin words joined by hyphens. Changes of different types go into separate
  commits. The project writes descriptions in Russian.
- **Markdown tables** are aligned: every column is padded with spaces so that the `|`
  characters line up in the source, and the separator row is stretched to the column
  width keeping the alignment colons.
- **Tasks, plans and deviations** live in `plans/NN-short-comment/`: the task
  statement (`NN-prompt-….txt`), the plan (`NN-plan-….md`, with a status line) and a log
  of deviations from the plan (`NN-deviations-….md`).

## Documentation

- [README.md](README.md) — quick start (English);
- [docs/MANUAL.md](docs/MANUAL.md) and [manual.md](manual.md) — the user manual in
  English and Russian; keep them in sync when you change the user interface;
- [doc/](doc/README.md) — the magazine articles (Russian).

## License

By contributing you agree that your contributions are dual licensed under MIT OR
Apache-2.0, as described in [README.md](README.md#license).
