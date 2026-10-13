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
codesign --verify --deep --strict --verbose=2 "target/release/bundle/macos/好好写剧本.app"
app_version=$(node -p "require('./package.json').version")
mount_dir=$(mktemp -d /tmp/haohaoxie-dmg-verify.XXXXXX)
trap 'hdiutil detach "$mount_dir" >/dev/null 2>&1 || true; rmdir "$mount_dir" 2>/dev/null || true' EXIT
hdiutil attach -readonly -nobrowse -mountpoint "$mount_dir" "target/release/bundle/dmg/好好写剧本_${app_version}_aarch64.dmg" >/dev/null
codesign --verify --deep --strict --verbose=2 "$mount_dir/好好写剧本.app"
hdiutil detach "$mount_dir" >/dev/null
rmdir "$mount_dir"
trap - EXIT
python3 scripts/prepare-update-manifest.py "$@"
