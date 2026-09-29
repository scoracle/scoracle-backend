//! The Prose-shaped cognition slot's vocabulary.
//!
//! A character plugin prepares a world, the model returns prose, and the plugin
//! enforces its own surface. Nothing here decides what the prose may say: that is
//! the plugin's prepared world and its `prompt.rs` manual. This module only
//! describes the response the model is permitted to return.
//!
//! `Contract::enforce` is where the permitted surface becomes mechanical rather
//! than a parser's incidental behavior. F4 supplies that body — one validator
//! over plugin-chosen keys and dimensions, replacing the six divergent schemas —
//! and it must leave the current per-plugin enforcement unchanged for Influencer
//! while Journalist's paragraph rule is decided by replay rather than assumed.
use super::Contract;
use crate::plugins::support::form;
use anyhow::Result;
use serde::Serialize;
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
    /// for Journalist, `["headline", "body"]` for Scout. The shape is shared; the
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
        serde_json::json!({
            "keys": self.keys,
            "max_chars": self.dims.total_max_chars,
            "paragraph_max_chars": self.dims.paragraph_max_chars,
        })
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

impl Contract for Prose {
    /// A prepared world. Its shape is the plugin's; nothing here reads it yet.
    type Request = Value;
    /// The keyed prose map, after enforcement.
    type Response = Value;

    /// The declared surface, checked. This is the whole point of the slot: a
    /// response that does not match what the plugin said it would return is
    /// refused rather than parsed permissively.
    fn enforce(&self, _request: &Value, response: Value) -> Result<Value> {
        let map = form::parse_prose_map(&response.to_string(), &self.keys, self.dims)?;
        Ok(serde_json::to_value(map)?)
    }
}

/// The `form` block shared shape, so a plugin's declared surface and the shape it
/// hands the model cannot drift apart unnoticed.
pub fn declared_form(prose: &Prose) -> String {
    serde_json::to_string(&prose.form()).expect("Prose form serializes")
}

/// Common limits re-exported for a plugin's declaration to read from one place.
pub use form::BODY_MAX_CHARS;

/// A plugin's world is opaque here on purpose: assembling it is the plugin's job
/// and belongs in its `cognition` module, not in this vocabulary.
pub type World<'a> = &'a dyn Serialize;

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn enforcement_is_real_and_still_fails_closed() {
        // F1b landed this contract refusing to parse anything, with a passing
        // test pinning that. F4 supplied the validator, so the test is inverted:
        // a conforming response is now accepted, and the refusals are the point.
        let prose = Prose::new(&["body"], Dimensions::new(1200, Some(140)));
        let ok = prose
            .enforce(&Value::Null, serde_json::json!({"body": "Cedar won."}))
            .unwrap();
        assert_eq!(ok["body"], serde_json::json!("Cedar won."));

        // A declared contract that accepted everything would be worse than none.
        for refused in [
            serde_json::json!({}),                                        // omitted slot
            serde_json::json!({"body": "Cedar won.", "score": 7}),        // undeclared field
            serde_json::json!({"body": 7}),                               // not prose
            serde_json::json!({"body": "  "}),                            // blank
            serde_json::json!({"body": "é".repeat(1201)}),                // over body ceiling
            serde_json::json!({"body": format!("{}.", "é".repeat(140))}), // over paragraph
        ] {
            assert!(
                prose.enforce(&Value::Null, refused.clone()).is_err(),
                "contract accepted {refused}"
            );
        }
    }

    #[test]
    fn a_declined_slot_is_not_a_missing_slot() {
        // `body: null` is the Influencer's abstention and is legitimate; a
        // missing `body` is a dropped slot. The two must not collapse.
        let prose = Prose::new(&["body"], Dimensions::new(1200, Some(140)));
        assert!(prose
            .enforce(&Value::Null, serde_json::json!({"body": null}))
            .is_ok());
        assert!(prose.enforce(&Value::Null, serde_json::json!({})).is_err());
    }

    #[test]
    fn a_plugin_chooses_its_own_keys_and_the_validator_accepts_only_them() {
        let declared = Prose::new(&["headline", "body"], Dimensions::new(1200, Some(140)));
        let other = Prose::new(&["body", "lede"], Dimensions::new(1200, Some(140)));
        let response = serde_json::json!({"headline": "A win", "body": "Cedar won."});
        assert!(declared.enforce(&Value::Null, response.clone()).is_ok());
        // The same bytes are a contract violation for a plugin that declared
        // different keys. The shape is shared; the keys are the plugin's.
        assert!(other.enforce(&Value::Null, response).is_err());
    }

    #[test]
    fn a_plugin_may_opt_out_of_the_paragraph_rule() {
        // The Journalist's recorded non-participation. It still enforces the
        // shared body ceiling; only the readability policy is absent.
        let no_paragraph_rule =
            Prose::new_owned(vec!["report_1".into()], Dimensions::new(1200, None));
        let long = "é".repeat(400);
        assert!(no_paragraph_rule
            .enforce(&Value::Null, serde_json::json!({"report_1": long}))
            .is_ok());
        assert!(no_paragraph_rule
            .enforce(
                &Value::Null,
                serde_json::json!({"report_1": "é".repeat(1201)})
            )
            .is_err());
    }
}
