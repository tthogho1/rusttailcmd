use clap::Parser;
use regex::{Regex, RegexBuilder};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use wtail::tail;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input files (stdin if omitted). Multiple files are supported.
    files: Vec<PathBuf>,

    /// Number of initial lines [default: 10]
    #[arg(short = 'n', long = "lines", default_value_t = 10usize)]
    lines: usize,

    /// Output the last N bytes of each file (conflicts with --lines)
    #[arg(short = 'c', long = "bytes", conflicts_with = "lines")]
    bytes: Option<u64>,

    /// Follow file as it grows
    #[arg(short = 'f', long = "follow")]
    follow: bool,

    /// Suppress headers/errors
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Always print headers giving file names
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    /// Poll interval in seconds [default: 0.1]
    #[arg(short = 's', long = "sleep", default_value_t = 0.1f64)]
    sleep: f64,

    /// Only show lines matching PATTERN (substring by default).
    /// Use --regex to treat PATTERN as a regular expression.
    /// Ignored when -c/--bytes is used.
    #[arg(short = 'g', long = "grep", alias = "filter", value_name = "PATTERN")]
    grep: Option<String>,

    /// Treat --grep PATTERN as a regular expression
    #[arg(short = 'E', long = "regex", requires = "grep")]
    regex: bool,

    /// Case-insensitive matching for --grep
    #[arg(short = 'i', long = "ignore-case", requires = "grep")]
    ignore_case: bool,

    /// Invert match: show lines that do NOT match --grep PATTERN
    #[arg(long = "invert-match", requires = "grep")]
    invert_match: bool,
}

/// Compiled line matcher built from --grep options.
#[derive(Clone)]
struct LineMatcher {
    regex: Regex,
    invert: bool,
}

impl LineMatcher {
    fn build(args: &Args) -> anyhow::Result<Option<Self>> {
        let Some(pattern) = args.grep.as_deref() else {
            return Ok(None);
        };
        let pat = if args.regex {
            pattern.to_string()
        } else {
            regex::escape(pattern)
        };
        let regex = RegexBuilder::new(&pat)
            .case_insensitive(args.ignore_case)
            .build()?;
        Ok(Some(Self {
            regex,
            invert: args.invert_match,
        }))
    }

    fn is_match(&self, line: &str) -> bool {
        self.regex.is_match(line) ^ self.invert
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let matcher = LineMatcher::build(&args)?;
    if matcher.is_some() && args.bytes.is_some() && !args.quiet {
        eprintln!("[wtail] note: --grep is ignored when --bytes is used");
    }

    if !args.files.is_empty() {
        // Show headers when multiple files are given, or when -v is set,
        // and not when -q is set.
        let show_headers = !args.quiet && (args.verbose || args.files.len() > 1);

        // initial tail for each file
        for (i, path) in args.files.iter().enumerate() {
            if !args.quiet {
                if let Some(n) = args.bytes {
                    info!(
                        "Tailing {} (last {} bytes) follow={}",
                        path.display(),
                        n,
                        args.follow
                    );
                } else {
                    info!(
                        "Tailing {} (last {} lines) follow={}",
                        path.display(),
                        args.lines,
                        args.follow
                    );
                }
            }
            if show_headers {
                if i > 0 {
                    println!();
                }
                println!("==> {} <==", path.display());
            }
            if let Some(n) = args.bytes {
                match tail::tail_initial_bytes(path, n) {
                    Ok(buf) => {
                        use std::io::Write;
                        let stdout = std::io::stdout();
                        let mut out = stdout.lock();
                        if let Err(e) = out.write_all(&buf) {
                            if !args.quiet {
                                eprintln!("Error writing {}: {}", path.display(), e);
                            }
                        }
                        let _ = out.flush();
                    }
                    Err(e) => {
                        if !args.quiet {
                            eprintln!("Error reading {}: {}", path.display(), e);
                        }
                    }
                }
            } else {
                match tail::tail_initial(path, args.lines) {
                    Ok(lines) => {
                        for l in lines {
                            if matcher.as_ref().map_or(true, |m| m.is_match(&l)) {
                                println!("{}", l);
                            }
                        }
                    }
                    Err(e) => {
                        if !args.quiet {
                            eprintln!("Error reading {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }

        if args.follow {
            // Track which file produced the last output so we can print a
            // header whenever the source switches (mimicking `tail -f`).
            let last_source: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
            let mut handles = Vec::new();

            for path in args.files.iter().cloned() {
                let sleep = args.sleep;
                let last_source = Arc::clone(&last_source);
                let show_headers = show_headers;
                let path_for_task = path.clone();
                let matcher = matcher.clone();

                let handle = tokio::spawn(async move {
                    let path_for_cb = path_for_task.clone();
                    let last_source_cb = Arc::clone(&last_source);
                    let result = tail::follow(&path_for_task, sleep, move |line| {
                        if let Some(m) = matcher.as_ref() {
                            if !m.is_match(&line) {
                                return;
                            }
                        }
                        if show_headers {
                            // blocking lock inside sync callback is fine here
                            let mut guard = last_source_cb.blocking_lock();
                            let need_header = match guard.as_ref() {
                                Some(p) => p != &path_for_cb,
                                None => true,
                            };
                            if need_header {
                                if guard.is_some() {
                                    println!();
                                }
                                println!("==> {} <==", path_for_cb.display());
                                *guard = Some(path_for_cb.clone());
                            }
                        }
                        println!("{}", line);
                    })
                    .await;

                    if let Err(e) = result {
                        eprintln!("Follow error ({}): {}", path_for_task.display(), e);
                    }
                });
                handles.push(handle);
            }

            // wait for ctrl_c or all follow tasks to finish
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    // graceful
                }
                _ = async {
                    for h in handles {
                        let _ = h.await;
                    }
                } => {}
            }
        }
    } else {
        // stdin
        let mut buf = String::new();
        use tokio::io::AsyncReadExt;
        let mut stdin = tokio::io::stdin();
        stdin.read_to_string(&mut buf).await?;
        if let Some(n) = args.bytes {
            let bytes = buf.as_bytes();
            let start = bytes.len().saturating_sub(n as usize);
            use std::io::Write;
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            let _ = out.write_all(&bytes[start..]);
            let _ = out.flush();
        } else {
            let mut lines: Vec<String> = buf.lines().map(|s| s.to_string()).collect();
            let start = if lines.len() > args.lines { lines.len() - args.lines } else { 0 };
            for l in lines.drain(start..) {
                if matcher.as_ref().map_or(true, |m| m.is_match(&l)) {
                    println!("{}", l);
                }
            }
        }
        if args.follow {
            eprintln!("-f not supported for stdin; exiting");
        }
    }

    Ok(())
}
