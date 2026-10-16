//! The collection log, `<data>/critters.json`: which species have visited,
//! how often, pets, names, the first visit. Machine-local and never
//! exported. Both apps read and write this same shape (the desktop through
//! its Tauri commands), so it is versioned and every field defaults. The
//! hosts do the JSON (this crate depends on serde alone): an unreadable
//! file is an empty log, since it is an easter egg's tally.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::world::Note;

pub const LOG_VERSION: u32 = 1;
pub const LOG_FILE: &str = "critters.json";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub seen: u32,
    pub pets: u32,
    pub fed: u32,
    /// The name the user gave this species' visitors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Local date of the first visit, `YYYY-MM-DD`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_seen: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CritterLog {
    pub version: u32,
    /// By species id.
    pub species: BTreeMap<String, Entry>,
    /// Genie lamps taken.
    pub lamps: u32,
    /// "Hide critters until tomorrow": the local date it was chosen on.
    /// Visitors stay away while today's date equals it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_on: Option<String>,
}

impl Default for CritterLog {
    fn default() -> Self {
        CritterLog {
            version: LOG_VERSION,
            species: BTreeMap::new(),
            lamps: 0,
            hidden_on: None,
        }
    }
}

impl CritterLog {
    /// Folds a world note in. `today` is the local date, `YYYY-MM-DD`.
    pub fn record(&mut self, note: &Note, today: &str) {
        match note {
            Note::Seen { species } => {
                let e = self.species.entry((*species).to_string()).or_default();
                e.seen += 1;
                if e.first_seen.is_none() {
                    e.first_seen = Some(today.to_string());
                }
            }
            Note::Petted { species } => {
                self.species.entry((*species).to_string()).or_default().pets += 1
            }
            Note::Fed { species } => {
                self.species.entry((*species).to_string()).or_default().fed += 1
            }
            Note::Named { species, name } => {
                self.species.entry((*species).to_string()).or_default().name = Some(name.clone())
            }
            Note::Lamp => self.lamps += 1,
        }
    }

    /// Visitors are hidden today.
    pub fn hidden(&self, today: &str) -> bool {
        self.hidden_on.as_deref() == Some(today)
    }

    /// The given names, for `World::names`.
    pub fn names(&self) -> BTreeMap<String, String> {
        self.species
            .iter()
            .filter_map(|(id, e)| e.name.clone().map(|n| (id.clone(), n)))
            .collect()
    }
}
