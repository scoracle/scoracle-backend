#!/usr/bin/env bash
# SessionStart hook — surface a stale local baseline before any editing.
# Fetches origin and warns ONLY when out of sync. Never blocks (always exit 0).
set -u
repo="${CLAUDE_PROJECT_DIR:-$PWD}"

git -C "$repo" rev-parse --is-inside-work-tree >/dev/null 2>&1 || exit 0
git -C "$repo" fetch --quiet 2>/dev/null

local=$(git -C "$repo" rev-parse @ 2>/dev/null)
remote=$(git -C "$repo" rev-parse '@{u}' 2>/dev/null)
base=$(git -C "$repo" merge-base @ '@{u}' 2>/dev/null)
branch=$(git -C "$repo" rev-parse --abbrev-ref HEAD 2>/dev/null)

[ -z "$remote" ] && exit 0   # no upstream configured — nothing to compare

if [ "$local" != "$remote" ]; then
  if [ "$local" = "$base" ]; then
    echo "⚠️  git[$branch]: BEHIND origin — run 'git pull --ff-only' before editing."
  elif [ "$remote" = "$base" ]; then
    echo "⚠️  git[$branch]: AHEAD of origin (unpushed local commits)."
  else
    echo "⚠️  git[$branch]: DIVERGED from origin — reconcile before editing."
  fi
fi
exit 0
