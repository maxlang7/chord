#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
git submodule update --init vendor/pickle vendor/ai-bridge
patch_file="$PWD/patches/pickle-local-history.patch"
if git -C vendor/pickle apply --check "$patch_file" 2>/dev/null; then
  git -C vendor/pickle apply "$patch_file"
elif ! git -C vendor/pickle apply --reverse --check "$patch_file" 2>/dev/null; then
  echo 'Pickle has conflicting local edits. Review them before applying the Chord patch.' >&2
  exit 1
fi
npm ci
(
  cd vendor/pickle
  pnpm install --frozen-lockfile
  pnpm --filter @beeper/pickle --filter @beeper/pickle-state-file build
)
cargo build --release --locked
printf '%s\n' 'Built Chord. Launch with ./scripts/launch.sh.'
