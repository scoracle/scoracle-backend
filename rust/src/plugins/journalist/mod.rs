//! Journalist plugin: preparation, cognition, publication, and its manifest.

pub mod adapter;
pub mod cognition;
pub mod manifest;
// Public because `cognition::Parts` is: a stored fixture holds this plugin's
// parts, and the selection a caller reads is a named part of the world, not an
// internal detail of preparation.
pub mod memories;
