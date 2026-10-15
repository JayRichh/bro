//! BACK / FORWARD between places, the desktop's `shell/history`: a place
//! is the project tab the grid is on, the session card under its cursor
//! and whether that session is full-screen. Every key and click that
//! lands somewhere new notes the place (`event_loop::desk::note_place`);
//! `⌥←` / `⌥→` (or `<` / `>`) and the header's `‹ ›` walk the list
//! (`event_loop::desk::go`). Walking back and then going somewhere new
//! drops the places ahead, as a browser does.

use nebula_core::{AgentId, ProjectId};

/// Most places kept; the oldest go first.
pub const KEPT: usize = 50;

/// Where the grid was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub project: Option<ProjectId>,
    pub session: Option<AgentId>,
    pub full_screen: bool,
}

#[derive(Debug, Clone, Default)]
pub struct History {
    places: Vec<Place>,
    at: usize,
    /// A walk is under way: the next note is where it landed, which takes
    /// the walked-to slot rather than cutting the places ahead.
    walking: bool,
}

impl History {
    /// The place the grid is on now.
    pub fn note(&mut self, place: Place) {
        if self.walking {
            self.walking = false;
            if let Some(slot) = self.places.get_mut(self.at) {
                *slot = place;
                return;
            }
        }
        if self.places.get(self.at) == Some(&place) {
            return;
        }
        if !self.places.is_empty() {
            self.places.truncate(self.at + 1);
        }
        self.places.push(place);
        if self.places.len() > KEPT {
            self.places.remove(0);
        }
        self.at = self.places.len() - 1;
    }

    pub fn can_back(&self) -> bool {
        self.at > 0 && !self.places.is_empty()
    }

    pub fn can_forward(&self) -> bool {
        self.at + 1 < self.places.len()
    }

    /// One step back (`-1`) or forward (`1`): the place to go to.
    pub fn step(&mut self, delta: i64) -> Option<Place> {
        let to = self.at as i64 + delta;
        if to < 0 || to as usize >= self.places.len() {
            return None;
        }
        self.at = to as usize;
        self.walking = true;
        self.places.get(self.at).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(session: &str) -> Place {
        Place {
            project: Some(ProjectId("p".into())),
            session: Some(AgentId(session.into())),
            full_screen: false,
        }
    }

    #[test]
    fn back_and_forward_walk_the_places_and_a_new_one_drops_those_ahead() {
        let mut h = History::default();
        assert!(!h.can_back());
        h.note(at("a"));
        h.note(at("a"));
        h.note(at("b"));
        h.note(at("c"));
        assert_eq!(h.step(-1), Some(at("b")));
        h.note(at("b"));
        assert_eq!(h.step(-1), Some(at("a")));
        h.note(at("a"));
        assert!(!h.can_back() && h.can_forward());
        assert_eq!(h.step(-1), None);
        assert_eq!(h.step(1), Some(at("b")));
        h.note(at("b"));
        h.note(at("d"));
        assert!(!h.can_forward());
        assert_eq!(h.step(-1), Some(at("b")));
    }

    #[test]
    fn a_walk_that_lands_elsewhere_takes_the_slot() {
        let mut h = History::default();
        h.note(at("a"));
        h.note(at("b"));
        h.step(-1);
        // `a` was gone: the grid landed on `x` instead.
        h.note(at("x"));
        assert!(h.can_forward());
        assert_eq!(h.step(1), Some(at("b")));
    }

    #[test]
    fn the_oldest_places_go_past_the_cap() {
        let mut h = History::default();
        for i in 0..KEPT + 5 {
            h.note(at(&i.to_string()));
        }
        let mut back = 0;
        while h.step(-1).is_some() {
            back += 1;
        }
        assert_eq!(back, KEPT - 1);
    }
}
