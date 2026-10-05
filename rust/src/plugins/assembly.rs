//! One renderer for a prepared world.
//!
//! This is shared rendering and nothing else. It does not know which parts a
//! plugin has, what nests inside what, how anything is paired or selected, or
//! what a part means. Those decisions belong to the plugin that assembled the
//! world, and a change that puts any of them here is a defect.
//!
//! # Why parts carry pre-rendered JSON rather than `serde_json::Value`
//!
//! `serde_json` is built here without its `preserve_order` feature, so a
//! `Value::Object` **sorts its keys alphabetically** on serialization, while a
//! `#[derive(Serialize)]` struct emits fields in declaration order. That
//! difference is load-bearing here: `form.rs` records that field order
//! demonstrably changes SmolLM3's output. Building a `Value` per part would
//! therefore silently alphabetize every part and change the product's
//! behavior. Rendering each part to its own JSON string preserves the exact
//! bytes the plugin's types produce.
//!
//! # What is shared
//!
//! - rendering in insertion order, never sorted, so the wire order is a stated
//!   choice rather than an accident of struct declaration;
//! - one [`World::hash`] over the rendered parts, so every plugin fingerprints a
//!   world the same way;
//! - the guarantee that a prepared world is rendered exactly once, in one place,
//!   by production, replay and the evaluation harness alike.
//!
//! # What is not shared
//!
//! Which parts exist, and in what order, for a given plugin. Each plugin names
//! its own parts and their nesting, and a test pins each plugin's chosen key
//! order. The renderer guarantees determinism, not a particular order.
use serde::Serialize;

