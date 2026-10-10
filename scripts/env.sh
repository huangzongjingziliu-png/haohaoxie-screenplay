#!/usr/bin/env bash
# 统一构建环境（source 使用，不要直接执行）。
#
# 为什么需要这个文件：
#   本机 /opt/homebrew/bin 里有一个旧的 Homebrew cargo 1.76，
#   而本项目 Cargo.lock 是 lockfile v4（需要 cargo >= 1.78）。
#   Homebrew 的 /opt/homebrew/bin 在 PATH 中排在 ~/.cargo/bin 之前，
#   于是 shell 默认拿到 1.76，报错：
#     "lock file version `4` was found, but this version of Cargo does not
#      understand this lock file"
#   这个失败发生在解析阶段，很容易被误判成项目代码问题。
#
# 用法：
#   source scripts/env.sh && cargo test --workspace

export PATH="$HOME/.cargo/bin:$PATH"
export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8

# 确认拿到的是 rustup 工具链而不是 Homebrew 的旧版本
if ! cargo --version 2>/dev/null | grep -q "cargo 1\.9\|cargo 1\.8[0-9]"; then
  echo "警告：未检测到预期的 rustup 工具链（当前 $(cargo --version 2>&1)）" >&2
fi