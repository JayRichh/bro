//! WORKTREE PORTS: a stable block of localhost ports per worktree, and
//! the ports its sessions are actually listening on.
//!
//! Two checkouts of one app both running `npm run dev` fight over `:3000`.
//! So every WORKTREE gets a PORT BLOCK of its own — `port_block` ports
//! from `port_base + port_block × slot` (3000, 3010, 3020, … by default),
//! its slot the lowest free one when it was first asked for, persisted by
//! the DAEMON so the block never moves — and every session and RUN
//! TERMINAL in it starts with the block in its environment ([`PortBlock::env`]):
//! `PORT`, `KETE_PORT` and `BRO_PORT` name the first port, `KETE_PORT_1` … the rest. Dev
//! servers that read `PORT` (Next.js, Express, most of the Node world) bind
//! there without being told; a Vite config can read it too. The WORKTREE
//! HOOK gets the same variables, so a create hook can write the port into
//! a `.env` or a proxy route.
//!
//! What is *live* is read off the machine, not assumed: the TCP sockets in
//! LISTEN state ([`listening`]) whose owner is in one of a worktree's
//! session process trees ([`live_ports`]). The worktree band's rule draws
//! them (`:3010 ●`), `kete sessions --json` lists them, and the browser and
//! screenshot keys open them.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// `PORT`: what most dev servers bind when it is set.
pub const PORT: &str = "PORT";
/// `KETE_PORT`: the block's first port, under a name nothing else uses.
pub const KETE_PORT: &str = "KETE_PORT";
/// `BRO_PORT`: the same, under Kete's old name, so a dev config written
/// for Bro keeps finding its port.
pub const BRO_PORT: &str = "BRO_PORT";

/// The first port of slot 0, unless `port_base` says otherwise.
pub const DEFAULT_BASE: u16 = 3000;
/// Ports per worktree, unless `port_block` says otherwise.
pub const DEFAULT_BLOCK: u16 = 10;

/// One worktree's PORT BLOCK: `size` ports from `first`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortBlock {
    pub first: u16,
    pub size: u16,
}

impl PortBlock {
    /// Slot `slot`'s block under `base` / `block`, or None when it would
    /// run past port 65535.
    pub fn of_slot(slot: u32, base: u16, block: u16) -> Option<Self> {
        let block = block.max(1);
        let first = u32::from(base) + slot.checked_mul(u32::from(block))?;
        let last = first + u32::from(block) - 1;
        (last <= u32::from(u16::MAX)).then_some(Self {
            first: first as u16,
            size: block,
        })
    }

    /// The environment a session in the worktree starts with: `PORT` and
    /// `KETE_PORT` on the first port, `KETE_PORT_1` … `KETE_PORT_<size-1>`
    /// on the rest, and the same again under `BRO_PORT`.
    pub fn env(&self) -> Vec<(String, String)> {
        let mut out = vec![
            (PORT.to_string(), self.first.to_string()),
            (KETE_PORT.to_string(), self.first.to_string()),
            (BRO_PORT.to_string(), self.first.to_string()),
        ];
        for i in 1..self.size {
            for name in [KETE_PORT, BRO_PORT] {
                out.push((format!("{name}_{i}"), (self.first + i).to_string()));
            }
        }
        out
    }

    /// Whether `port` is one of the block's.
    pub fn contains(&self, port: u16) -> bool {
        port >= self.first && u32::from(port) < u32::from(self.first) + u32::from(self.size)
    }
}

/// What the DAEMON tells its clients about one worktree's ports
/// (`ServerEvent::WorktreePorts`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreePorts {
    pub worktree: crate::WorktreeId,
    /// Its PORT BLOCK, once one has been allocated.
    pub block: Option<PortBlock>,
    /// Ports its sessions' process trees are listening on, ascending.
    pub live: Vec<u16>,
}

/// Every TCP socket in LISTEN state on this machine, as `(port, pid)`.
/// Windows reads the owner-pid tables (`platform::listening_ports`); unix
/// asks `lsof`, best effort — none installed reads as nothing listening.
pub fn listening() -> Vec<(u16, u32)> {
    #[cfg(windows)]
    {
        crate::platform::listening_ports()
    }
    #[cfg(unix)]
    {
        std::process::Command::new("lsof")
            .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpn"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .map(|out| parse_lsof(&String::from_utf8_lossy(&out.stdout)))
            .unwrap_or_default()
    }
}

