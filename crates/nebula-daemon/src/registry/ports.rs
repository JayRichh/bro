use super::*;
use nebula_core::ports::{PortBlock, WorktreePorts};

impl Daemon {
    // ---- worktree ports ----

    /// `worktree`'s PORT BLOCK, allocating its slot the first time it is
    /// asked for (a session spawning there, its create hook). None only
    /// when the store fails or the slot would run past port 65535 — a
    /// session then simply starts without the variables.
    pub(crate) fn port_block(&self, worktree: &WorktreeId) -> Option<PortBlock> {
        let slot = match self.store.allocate_port_slot(worktree) {
            Ok(slot) => slot,
            Err(e) => {
                tracing::warn!(worktree = %worktree, error = %e, "port slot allocation failed");
                return None;
            }
        };
        let config = crate::config::Config::load();
        PortBlock::of_slot(slot, config.port_base, config.port_block)
    }

    /// The block `worktree` already holds, without allocating one.
    pub(crate) fn held_port_block(&self, worktree: &WorktreeId) -> Option<PortBlock> {
        let slot = self.store.worktree_port_slot(worktree).ok().flatten()?;
        let config = crate::config::Config::load();
        PortBlock::of_slot(slot, config.port_base, config.port_block)
    }

    /// The WORKTREE PORTS the watcher last read: what a new client gets
    /// after its Snapshot, and what `nebula sessions` reports.
    pub(crate) fn worktree_ports(&self) -> Vec<WorktreePorts> {
        self.live_ports.lock().clone()
    }

    /// One pass of the WORKTREE PORTS watcher: every worktree's block,
    /// and the TCP ports in LISTEN state owned by a process in one of its
    /// sessions' trees (an agent's, a terminal's, a RUN TERMINAL's), read
    /// off the machine. Clients hear about it only when it changed.
    pub(crate) async fn refresh_worktree_ports(self: &Arc<Self>) {
        // Each live session's child pid, then the row it belongs to.
        let pids: Vec<(SessionRef, u32)> = self
            .sessions
            .lock()
            .iter()
            .filter_map(|(sref, s)| Some((sref.clone(), s.child_pid?)))
            .collect();
        let Ok((_, worktrees, agents, terminals, slots)) = self
            .store_blocking(|store| {
                let (p, w, a, t) = store.load_tree()?;
                Ok((p, w, a, t, store.port_slots()?))
            })
            .await
        else {
            return;
        };
        let home: HashMap<SessionRef, WorktreeId> = agents
            .into_iter()
            .map(|a| (SessionRef::Agent(a.id), a.worktree_id))
            .chain(
                terminals
                    .into_iter()
                    .map(|t| (SessionRef::Terminal(t.id), t.worktree_id)),
            )
            .collect();
        let roots: Vec<(u32, WorktreeId)> = pids
            .into_iter()
            .filter_map(|(sref, pid)| Some((pid, home.get(&sref)?.clone())))
            .collect();
        // The machine-wide sweeps are a process snapshot and a socket
        // table: off the async workers, like every other blocking read.
        let live = tokio::task::spawn_blocking(move || {
            if roots.is_empty() {
                return HashMap::new();
            }
            nebula_core::ports::live_ports(
                &roots,
                &nebula_core::ports::process_pairs(),
                &nebula_core::ports::listening(),
            )
        })
        .await
        .unwrap_or_default();
        let config = crate::config::Config::load();
        let mut next: Vec<WorktreePorts> = worktrees
            .iter()
            .filter_map(|w| {
                let block = slots
                    .iter()
                    .find(|(id, _)| id == &w.id)
                    .and_then(|(_, slot)| {
                        PortBlock::of_slot(*slot, config.port_base, config.port_block)
                    });
                let live = live.get(&w.id).cloned().unwrap_or_default();
                (block.is_some() || !live.is_empty()).then(|| WorktreePorts {
                    worktree: w.id.clone(),
                    block,
                    live,
                })
            })
            .collect();
        next.sort_by(|a, b| a.worktree.cmp(&b.worktree));
        let changed = {
            let mut current = self.live_ports.lock();
            let changed = *current != next;
            if changed {
                *current = next.clone();
            }
            changed
        };
        if changed {
            self.broadcast(ServerEvent::WorktreePorts { ports: next });
        }
    }
}
