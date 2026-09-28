//! Subject identity supplied by a plugin from canonical data.
//!
//! This describes who the task concerns. It is separate from source evidence and
//! does not imply affiliations, aliases, or relationships absent from the frame.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct EntityMeta {
    pub name: String,
    pub entity_type: String,
    pub entity_id: i32,
    pub sport: String,
}

impl EntityMeta {
    /// Compact identity for a predicate; the canonical ID stays in the receipt.
    pub fn reference(&self) -> String {
        format!("{}, the {} {}", self.name, self.sport, self.entity_type)
    }
}

/// Identity used for prose; database identity remains in plugin provenance.
#[derive(Serialize)]
pub struct WritingIdentity<'a> {
    name: &'a str,
    entity_type: &'a str,
    sport: &'a str,
}
impl EntityMeta {
    pub fn for_writing(&self) -> WritingIdentity<'_> {
        WritingIdentity {
            name: &self.name,
            entity_type: &self.entity_type,
            sport: &self.sport,
        }
    }
}
