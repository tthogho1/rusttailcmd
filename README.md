# wtail

`wtail` is a small cross-platform `tail` replacement focused on Windows `cmd.exe` compatibility but developed on macOS. It implements `tail -n` / `-c` and `-f` follow behavior with efficient file watching via `notify` and a polling fallback. It also supports multiple input files and built-in line filtering (grep).

## Usage

```
wtail [OPTIONS] [FILES]...

Args:
  [FILES]...  Input files (stdin if omitted). Multiple files are supported.

Options:
  -n, --lines <LINES>      Number of initial lines [default: 10]
  -c, --bytes <BYTES>      Output the last N bytes of each file (conflicts with --lines)
  -f, --follow             Follow file as it grows
  -q, --quiet              Suppress headers/errors
  -v, --verbose            Always print headers giving file names
  -s, --sleep <SECONDS>    Poll interval in seconds [default: 0.1]
  -g, --grep <PATTERN>     Only show lines matching PATTERN (substring by default).
                           Alias: --filter. Ignored when -c/--bytes is used.
  -E, --regex              Treat --grep PATTERN as a regular expression
  -i, --ignore-case        Case-insensitive matching for --grep
      --invert-match       Show lines that do NOT match --grep PATTERN
  -h, --help               Print help
  -V, --version            Print version
```

## Examples

```sh
# Show the last 10 lines of a file
wtail app.log

# Show the last 50 lines
wtail -n 50 app.log

# Follow a file as it grows
wtail -f app.log

# Show the last 4 KiB of bytes (binary-safe, useful for huge single-line logs)
wtail -c 4096 huge.jsonl

# Multiple files: each section is prefixed with "==> FILE <==" headers
wtail app1.log app2.log

# Follow multiple files concurrently; headers switch as the source changes
wtail -f app1.log app2.log

# Force headers even for a single file
wtail -v app.log

# Filter: show only lines containing "ERROR"
wtail -g ERROR app.log

# Case-insensitive substring match
wtail -g timeout -i app.log

# Regular expression match
wtail -E -g '^(ERROR|WARN) ' app.log

# Inverted match (everything except ERROR)
wtail -g ERROR --invert-match app.log

# Combine: follow multiple files, show only 5xx responses
wtail -f -E -g ' 5[0-9]{2} ' access1.log access2.log
```

## Quick Start

Fast commands to build, run, and test locally.

```sh
# Build and run in release mode:
cargo run --release -- [OPTIONS] [FILES...]

# Run the test suite:
cargo test

# Install locally with cargo:
cargo install --path .
```

## Build

```sh
cargo build --release
```

### Cross-compile to Windows

```sh
rustup target add x86_64-pc-windows-gnu
cargo build --target x86_64-pc-windows-gnu --release
```

## Design Highlights

- **Initial tail** reads from the file end in blocks to efficiently gather the last N lines.
- **Bytes mode (`-c`)** seeks directly to `file_size - N` and writes raw bytes to stdout (no UTF-8 decoding, binary-safe).
- **File watching** uses `notify` for filesystem events, with a polling fallback so it works reliably across platforms (including Windows).
- **Rotation / truncation** is handled by resetting the read position when the file shrinks.
- **Multiple files** are tailed concurrently with `tokio::spawn`; a shared "last source" marker prints `==> FILE <==` headers whenever the output source changes (matching GNU `tail -f` behavior).
- **Grep / filter** is compiled once via the `regex` crate; literal patterns are escaped automatically, and `--invert-match` flips the result. Filtering is applied to both the initial output and the follow output (and is intentionally skipped in `-c` bytes mode).
- **UTF-8** lines only in line mode; invalid UTF-8 lines are skipped.
- **Binary detection**: if NUL bytes appear in appended data during follow, a warning is emitted and follow stops.
- **Graceful shutdown** on `Ctrl+C` via `tokio::signal`.

## License

MIT OR Apache-2.0
