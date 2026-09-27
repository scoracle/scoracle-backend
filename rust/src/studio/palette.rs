//! Plugin-owned output material. The model chooses expression only from this finite palette.
//! A model response is never served as prose: Studio resolves its choices against the
//! plugin's prepared statements. One choice per positional fact slot makes omission
//! and duplication impossible even when a model cannot track identifiers reliably.

use crate::studio::Parser;
use anyhow::{ensure, Result};
use serde::Deserialize;
use std::collections::HashSet;

/// Output reservation for the compact positional choice object.
pub const PALETTE_NUM_PREDICT: i32 = 160;

#[derive(Clone, Debug)]
pub struct Paint {
    pub id: String,
    /// Equivalent, plugin-approved ways to state the same fact.
    pub phrasings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Palette {
    pub paints: Vec<Paint>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Composition {
    pub choices: Vec<usize>,
}

impl Palette {
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.paints.is_empty(), "palette has no statements");
        let mut ids = HashSet::new();
        for paint in &self.paints {
            ensure!(
                !paint.id.is_empty() && ids.insert(&paint.id),
                "duplicate or empty paint id"
            );
            ensure!(!paint.phrasings.is_empty(), "paint has no phrasing");
            ensure!(
                paint.phrasings.iter().all(|s| !s.trim().is_empty()),
                "empty phrasing"
            );
        }
        Ok(())
    }

    pub fn prompt(&self) -> String {
        let mut prompt = String::from("Choose one approved phrasing for each fact in order. Return only JSON with a choices array of zero-based phrasing indexes. The array must have one choice for each numbered fact.\n");
        for (slot, paint) in self.paints.iter().enumerate() {
            prompt.push_str(&format!("\nFact {slot} ({}):\n", paint.id));
            for (index, phrasing) in paint.phrasings.iter().enumerate() {
                prompt.push_str(&format!("  {index}: {phrasing}\n"));
            }
        }
        prompt
    }

    pub fn schema(&self) -> serde_json::Value {
        // A single items schema applies to every positional slot. Bound it to
        // the choices shared by all slots so constrained decoding cannot emit
        // an index that one of the prepared facts cannot render.
        let highest_common_choice = self
            .paints
            .iter()
            .map(|paint| paint.phrasings.len())
            .min()
            .unwrap_or(1)
            .saturating_sub(1);
        serde_json::json!({
            "type": "object",
            "properties": {"choices": {"type": "array", "minItems": self.paints.len(), "maxItems": self.paints.len(),
                "items": {"type": "integer", "minimum": 0, "maximum": highest_common_choice}}},
            "required": ["choices"], "additionalProperties": false
        })
    }

    pub fn render(&self, composition: &Composition) -> Result<String> {
        self.validate()?;
        ensure!(
            composition.choices.len() == self.paints.len(),
            "composition omitted facts"
        );
        let mut sentences = Vec::with_capacity(self.paints.len());
        for (paint, choice) in self.paints.iter().zip(&composition.choices) {
            let phrasing = paint
                .phrasings
                .get(*choice)
                .ok_or_else(|| anyhow::anyhow!("composition used an unknown phrasing"))?;
            sentences.push(phrasing.as_str());
        }
        Ok(sentences.join(" "))
    }
}

pub struct PaletteParser<'a>(pub &'a Palette);

impl Parser<String> for PaletteParser<'_> {
    fn parse(&self, raw: &str) -> Result<Option<String>> {
        let composition: Composition = serde_json::from_str(raw)?;
        Ok(Some(self.0.render(&composition)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_cannot_add_omit_or_repeat_a_fact() {
        let palette = Palette {
            paints: vec![
                Paint {
                    id: "a".into(),
                    phrasings: vec!["A is strong.".into()],
                },
                Paint {
                    id: "b".into(),
                    phrasings: vec!["B is poor.".into()],
                },
            ],
        };
        assert_eq!(
            palette
                .render(&Composition {
                    choices: vec![0, 0]
                })
                .unwrap(),
            "A is strong. B is poor."
        );
        assert!(palette.render(&Composition { choices: vec![0] }).is_err());
        assert!(palette
            .render(&Composition {
                choices: vec![9, 0]
            })
            .is_err());
        assert!(PaletteParser(&palette)
            .parse(r#"{"choices":[0,0],"made_up":"42 goals"}"#)
            .is_err());
    }

    #[test]
    fn schema_bounds_every_choice_to_a_renderable_phrasing() {
        let palette = Palette {
            paints: vec![
                Paint {
                    id: "a".into(),
                    phrasings: vec!["First.".into(), "Second.".into(), "Third.".into()],
                },
                Paint {
                    id: "b".into(),
                    phrasings: vec!["One.".into(), "Two.".into()],
                },
            ],
        };
        palette.validate().unwrap();
        assert_eq!(
            palette.schema()["properties"]["choices"]["items"]["maximum"],
            1
        );
        assert!(palette
            .render(&Composition {
                choices: vec![2, 1]
            })
            .is_ok());
        assert!(palette
            .render(&Composition {
                choices: vec![1, 2]
            })
            .is_err());
    }
}
