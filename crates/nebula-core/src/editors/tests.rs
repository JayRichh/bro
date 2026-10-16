use super::*;

/// What this machine has, printed: a hand check, never run by default.
/// It only reads (the registry, vswhere, the window list, the icons into
/// a temp folder); nothing is launched or moved.
/// `cargo test -p nebula-core editors::tests::this_machine -- --ignored --nocapture`
#[test]
#[ignore]
fn this_machine() {
    let found = detect(&detect::RealProbe);
    for e in &found {
        println!(
            "{:<28} {:<14} {:<12} {:<10} {:?} {}",
            e.id,
            e.short,
            e.version,
            e.edition.as_deref().unwrap_or("-"),
            e.source,
            e.exe.display()
        );
    }
    let dir = tempfile::tempdir().unwrap();
    for e in &found {
        println!("icon {}: {:?}", e.id, icon(e, 32, dir.path()));
    }
    for w in windows(&found) {
        println!("window {} {} {:?}", w.editor, w.pid, w.title);
    }
}
