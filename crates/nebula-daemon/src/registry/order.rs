use super::*;
use nebula_core::ReorderScope;

impl Daemon {
    // ---- user order ----

    /// `ClientRequest::Reorder`: write the user's order for `scope`
    /// (`Store::reorder` has the rules) and push every row whose slot
    /// changed to every subscriber, after the transaction commits. The
    /// clients re-sort on the upserts; nothing else is sent.
    pub fn reorder(self: &Arc<Self>, scope: &ReorderScope, ids: &[String]) -> Result<()> {
        let changed = self.store.reorder(scope, ids)?;
        for id in changed {
            let entity = match scope {
                ReorderScope::Projects => {
                    self.store.get_project(&ProjectId(id))?.map(Entity::Project)
                }
                ReorderScope::Worktrees(_) => self
                    .store
                    .get_worktree(&WorktreeId(id))?
                    .map(Entity::Worktree),
                // The live row, liveness included: a store row says dead.
                ReorderScope::Agents(_) => Some(Entity::Agent(self.agent_entity(&AgentId(id))?)),
            };
            if let Some(entity) = entity {
                self.broadcast(ServerEvent::EntityUpserted { entity });
            }
        }
        Ok(())
    }
}
