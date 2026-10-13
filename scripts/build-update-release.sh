#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh
key_path="${TAURI_SIGNING_PRIVATE_KEY:-$HOME/Library/Application Support/ScreenplayStudio/update-signing/private.key}"
if [[ ! -f "$key_path" ]]; then
  echo 'Missing publisher update signing key. See docs/UPDATES.md.' >&2
  exit 1
fi
export TAURI_SIGNING_PRIVATE_KEY="$key_path"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"
npm run tauri:build
python3 scripts/prepare-update-manifest.py "$@"
