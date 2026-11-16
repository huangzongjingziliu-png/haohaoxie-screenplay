#!/bin/bash
# Local sandbox experiment only; ad-hoc signed and not a submission package.
set -euo pipefail
cd "$(dirname "$0")/.."
source_app='target/release/bundle/macos/好好写剧本.app'
prototype='target/sandbox-prototype/好好写剧本沙盒验证.app'
[ -d "$source_app" ] || { echo '先执行 npm run tauri:build'; exit 1; }
mkdir -p target/sandbox-prototype
ditto "$source_app" "$prototype"
plist="$prototype/Contents/Info.plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleIdentifier com.screenplay.studio.sandboxprobe' "$plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleName 好好写剧本沙盒验证' "$plist"
/usr/libexec/PlistBuddy -c 'Set :CFBundleDisplayName 好好写剧本沙盒验证' "$plist"
resources="$prototype/Contents/Resources/resources"
for helper in "$resources/ocr_sidecar" "$resources/pdf_sidecar" "$resources/agent/screenplay-bin"; do
 codesign --force --sign - --entitlements packaging/app-store/helper.entitlements.plist "$helper"
done
codesign --force --sign - --entitlements packaging/app-store/app.entitlements.plist "$prototype"
codesign --verify --deep --strict "$prototype"
codesign -d --entitlements - "$prototype"
echo "验证版：$prototype"
