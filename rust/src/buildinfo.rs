//! Buildinfo — the compile-time-pinned commit + build-time, stamped by
//! [`build.rs`](../../build.rs) via `cargo:rustc-env`. The Rust analog of Go's
//! `internal/buildinfo` LDFLAGS (`release.sh` sets `-X …Commit=…` / `-X …BuildTime=…`).
//!
//! Falls back to `"unknown"` when the crate was built outside a git work-tree (e.g.
//! `cargo install` from a tarball), so a stripped-source build never fails nor lies — the
//! "what's deployed" check just reports `unknown` instead of a SHA.
//!
//! The daemon logs both at boot (see `main.rs`); the only fields Go exposes that this
//! mirrors are `Commit` and `BuildTime`. Read via [`COMMIT`] / [`BUILD_TIME`].
//!
//! NB: a debug-cycle incremental build re-runs `build.rs` (its `rerun-if-changed=build.rs`),
//! so the build-time advances on every touch — exactly what distinguishes a fresh build
//! from a stale one. The Go `-dirty` suffix is intentionally not reproduced here: the
//! build-time alone is enough of a freshness marker, and the Go `-dirty` exists primarily
//! because a Go binary can be hand-built outside `release.sh` with no other signal.

/// The short (12-char) SHA the binary was built from, or `unknown` outside a work-tree.
pub const COMMIT: &str = match option_env!("SCORACLE_BUILD_COMMIT") {
    Some(s) => s,
    None => "unknown",
};

/// The UTC ISO-8601 build timestamp (`YYYY-MM-DDTHH:MM:SSZ`), or `unknown` outside a work-tree.
pub const BUILD_TIME: &str = match option_env!("SCORACLE_BUILD_TIME") {
    Some(s) => s,
    None => "unknown",
};
