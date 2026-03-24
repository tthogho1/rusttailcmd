use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

pub fn tail_initial(path: &Path, lines: usize) -> Result<Vec<String>> {
    let mut f = File::open(path).with_context(|| format!("No such file: {}", path.display()))?;
    let metadata = f.metadata()?;
    let mut file_size = metadata.len() as i64;
    if file_size == 0 {
        return Ok(vec![]);
    }

    let chunk_size: i64 = 8 * 1024;
    let mut buf: Vec<u8> = Vec::new();
    let mut read_pos = file_size;

    while read_pos > 0 && buf.iter().filter(|&&b| b == b'\n').count() < lines {
        let to_read = std::cmp::min(chunk_size, read_pos);
        let start = read_pos - to_read;
        f.seek(SeekFrom::Start(start as u64))?;
        let mut chunk = vec![0u8; to_read as usize];
        f.read_exact(&mut chunk)?;
        // prepend chunk
        let mut newbuf = chunk;
        newbuf.extend_from_slice(&buf);
        buf = newbuf;
        read_pos = start;
    }

    // Split into lines, keep last `lines`
    let mut out = Vec::new();
    for line_bytes in buf.split(|&b| b == b'\n') {
        if line_bytes.is_empty() {
            out.push(String::new());
            continue;
        }
        if let Ok(s) = std::str::from_utf8(line_bytes) {
            out.push(s.to_string());
        } else {
            // skip invalid UTF-8 lines per spec
        }
    }

    // If the file ended with a newline, the split above creates a trailing empty string; remove the final empty unless it was intended
    if buf.ends_with(&[b'\n']) && out.last().map(|s| s.is_empty()).unwrap_or(false) {
        out.pop();
    }

    let len = out.len();
    let start = if len > lines { len - lines } else { 0 };
    Ok(out[start..].to_vec())
}

pub async fn follow(path: &Path, sleep_seconds: f64, mut on_line: impl FnMut(String) + Send + 'static) -> Result<()> {
    use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc::channel;

    let sleep = Duration::from_secs_f64(sleep_seconds);

    // open file for reading (we re-open on errors/rotations)
    let mut last_pos: u64 = 0;
    let mut leftover: Vec<u8> = Vec::new();

    // channel for notify events
    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = RecommendedWatcher::new(move |res| {
        let _ = tx.send(res);
    }, Config::default())?;
    watcher.watch(path, RecursiveMode::NonRecursive)?;

    // initial last_pos is file length
    if let Ok(meta) = std::fs::metadata(path) {
        last_pos = meta.len();
    }

    loop {
        // non-blocking check for events
        let mut triggered = false;
        while let Ok(res) = rx.try_recv() {
            if let Ok(event) = res {
                match event.kind {
                    EventKind::Modify(_) | EventKind::Modify(notify::event::ModifyKind::Any) | EventKind::Create(_) => {
                        triggered = true;
                    }
                    _ => {}
                }
            }
        }

        if !triggered {
            // fallback poll
            tokio::time::sleep(sleep).await;
            if let Ok(meta) = std::fs::metadata(path) {
                if meta.len() > last_pos {
                    triggered = true;
                } else if meta.len() < last_pos {
                    // truncated/rotated
                    last_pos = 0;
                    leftover.clear();
                    triggered = true;
                }
            }
        }

        if triggered {
            // try to read appended data
            match OpenOptions::new().read(true).open(path) {
                Ok(mut f) => {
                    let meta = f.metadata().ok();
                    let cur_len = meta.map(|m| m.len()).unwrap_or(0);
                    if cur_len < last_pos {
                        // file truncated or rotated
                        last_pos = 0;
                    }
                    f.seek(SeekFrom::Start(last_pos)).ok();
                    let mut buf = Vec::new();
                    f.read_to_end(&mut buf).ok();
                    if !buf.is_empty() {
                        // binary detection
                        if buf.iter().any(|&b| b == 0) {
                            // warn and stop following per spec
                            on_line("[wtail] binary data detected, stopping follow".to_string());
                            return Ok(());
                        }

                        // combine leftover + new data
                        if !leftover.is_empty() {
                            let mut combined = leftover.clone();
                            combined.extend_from_slice(&buf);
                            buf = combined;
                            leftover.clear();
                        }

                        // split by newline
                        let mut start_idx = 0usize;
                        for (i, &b) in buf.iter().enumerate() {
                            if b == b'\n' {
                                let slice = &buf[start_idx..i];
                                if let Ok(s) = std::str::from_utf8(slice) {
                                    on_line(s.to_string());
                                }
                                start_idx = i + 1;
                            }
                        }
                        // leftover partial line
                        if start_idx < buf.len() {
                            leftover.extend_from_slice(&buf[start_idx..]);
                        }

                        last_pos = cur_len;
                    }
                }
                Err(e) => {
                    // could be removed/permission error
                    on_line(format!("[wtail] watch error: {}", e));
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }
}
