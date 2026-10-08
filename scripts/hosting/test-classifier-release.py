#!/usr/bin/env python3
"""Run real release/install scripts in a disposable tree with fake external commands."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

SOURCE = Path(__file__).resolve().parents[2]
with tempfile.TemporaryDirectory(prefix="classifier-release-check-") as directory:
    root = Path(directory)
    for name in ("scripts/hosting", "scripts/systemd", "go", "rust/target/debug", "bins", "units", "mock"):
        (root / name).mkdir(parents=True)
    for name in ("release.sh", "install.sh", "verify-classifier.sh"):
        shutil.copy2(SOURCE / "scripts/hosting" / name, root / "scripts/hosting" / name)
    for source in (SOURCE / "scripts/systemd").glob("*"):
        if source.is_file(): shutil.copy2(source, root / "scripts/systemd" / source.name)
    for name in ("scoracle-cognition", "statcommentary", "factsweep"):
        (root / "rust/target/debug" / name).write_text("fixture binary")
    commands = {
        "cargo": 'exit "${FAIL_BUILD:-0}"',
        "go": 'while [ "$1" != -o ]; do shift; done; shift; printf binary > "$1"',
        "git": 'case "$*" in *rev-parse*) echo fixturecommit ;; esac',
        "psql": 'cat >/dev/null; exit "${FAIL_SCHEMA:-0}"',
        "curl": """case "$*" in *health/db*) printf 200 ;; *) printf '{"commit":"fixturecommit"}' ;; esac""",
        "sha256sum": 'printf "fixturehash  %s\\n" "$1"',
        "systemctl": '''echo "$*" >> "$CALL_LOG"
case "$*" in
  *is-active*scoracle-cognition.service*) exit "${COGNITION_STATE:-1}" ;;
  *is-active*scoracle-cognition.path*) exit "${WATCHER_STATE:-1}" ;;
  *is-active*scoracle-classifier-source.service*) exit "${SOURCE_STATE:-1}" ;;
  *is-active*scoracle-api.path*) exit 1 ;;
esac''',
    }
    for name, body in commands.items():
        path = root / "mock" / name
        path.write_text("#!/usr/bin/env bash\n" + body + "\n")
        path.chmod(0o755)
    env = {**os.environ, "PATH": str(root / "mock") + ":" + str(Path.home() / ".cargo/bin") + ":" + os.environ["PATH"],
           "RELEASE_BIN_DIR": str(root / "bins"), "SCORACLE_SYSTEMD_DIR": str(root / "units"),
           "DATABASE_PRIVATE_URL": "fixture", "CALL_LOG": str(root / "calls")}
    def run(extra, expected, overrides=None):
        for path in (root / "bins").iterdir(): path.unlink()
        (root / "calls").write_text("")
        result = subprocess.run(["bash", str(root / "scripts/hosting/release.sh"), *extra],
                                env={**env, **(overrides or {})}, capture_output=True, text=True)
        assert result.returncode == expected, result.stdout + result.stderr
        return (root / "calls").read_text()
    calls = run(["--keep-cognition-paused"], 0)
    assert "restart scoracle-api.service" in calls
    assert "restart scoracle-cognition.service" not in calls and "start scoracle-cognition.path" not in calls
    assert "restart scoracle-classifier-source.service" not in calls
    source_unit = (root / "units/scoracle-classifier-source.service").read_text()
    assert "/usr/bin/env COGNITION_STAGES=classifier_acquire" in source_unit
    assert "__SCORACLE_REPO_ROOT__" not in source_unit
    assert len(list((root / "bins").iterdir())) == 7
    calls = run(["--keep-cognition-paused"], 0, {"SOURCE_STATE": "0"})
    assert "restart scoracle-classifier-source.service" in calls and "restart scoracle-cognition.service" not in calls
    for overrides, code in [({"COGNITION_STATE": "0"}, 1), ({"WATCHER_STATE": "0"}, 1),
                            ({"FAIL_SCHEMA": "3"}, 3), ({"FAIL_BUILD": "2"}, 2)]:
        calls = run(["--keep-cognition-paused"], code, overrides)
        assert not list((root / "bins").iterdir()) and "restart " not in calls
    calls = run([], 0, {"COGNITION_STATE": "0"})
    assert "restart scoracle-cognition.service" in calls
    calls = run(["--build-only"], 0)
    assert not calls and not (root / "calls").read_text()
print("Paused/source-only release, preflight/build failure, active restart and build-only checks pass.")
