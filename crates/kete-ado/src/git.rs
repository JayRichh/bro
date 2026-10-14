//! Running a process (git) to the end within a deadline.

use std::process::{Command, Output};
use std::time::{Duration, Instant};

/// Run `cmd` to the end, or kill it after `timeout` (`what timed out after
/// N s`). Its stdout and stderr are drained as it runs, so a chatty
/// process never stalls on a full pipe; on a timeout the readers are left
/// to finish on their own, as a killed git's helpers may hold the pipes.
pub fn output_within(mut cmd: Command, timeout: Duration, what: &str) -> Result<Output, String> {
    use std::io::Read;
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not run {what}: {e}"))?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{what} timed out after {} s", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("could not wait for {what}: {e}")),
        }
    };
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slow_command_is_killed_at_its_timeout() {
        #[cfg(windows)]
        let cmd = {
            let mut c = Command::new("cmd");
            c.args(["/C", "ping -n 30 127.0.0.1 >NUL"]);
            c
        };
        #[cfg(not(windows))]
        let cmd = {
            let mut c = Command::new("sleep");
            c.arg("30");
            c
        };
        let started = Instant::now();
        let e = output_within(cmd, Duration::from_millis(300), "the wait").unwrap_err();
        assert_eq!(e, "the wait timed out after 0 s");
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
