use clap::Parser;
use std::path::PathBuf;
use tracing::info;

use wtail::tail;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input file (stdin if omitted)
    file: Option<PathBuf>,

    /// Number of initial lines [default: 10]
    #[arg(short = 'n', long = "lines", default_value_t = 10usize)]
    lines: usize,

    /// Follow file as it grows
    #[arg(short = 'f', long = "follow")]
    follow: bool,

    /// Suppress headers/errors
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Poll interval in seconds [default: 0.1]
    #[arg(short = 's', long = "sleep", default_value_t = 0.1f64)]
    sleep: f64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    if let Some(path) = args.file {
        if !args.quiet {
            info!("Tailing {} (last {} lines) follow={}", path.display(), args.lines, args.follow);
        }
        // initial tail
        match tail::tail_initial(&path, args.lines) {
            Ok(lines) => {
                for l in lines {
                    println!("{}", l);
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        if args.follow {
            // handle Ctrl+C
            let quit = tokio::spawn(async move {
                if let Err(e) = tail::follow(&path, args.sleep, |line| {
                    println!("{}", line);
                }).await {
                    eprintln!("Follow error: {}", e);
                }
            });

            // wait for ctrl_c
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    // graceful
                }
                _ = quit => {}
            }
        }
    } else {
        // stdin
        let mut buf = String::new();
        use tokio::io::AsyncReadExt;
        let mut stdin = tokio::io::stdin();
        stdin.read_to_string(&mut buf).await?;
        let mut lines: Vec<String> = buf.lines().map(|s| s.to_string()).collect();
        let start = if lines.len() > args.lines { lines.len() - args.lines } else { 0 };
        for l in lines.drain(start..) {
            println!("{}", l);
        }
        if args.follow {
            eprintln!("-f not supported for stdin; exiting");
        }
    }

    Ok(())
}
