//! State for the views ported over from the desktop window: the
//! SUBAGENTS the daemon reports (`crate::subagents`), the ATTENTION
//! STRIP's round-robin (`crate::attention`) and the BACK / FORWARD
//! history of places (`crate::history`). One field on `App`, so the ports
//! stay out of the state structs the grid's own code lives in.

#[derive(Debug, Clone, Default)]
pub struct DeskState {
    pub subagents: crate::subagents::Subagents,
    pub attention: crate::attention::Turns,
    pub history: crate::history::History,
}
