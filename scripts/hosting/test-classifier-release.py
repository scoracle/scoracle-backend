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
    for name in ("release.sh", "install.sh", "verify-classifier.sh", "run-classifier-model.sh"):
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
  *is-active*scoracle-coverage.service*) exit "${COVERAGE_STATE:-1}" ;;
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
    assert not (root / "units/scoracle-classifier-source.service").exists()
    coverage_unit = (root / "units/scoracle-coverage.service").read_text()
    assert str(root / "go/bin/pipeline") + " -mode monitor" in coverage_unit
    assert "__SCORACLE_REPO_ROOT__" not in coverage_unit
    assert "start scoracle-coverage" not in calls and "restart scoracle-coverage" not in calls
    assert len(list((root / "bins").iterdir())) == 7
    calls = run([], 0, {"COVERAGE_STATE": "0"})
    assert "restart scoracle-coverage.service" in calls and "--quiet is-active scoracle-coverage.service" in calls
    calls = run(["--build-only"], 0, {"COVERAGE_STATE": "0"})
    assert "restart scoracle-coverage.service" not in calls
    for overrides, code in [({"COGNITION_STATE": "0"}, 1), ({"WATCHER_STATE": "0"}, 1),
                            ({"FAIL_SCHEMA": "3"}, 3), ({"FAIL_BUILD": "2"}, 2)]:
        calls = run(["--keep-cognition-paused"], code, overrides)
        assert not list((root / "bins").iterdir()) and "restart " not in calls
    calls = run([], 0, {"COGNITION_STATE": "0"})
    assert "restart scoracle-cognition.service" in calls
    calls = run(["--build-only"], 0)
    assert not calls and not (root / "calls").read_text()
    native = root / ".local/bin/scoracle-llama-server"
    native.parent.mkdir(parents=True)
    native.write_text('#!/usr/bin/env bash\nprintf "%s\\n" "$@"\n')
    native.chmod(0o755)
    for settings, model, name in [
        ({}, str(root / "models/Kev-0.8B-Q8_0.gguf"), "ggml-org/Kev-0.8B-GGUF"),
        ({"CLASSIFIER_MODEL_FILE": "/models/new model.gguf", "CLASSIFIER_MODEL_NAME": "LiquidAI/d1-3B-GGUF", "CLASSIFIER_CONTEXT_SIZE":"32768", "CLASSIFIER_PARALLEL":"4"},
         "/models/new model.gguf", "LiquidAI/d1-3B-GGUF"),
    ]:
        clean_env = {k: v for k, v in env.items() if k not in ("CLASSIFIER_MODEL_FILE", "CLASSIFIER_MODEL_NAME", "CLASSIFIER_CONTEXT_SIZE", "CLASSIFIER_PARALLEL")}
        result = subprocess.run(["bash", str(root / "scripts/hosting/run-classifier-model.sh")],
                                env={**clean_env, "HOME": str(root), **settings}, capture_output=True, text=True, check=True)
        args = result.stdout.splitlines()
        assert args[:4] == ["-m", model, "--alias", name + "@sha256:fixturehash"]
        assert args[args.index("--ctx-size")+1] == settings.get("CLASSIFIER_CONTEXT_SIZE","65536")
        assert args[args.index("--parallel")+1] == settings.get("CLASSIFIER_PARALLEL","8")
    assert "EnvironmentFile=-%h/.config/scoracle/classifier-model.env" in (root / "units/scoracle-kev.service").read_text()
print("Model swap/default launcher, paused release, preflight/build failure, active restart and build-only checks pass.")
