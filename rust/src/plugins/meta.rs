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
        // Storage keeps the competition namespace used by adapters. Articulation
        // receives the underlying sport so identity does not imply a recap genre.
        let sport = match self.sport.as_str() {
            "NBA" => "basketball",
            "NFL" => "American football",
            sport => sport,
        };
        WritingIdentity {
            name: &self.name,
            entity_type: &self.entity_type,
            sport,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_identity_describes_the_sport_not_the_competition_namespace() {
        let identity = EntityMeta {
            name: "Cedar Comets".into(),
            entity_type: "team".into(),
            entity_id: 7,
            sport: "NBA".into(),
        };
        assert_eq!(
            serde_json::to_value(identity.for_writing()).unwrap(),
            serde_json::json!({"name":"Cedar Comets","entity_type":"team","sport":"basketball"})
        );
    }
}
