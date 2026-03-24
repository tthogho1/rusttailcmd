# wtail

`wtail` is a small cross-platform tail replacement focused on Windows `cmd.exe` compatibility but developed on macOS. It implements `tail -n` and `-f` follow behavior with efficient file watching via `notify` and a poll fallback.

Usage:

```
wtail [OPTIONS] [FILE]

Args:
  [FILE]  Input file (stdin if omitted)

Options:
  -n, --lines <LINES>    Number of initial lines [default: 10]
  -f, --follow           Follow file as it grows
  -q, --quiet            Suppress headers/errors
  -s, --sleep <SECONDS>  Poll interval in seconds [default: 0.1]
  -h, --help             Print help
  -V, --version          Print version
```

Build notes (cross-compile to Windows):

```
rustup target add x86_64-pc-windows-gnu
cargo build --target x86_64-pc-windows-gnu --release
```

Design highlights:
- Initial tail reads from file end in blocks to efficiently gather last N lines.
- `notify` is used to receive filesystem events; a polling fallback is used when needed.
- Handles truncation/rotation by resetting read position.
- UTF-8 only; invalid UTF-8 lines are skipped.
- Detects binary (NUL) in appended data and warns/stops follow.
