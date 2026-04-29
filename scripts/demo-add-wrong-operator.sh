#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/fixtures/rust/add_wrong_operator"
TMP="$(mktemp -d /tmp/patchwright-add-wrong-operator.XXXXXX)"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo was not found in this Bash environment." >&2
  echo "On Windows, run the PowerShell demo instead:" >&2
  echo "  powershell -ExecutionPolicy Bypass -File scripts/demo-add-wrong-operator.ps1" >&2
  exit 127
fi

cp -R "$SRC/." "$TMP/"

git -C "$TMP" init -q
git -C "$TMP" config user.email "patchwright@example.invalid"
git -C "$TMP" config user.name "Patchwright Demo"
git -C "$TMP" add .
git -C "$TMP" commit -qm "seed broken add fixture"

echo "Fixture repo: $TMP"
echo
echo "Before Patchwright:"
cargo test --manifest-path "$TMP/Cargo.toml" || true

echo
echo "Running Patchwright:"
cargo run -p patchwright-cli -- solve \
  --repo "$TMP" \
  --task "$(cat "$TMP/TASK.md")" \
  --model-provider codex-cli \
  --max-steps 12

echo
echo "After Patchwright:"
cargo test --manifest-path "$TMP/Cargo.toml"
