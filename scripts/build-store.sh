#!/bin/bash
# 商店渠道本地构建：独立 bundle id + 沙盒 entitlement + 隐私清单，ad-hoc 签名。
# 产物仅用于本机安装验收，不是提交包（正式签名见 docs/APP_STORE_PLAN.md 阶段 4）。
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh
export no_proxy='*' NO_PROXY='*'

# StoreKit 走进程内 framework，必须先构建再打包。
packaging/iap/build-framework.sh

npx tauri build --config src-tauri/tauri.store.conf.json --features app-store

app='target/release/bundle/macos/好好写剧本.app'
[ -d "$app" ] || { echo "未找到构建产物：$app"; exit 1; }

# 沙盒子进程必须继承父进程沙盒；Tauri 只签主程序，Resources 里的 sidecar 需要单独签。
resources="$app/Contents/Resources/resources"
for helper in "$resources/ocr_sidecar" "$resources/pdf_sidecar"; do
  [ -f "$helper" ] || continue
  codesign --force --sign - --entitlements packaging/app-store/helper.entitlements.plist "$helper"
done
codesign --force --sign - --entitlements packaging/app-store/app.entitlements.plist "$app"
codesign --verify --deep --strict "$app"
echo '--- app entitlements ---'
codesign -d --entitlements - "$app" 2>/dev/null
echo "商店验收包：$app"
