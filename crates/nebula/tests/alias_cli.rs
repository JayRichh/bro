//! `bro` is Kete's old name, installed beside `kete` as the same program.
//! Each says its own name in usage lines, so the help you read matches the
//! command you typed.

use std::process::Command;

fn help(bin: &str, args: &[&str]) -> String {
    let out = Command::new(bin)
        .args(args)
        .env("COLUMNS", "100")
        .output()
        .unwrap_or_else(|e| panic!("failed to run {bin}: {e}"));
    assert!(out.status.success(), "{bin} {args:?}: {:?}", out.status);
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn kete_and_bro_each_say_their_own_name() {
    for (bin, name) in [
        (env!("CARGO_BIN_EXE_kete"), "kete"),
        (env!("CARGO_BIN_EXE_bro"), "bro"),
    ] {
        let root = help(bin, &["--help"]);
        assert!(
            root.contains(&format!("Usage: {name} ")),
            "{name} --help:\n{root}"
        );
        assert!(
            !root.contains(".exe"),
            "no file extension in usage:\n{root}"
        );
        let sub = help(bin, &["rename", "--help"]);
        assert!(
            sub.contains(&format!("Usage: {name} rename")),
            "{name} rename --help:\n{sub}"
        );
    }
}

#[test]
fn both_names_are_the_same_version() {
    let kete = help(env!("CARGO_BIN_EXE_kete"), &["--version"]);
    let bro = help(env!("CARGO_BIN_EXE_bro"), &["--version"]);
    assert_eq!(kete, bro);
    assert!(kete.starts_with("kete "), "{kete}");
}
