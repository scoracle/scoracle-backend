//! The Prose-shaped cognition slot's vocabulary.
//!
//! A character plugin prepares a world, the model returns prose, and the plugin
//! enforces its own surface. Nothing here decides what the prose may say: that is
//! the plugin's prepared world and its `prompt.rs` manual. This module only
//! describes the response the model is permitted to return.
//!
//! The shared prose decoder is where the permitted surface becomes mechanical rather
//! than a parser's incidental behavior. F4 supplies that body — one validator
//! over plugin-chosen keys and dimensions, replacing the six divergent schemas —
//! and it must leave the current per-plugin enforcement unchanged for Influencer
//! while Journalist's paragraph rule is decided by replay rather than assumed.
use serde_json::Value;

/// Reader-facing and readability limits for one plugin's response.
///
/// The two numbers are not the same kind of thing, and this plan previously
/// treated them as one. `total_max_chars` is a product constraint on what a
/// reader is shown; it is shared and it is not negotiable per plugin.
/// `paragraph_max_chars` is a writing policy: Influencer enforces 140 today and
/// Journalist enforces nothing. Forcing 140 onto Journalist, or dropping it from
/// Influencer, would make one of them worse in order to share a validator, so it
/// is a parameter and the choice is recorded per plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dimensions {
    /// Ceiling across every prose key in the response.
    pub total_max_chars: usize,
    /// Ceiling for one paragraph, or `None` when the plugin does not apply one.
    pub paragraph_max_chars: Option<usize>,
}

impl Dimensions {
    /// The shared product ceiling, with a plugin's readability policy.
    pub const fn new(total_max_chars: usize, paragraph_max_chars: Option<usize>) -> Self {
        Self {
            total_max_chars,
            paragraph_max_chars,
        }
    }
}

/// The Prose slot: a prepared world in, a keyed prose map out.
#[derive(Clone, Debug)]
pub struct Prose {
    /// The plugin's output keys. `["body"]` for Influencer, `report_1..report_N`
    /// for Journalist, `["body"]` for Scout. The shape is shared; the
    /// keys are the plugin's.
    pub keys: Vec<String>,
    pub dims: Dimensions,
}

impl Prose {
    pub fn new(keys: &[&str], dims: Dimensions) -> Self {
        Self::new_owned(keys.iter().map(|k| (*k).to_string()).collect(), dims)
    }

    /// For a plugin whose keys are computed rather than written out, such as the
    /// Journalist's `report_1..report_N`.
    pub fn new_owned(keys: Vec<String>, dims: Dimensions) -> Self {
        Self { keys, dims }
    }

    /// The `form` block a world presents, describing structure only.
    pub fn form(&self) -> Value {
        let mut form = serde_json::json!({
            "keys": self.keys,
            "max_chars": self.dims.total_max_chars,
        });
        if let Some(limit) = self.dims.paragraph_max_chars {
            form["paragraph_max_chars"] = serde_json::json!(limit);
        }
        form
    }

    /// The permissive JSON shape a response must satisfy. Deliberately
    /// `additionalProperties: false` over exactly the declared keys so the model
    /// cannot invent a field the plugin would have to ignore.
    pub fn schema(&self) -> Value {
        let mut properties = serde_json::Map::new();
        for key in &self.keys {
            properties.insert(key.clone(), serde_json::json!({"type": ["null", "string"]}));
        }
        serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": properties,
            "required": self.keys,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::support::form::BODY_MAX_CHARS;

    #[test]
    fn the_shared_ceiling_is_a_parameter_and_the_keys_are_the_plugins() {
        let influencer = Prose::new(&["body"], Dimensions::new(BODY_MAX_CHARS, Some(140)));
        let journalist = Prose::new(&["report_1"], Dimensions::new(BODY_MAX_CHARS, Some(140)));
        assert_eq!(influencer.schema()["required"], serde_json::json!(["body"]));
        assert_eq!(
            journalist.schema()["required"],
            serde_json::json!(["report_1"])
        );
        // A shared validator, not a shared policy: the same rule set, and the
        // readability ceiling is each plugin's declaration.
        assert_eq!(
            influencer.dims.total_max_chars,
            journalist.dims.total_max_chars
        );
        assert_eq!(influencer.dims.paragraph_max_chars, Some(140));
    }

    #[test]
    fn a_response_may_not_carry_a_field_the_plugin_did_not_declare() {
        let prose = Prose::new(&["headline", "body"], Dimensions::new(1200, Some(140)));
        let schema = prose.schema();
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
        let keys = schema["properties"].as_object().unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains_key("headline") && keys.contains_key("body"));
    }
}