/// An ordered prepared world.
///
/// Insertion order is the wire order and is load-bearing; this never sorts. The
/// parts are stored already rendered, so each keeps the byte order its own type
/// produces.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    parts: Vec<(&'static str, String)>,
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a part. The value is rendered immediately, at the point the plugin
    /// hands it over, so a later mutation cannot change a world already built.
    pub fn part(mut self, name: &'static str, value: impl Serialize) -> Self {
        self.parts.push((
            name,
            serde_json::to_string(&value).expect("world part serializes"),
        ));
        self
    }

    /// Append a part the plugin has already rendered.
    pub fn rendered(mut self, name: &'static str, json: String) -> Self {
        self.parts.push((name, json));
        self
    }

    /// The part names, in wire order.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.parts.iter().map(|(name, _)| *name)
    }

    /// The world as the model will read it.
    ///
    /// Rendering is deterministic and byte-identical across calls: the parts are
    /// already strings, so this only concatenates them in insertion order.
    pub fn render(&self) -> String {
        let mut out = String::from("{");
        for (index, (name, value)) in self.parts.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&serde_json::to_string(name).expect("part name serializes"));
            out.push(':');
            out.push_str(value);
        }
        out.push('}');
        out
    }

    /// One fingerprint over the rendered world, so every plugin hashes a world
    /// the same way. Adding, removing, reordering or changing any part changes
    /// it, which is what makes it usable as a debounce and fencing input.
    pub fn hash(&self) -> String {
        crate::util::hash_components(&self.render())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parts_render_in_insertion_order_and_never_sort() {
        // Alphabetical order would be identity, form, fresh, voice. The wire
        // order is whatever the plugin chose, and it is preserved.
        let world = World::new()
            .part("voice", "Clear.")
            .part("identity", json!({"name": "Cedar"}))
            .part("fresh", json!([{"report_key": "report_1"}]));
        assert_eq!(
            world.render(),
            r#"{"voice":"Clear.","identity":{"name":"Cedar"},"fresh":[{"report_key":"report_1"}]}"#
        );
        assert_eq!(
            world.names().collect::<Vec<_>>(),
            ["voice", "identity", "fresh"]
        );
    }

    #[test]
    fn a_part_keeps_its_own_field_order_rather_than_being_alphabetized() {
        // `serde_json::Value` sorts object keys in this build. A part handed over
        // as a Value is therefore already sorted, but a struct-derived part is
        // not, and the renderer must not add sorting of its own.
        #[derive(serde::Serialize)]
        struct Ordered {
            zeta: u8,
            alpha: u8,
        }
        let world = World::new().part("form", Ordered { zeta: 1, alpha: 2 });
        assert_eq!(world.render(), r#"{"form":{"zeta":1,"alpha":2}}"#);
    }

    #[test]
    fn rendering_twice_is_byte_identical_and_the_hash_moves_with_the_world() {
        let build = || {
            World::new()
                .part("identity", json!({"name": "Cedar"}))
                .part("fresh", json!([{"report_key": "report_1"}]))
        };
        assert_eq!(build().render(), build().render());
        assert_eq!(build().hash(), build().hash());

        let base = build();
        // Adding, removing and reordering a part are all different worlds.
        let added = build().part("voice", "Clear.");
        let removed = World::new().part("fresh", json!([{"report_key": "report_1"}]));
        let reordered = World::new()
            .part("fresh", json!([{"report_key": "report_1"}]))
            .part("identity", json!({"name": "Cedar"}));
        let changed = World::new()
            .part("identity", json!({"name": "Vale"}))
            .part("fresh", json!([{"report_key": "report_1"}]));
        for other in [added, removed, reordered, changed] {
            assert_ne!(base.hash(), other.hash());
        }
    }

    #[test]
    fn each_plugin_pins_its_own_key_order() {
        // There is deliberately no single canonical order. These two already
        // differed, and `form.rs` records that field order changes SmolLM3's
        // output, so forcing one onto whichever had it wrong would be making a
        // plugin worse in order to share a renderer. The renderer guarantees
        // determinism; each plugin guarantees its own order, and this test is
        // what stops that order drifting unnoticed.
        let influencer = crate::plugins::influencer::cognition::Assignment {
            subject: crate::plugins::meta::EntityMeta {
                name: "Cedar".into(),
                entity_type: "team".into(),
                sport: "NBA".into(),
                entity_id: 7,
            },
            source: crate::plugins::harvester::delivery::SourceContext {
                classification_id: 1,
                article_id: 42,
                headline: "Fresh".into(),
                context: "Cedar won.".into(),
                source: "Wire".into(),
                published_at_epoch: Some(1_790_553_600),
            },
            history: vec![],
            input_components_json: String::new(),
            input_hash: String::new(),
        };
        let rendered: serde_json::Value = serde_json::from_str(
            &crate::plugins::influencer::cognition::assembled_prompt(&influencer),
        )
        .unwrap();
        // Parsed order is alphabetical, so the wire order is asserted on the
        // rendered string rather than on the map.
        let prompt = crate::plugins::influencer::cognition::assembled_prompt(&influencer);
        let wire = wire_order(&prompt);
        assert_eq!(wire, ["meta", "fresh", "voice", "form"]);
        assert!(rendered.get("memories").is_none());

        // The Journalist omits memories when no history was attached.
        let subject = crate::plugins::meta::EntityMeta {
            name: "Cedar".into(),
            entity_type: "team".into(),
            sport: "FOOTBALL".into(),
            entity_id: 7,
        };
        let journalist = crate::plugins::journalist::cognition::prepare(
            subject,
            vec![crate::plugins::journalist::cognition::CorpusItem {
                id: 1,
                title: String::new(),
                context: "Cedar won.".into(),
                source: "Wire".into(),
                published_at_epoch: Some(1_790_553_600),
            }],
            &crate::plugins::journalist::cognition::Continuity::default(),
            1_790_553_600,
        )
        .unwrap();
        let prompt = crate::plugins::journalist::cognition::prompt(&journalist);
        assert_eq!(wire_order(&prompt), ["meta", "fresh", "voice", "form"]);
    }

    /// The top-level key order of a rendered package.
    ///
    /// A scanner, not a parser: `serde_json` would hand back a sorted map, which
    /// is precisely the thing under test.
    fn wire_order(rendered: &str) -> Vec<String> {
        let bytes = rendered.as_bytes();
        let (mut depth, mut start, mut keys) = (0usize, None, Vec::new());
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'{' | b'[' => {
                    depth += 1;
                    if depth == 1 {
                        start = Some(index + 1);
                    }
                }
                b'}' | b']' => {
                    depth -= 1;
                    if depth == 0 {
                        let inner = &rendered[start.expect("object opened")..index];
                        for part in split_top_level(inner) {
                            if let Some((key, _)) = part.split_once(':') {
                                keys.push(key.trim().trim_matches('"').to_string());
                            }
                        }
                        return keys;
                    }
                }
                _ => {}
            }
            index += 1;
        }
        keys
    }

    /// Split an object's body on commas that are not inside a nested value.
    fn split_top_level(inner: &str) -> Vec<&str> {
        let mut parts = Vec::new();
        let (mut depth, mut start) = (0usize, 0usize);
        let bytes = inner.as_bytes();
        let mut in_string = false;
        let mut escaped = false;
        for (index, byte) in bytes.iter().enumerate() {
            match byte {
                b'"' if !escaped => in_string = !in_string,
                b'\\' if in_string => escaped = !escaped,
                _ => {
                    escaped = false;
                    match byte {
                        b'{' | b'[' if !in_string => depth += 1,
                        b'}' | b']' if !in_string => depth -= 1,
                        b',' if depth == 0 && !in_string => {
                            parts.push(&inner[start..index]);
                            start = index + 1;
                        }
                        _ => {}
                    }
                }
            }
        }
        parts.push(&inner[start..]);
        parts
            .into_iter()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect()
    }

    #[test]
    fn the_renderer_owns_no_policy() {
        // It renders what it is given: an empty world is valid, and a part it
        // has never heard of is passed through without interpretation.
        assert_eq!(World::new().render(), "{}");
        let world = World::new().part("anything_at_all", json!([1, 2, 3]));
        assert_eq!(world.render(), r#"{"anything_at_all":[1,2,3]}"#);
    }
}
