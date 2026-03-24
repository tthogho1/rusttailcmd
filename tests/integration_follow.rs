use std::fs::OpenOptions;
use std::io::Write;
use std::time::Duration;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_follow_receives_appends() {
    let f = NamedTempFile::new().expect("create temp file");
    let path = f.path().to_path_buf();

    // initial content
    {
        let mut fh = OpenOptions::new().write(true).open(&path).unwrap();
        writeln!(fh, "start").unwrap();
        fh.flush().unwrap();
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(10);

    // spawn follow task (clone path for the task)
    let follow_path = path.clone();
    let follow_handle = tokio::spawn(async move {
        let _ = wtail::tail::follow(&follow_path, 0.05, move |line| {
            let tx = tx.clone();
            // fire-and-forget send
            let _ = tokio::spawn(async move { let _ = tx.send(line).await; });
        }).await;
    });

    // give the follow task a moment to start watching
    tokio::time::sleep(Duration::from_millis(200)).await;

    // append lines
    for i in 0..3 {
        {
            let mut fh = OpenOptions::new().append(true).open(&path).unwrap();
            writeln!(fh, "appended {}", i).unwrap();
            fh.flush().unwrap();
        }
        // wait for the watcher/poll to pick up
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    // collect a few messages
    let mut got = Vec::new();
    for _ in 0..3 {
        if let Some(l) = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await.ok().flatten() {
            got.push(l);
        }
    }

    // stop follow
    follow_handle.abort();

    assert!(got.iter().any(|s| s.contains("appended 0")), "did not receive appended 0");
    assert!(got.iter().any(|s| s.contains("appended 1")), "did not receive appended 1");
    assert!(got.iter().any(|s| s.contains("appended 2")), "did not receive appended 2");
}
