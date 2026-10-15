//! Windows end to end: the real `kete` binary in a real ConPTY, its own
//! daemon, temp git repos with a local bare repo as `origin`, and a
//! scripted fake agent standing in for Claude Code — the screen read back
//! through vt100, as a user would see it.
//!
//! One binary plays two parts (`harness = false`): run by cargo it is the
//! suite; run by the daemon as a session's CLI (the temp config points the
//! `claude` harness's program at it, and [`agent::ROLE`] rides the
//! environment down) it is the fake agent, which reports status through
//! the hooks the daemon installed in the worktree, exactly as Claude does.
//!
//! `cargo test -p nebula --test e2e_windows [-- <name filter>]`

#[cfg(windows)]
mod agent;
#[cfg(windows)]
mod flows;
#[cfg(windows)]
mod harness;

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    if std::env::var_os(agent::ROLE).is_some() {
        agent::run();
        return;
    }
    run_suite();
}

/// libtest's job, done small: run every test whose name holds the filter
/// (the first argument that is not a flag), one at a time — each brings up
/// a TUI, a daemon and agents of its own — and fail the run if any fails.
#[cfg(windows)]
fn run_suite() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--list") {
        for (name, _) in flows::TESTS {
            println!("{name}: test");
        }
        return;
    }
    // Every test here is an end-to-end run; none is ignored, so a
    // workspace-wide `-- --ignored` runs none of them.
    if args.iter().any(|a| a == "--ignored") {
        println!("\nrunning 0 tests");
        return;
    }
    let filter = args.iter().find(|a| !a.starts_with('-')).cloned();
    let exact = args.iter().any(|a| a == "--exact");
    let chosen: Vec<_> = flows::TESTS
        .iter()
        .filter(|(name, _)| match &filter {
            Some(f) if exact => name == f,
            Some(f) => name.contains(f.as_str()),
            None => true,
        })
        .collect();
    println!("\nrunning {} tests", chosen.len());
    let began = std::time::Instant::now();
    let mut failed = Vec::new();
    for (name, test) in chosen.iter().copied() {
        let start = std::time::Instant::now();
        let outcome = std::panic::catch_unwind(*test);
        let took = start.elapsed().as_secs_f64();
        match outcome {
            Ok(()) => println!("test {name} ... ok ({took:.1} s)"),
            Err(_) => {
                println!("test {name} ... FAILED ({took:.1} s)");
                failed.push(*name);
            }
        }
    }
    println!(
        "\ntest result: {}. {} passed; {} failed; finished in {:.1} s",
        if failed.is_empty() { "ok" } else { "FAILED" },
        chosen.len() - failed.len(),
        failed.len(),
        began.elapsed().as_secs_f64()
    );
    if !failed.is_empty() {
        for name in &failed {
            println!("    {name}");
        }
        std::process::exit(101);
    }
}
