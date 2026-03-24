use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_tail_initial_basic() {
    let mut f = NamedTempFile::new().expect("create temp file");
    // write 20 lines
    for i in 0..20 {
        writeln!(f, "line {}", i).unwrap();
    }

    let path = f.path();
    let lines = wtail::tail::tail_initial(path, 10).expect("tail_initial");
    assert_eq!(lines.len(), 10);
    assert_eq!(lines[0], "line 10");
    assert_eq!(lines[9], "line 19");
}
