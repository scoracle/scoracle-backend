// build.rs — stamp commit + build-time into the binary so the Rust daemon can report what's
// deployed (the Go side gets the equivalent via LDFLAGS at `go build`; this is the Rust analog).
//
// Mirrors release.sh's stamping semantics: `git rev-parse --short=12` + a UTC ISO-8601 build
// time. Pinned via `cargo:rustc-env=…` so `env!("SCORACLE_BUILD_COMMIT")` / `env!(
// "SCORACLE_BUILD_TIME")` in `src/buildinfo.rs` resolve at compile time. Falls back to
// "unknown" when git is absent or the crate is built outside a work-tree (e.g. `cargo
// install` from a tarball), so the build never fails on a stripped source tree.
//
// The `-dirty` suffix release.sh adds on a dirty Go tree is intentionally NOT reproduced
// here; the Go side's `-dirty` exists because the Go binary may be hand-built outside
// release.sh and the marker is the only signal. release.sh is authoritative for the
// Rust binaries too.
//
// KNOW THIS BEFORE TRUSTING THE STAMP. Emitting any `cargo:rerun-if-changed` REPLACES
// cargo's default "re-run when any package file changed", so this script re-runs ONLY on
// the paths listed below — build.rs itself and git HEAD movement. Editing source without
// committing therefore produces a fresh BINARY carrying a STALE commit + build-time.
// Measured live on 2026-07-26: the ar6 binary booted reporting the ar5 commit, because it
// was built before the commit landed. The stamp means "HEAD when this was compiled", not
// "what code is inside". To make a deploy self-describing, commit first, then build.

use std::process::Command;

fn main() {
    // Re-run on build script changes and git HEAD movement so release logs report
    // the commit that was actually deployed.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    if let Some(head_ref) = git_head_ref() {
        println!("cargo:rerun-if-changed=../.git/{head_ref}");
    }

    let commit = git_short()
        .unwrap_or_else(|| "unknown".to_string())
        .trim()
        .to_string();
    let build_time = utc_now_iso8601();

    // Pin to the compile-time env so `env!` in buildinfo.rs picks them up.
    println!("cargo:rustc-env=SCORACLE_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=SCORACLE_BUILD_TIME={build_time}");
}

fn git_head_ref() -> Option<String> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")?;
    let head =
        std::fs::read_to_string(std::path::Path::new(&manifest).join("../.git/HEAD")).ok()?;
    let head = head.trim();
    head.strip_prefix("ref: ").map(|s| s.to_string())
}

/// git_short shells out to `git rev-parse --short=12 HEAD` against the crate's parent dir
/// (the repo root). Returns the trimmed short SHA, or None when git is unavailable / not a
/// repo. Uses the `GIT_DIR` env var when set (a cargo workspace may set it to point at the
/// workspace .git), else falls back to the standard `cd-to-crate-root` flow.
fn git_short() -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.arg("rev-parse").arg("--short=12").arg("HEAD");
    // Run from the crate root (the dir containing this build.rs) — `git rev-parse` looks
    // up the nearest .git from the cwd, which is cargo's invocation CWD by default. Belt
    // and braces: set it explicitly to CARGO_MANIFEST_DIR so a workspace build still
    // resolves to the surrounding repo, not wherever cargo was invoked from.
    if let Some(manifest) = std::env::var_os("CARGO_MANIFEST_DIR") {
        cmd.current_dir(manifest);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

/// utc_now_iso8601 returns `YYYY-MM-DDTHH:MM:SSZ` — the same shape Go's BUILD_TIME uses
/// (release.sh: `date -u +%Y-%m-%dT%H:%M:%SZ`). std only; no chrono dep.
fn utc_now_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = civil_from_unix(secs);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// civil_from_unix converts a Unix-second count to a UTC (Y, M, D, h, m, s) tuple.
/// Howard Hinnant's date algorithm (public domain) — the std-only date computation Rust
/// authors reach for when they don't want to pull chrono into a build script.
fn civil_from_unix(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let h = (rem / 3600) as u32;
    let mi = ((rem % 3600) / 60) as u32;
    let s = (rem % 60) as u32;
    // days since 1970-01-01 → civil Y-M-D. Hinnant's algorithm.
    let z = days + 719_468; // shift to 0000-03-01 epoch
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = (if m <= 2 { y + 1 } else { y }) as i32;
    (y, m, d, h, mi, s)
}
