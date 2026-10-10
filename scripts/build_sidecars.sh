#!/usr/bin/env bash
#
# 构建 macOS sidecar（Swift），并复制到 Cargo 输出目录。
#
# 为什么要这一步：
#   Rust 侧通过 `current_exe()` 的同级目录找 sidecar，这样打包后
#   （sidecar 在 .app/Contents/Resources/）与测试时（target/debug/）都能找到。
#   但 `cargo build` 本身不会编译 Swift，必须显式执行本脚本。
#
# 用法：
#   scripts/build_sidecars.sh              # 构建并复制到 target/{debug,release}
#   scripts/build_sidecars.sh --release    # 只构建 release
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="debug"
PROFILE_DIR="debug"
if [[ "${1:-}" == "--release" ]]; then
  PROFILE="release"
  PROFILE_DIR="release"
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "[sidecars] 非 macOS，跳过"
  exit 0
fi

if ! command -v swiftc >/dev/null 2>&1; then
  echo "[sidecars] 未找到 swiftc，跳过" >&2
  exit 0
fi

# 名称 → Swift 源文件相对路径
SIDECARS=(
  "ocr_sidecar:crates/screenplay-import/swift/ocr_sidecar.swift"
  "pdf_sidecar:crates/screenplay-export/swift/pdf_sidecar.swift"
)

build_one() {
  local name="$1"
  local src_rel="$2"
  local src="$ROOT/$src_rel"
  if [[ ! -f "$src" ]]; then
    echo "[sidecars] 跳过 $name（缺少 $src_rel）"
    return 0
  fi
  # 输出到源文件同目录的 swift/ 下（src_rel 已含 swift/xxx.swift）
  local out="$ROOT/${src_rel%.swift}"
  echo "[sidecars] 编译 $name ..."
  swiftc -O -o "$out" "$src"
  # 复制到 Cargo 输出目录，供 current_exe() 查找
  for p in debug release; do
    local dest_dir="$ROOT/target/$p"
    if [[ -d "$dest_dir" ]]; then
      cp -f "$out" "$dest_dir/$name"
    fi
  done
  mkdir -p "$ROOT/src-tauri/resources"
  cp -f "$out" "$ROOT/src-tauri/resources/$name"
  echo "[sidecars] $name -> $out"
}

for entry in "${SIDECARS[@]}"; do
  build_one "${entry%%:*}" "${entry#*:}"
done

echo "[sidecars] 完成"