/// Every process on the machine as `(pid, ppid)`, for [`live_ports`]'s
/// tree walk: the toolhelp snapshot on Windows, `ps` on unix.
pub fn process_pairs() -> Vec<(u32, u32)> {
    #[cfg(windows)]
    {
        crate::platform::process_table()
            .into_iter()
            .map(|row| (row.pid, row.ppid))
            .collect()
    }
    #[cfg(unix)]
    {
        let Ok(out) = std::process::Command::new("ps")
            .args(["-axo", "pid=,ppid="])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
        else {
            return Vec::new();
        };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| {
                let mut cols = line.split_whitespace();
                Some((cols.next()?.parse().ok()?, cols.next()?.parse().ok()?))
            })
            .collect()
    }
}

/// `lsof -Fpn` output: a `p<pid>` line, then an `n<addr>:<port>` line per
/// socket it holds.
pub fn parse_lsof(text: &str) -> Vec<(u16, u32)> {
    let mut out = Vec::new();
    let mut pid = None;
    for line in text.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.trim().parse().ok();
        } else if let (Some(addr), Some(pid)) = (line.strip_prefix('n'), pid) {
            if let Some(port) = addr.rsplit(':').next().and_then(|p| p.parse().ok()) {
                out.push((port, pid));
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The ports listened on by each root's process tree: `roots` maps a
/// session's child pid to the worktree it runs in, `table` is every
/// `(pid, ppid)` on the machine, `listening` every `(port, pid)`. A
/// worktree with no listener is absent.
pub fn live_ports<W: Clone + Eq + std::hash::Hash>(
    roots: &[(u32, W)],
    table: &[(u32, u32)],
    listening: &[(u16, u32)],
) -> HashMap<W, Vec<u16>> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(pid, ppid) in table {
        if pid != ppid {
            children.entry(ppid).or_default().push(pid);
        }
    }
    let mut owner: HashMap<u32, W> = HashMap::new();
    for (root, worktree) in roots {
        let mut stack = vec![*root];
        let mut seen = HashSet::new();
        while let Some(pid) = stack.pop() {
            if !seen.insert(pid) {
                continue;
            }
            owner.entry(pid).or_insert_with(|| worktree.clone());
            if let Some(kids) = children.get(&pid) {
                stack.extend(kids);
            }
        }
    }
    let mut out: HashMap<W, Vec<u16>> = HashMap::new();
    for (port, pid) in listening {
        if let Some(worktree) = owner.get(pid) {
            out.entry(worktree.clone()).or_default().push(*port);
        }
    }
    for ports in out.values_mut() {
        ports.sort_unstable();
        ports.dedup();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_is_a_block_of_its_own_and_its_env_names_every_port() {
        let block = PortBlock::of_slot(1, DEFAULT_BASE, DEFAULT_BLOCK).unwrap();
        assert_eq!(
            block,
            PortBlock {
                first: 3010,
                size: 10
            }
        );
        let env = block.env();
        assert_eq!(env[0], ("PORT".to_string(), "3010".to_string()));
        assert_eq!(env[1], ("KETE_PORT".to_string(), "3010".to_string()));
        assert_eq!(env[2], ("BRO_PORT".to_string(), "3010".to_string()));
        assert_eq!(env[3], ("KETE_PORT_1".to_string(), "3011".to_string()));
        assert_eq!(env[4], ("BRO_PORT_1".to_string(), "3011".to_string()));
        assert!(env.contains(&("KETE_PORT_9".to_string(), "3019".to_string())));
        assert_eq!(
            env.last().unwrap(),
            &("BRO_PORT_9".to_string(), "3019".to_string())
        );
        assert!(block.contains(3019) && !block.contains(3020));
        assert_eq!(PortBlock::of_slot(7000, 3000, 10), None, "past 65535");
    }

    #[test]
    fn listeners_belong_to_the_worktree_whose_tree_owns_them() {
        // 10 → 11 → 12 (worktree a), 20 (worktree b), 30 nobody's.
        let table = [(10, 1), (11, 10), (12, 11), (20, 1), (30, 1)];
        let listening = [(3000, 12), (3001, 12), (3010, 20), (5432, 30)];
        let live = live_ports(&[(10, "a"), (20, "b")], &table, &listening);
        assert_eq!(live.get("a"), Some(&vec![3000, 3001]));
        assert_eq!(live.get("b"), Some(&vec![3010]));
        assert_eq!(live.len(), 2, "postgres is nobody's session");
    }

    #[test]
    fn lsof_field_output_parses() {
        let text = "p123\nn*:3000\nn[::1]:3001\np9\nn127.0.0.1:5173\n";
        assert_eq!(parse_lsof(text), vec![(3000, 123), (3001, 123), (5173, 9)]);
    }
}
