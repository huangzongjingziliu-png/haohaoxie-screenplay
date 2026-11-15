# 决策：商店版只保留应用内 HTTP MCP，stdio 由站外 shim 满足

日期：2026-11-15　分支：`feat/0.1.58-mcp-http`

## 决策

1. App Store 包内只提供**应用内本地 Streamable HTTP MCP 服务**（只绑 127.0.0.1，带 bearer token），不再把 stdio 桥放进商店包。
2. 需要 stdio 的客户端，用**站外分发**的 `screenplay-mcp-shim` 满足：由用户单独安装、单独配置到客户端，不随商店包提交。
3. `screenplay-mcp-shim` 在仓库里只作为站外分发的源码与实验产物存在，**不是商店交付物**。

## 依据

- MAS 要求包内可执行代码在沙盒内运行。包内 helper 只有两条沙盒路径：`com.apple.security.inherit`（必须由沙盒父进程启动）或独立容器。外部 MCP 客户端（Claude Code、Codex 等）从终端拉起时两者都拿不到。
- 实测（2026-11-15，本机 ad-hoc 签名）：

  | 组合 | 运行方式 | 结果 |
  |---|---|---|
  | app-sandbox | `/tmp` 直接执行 | 退出码 133（SIGTRAP），无输出 |
  | app-sandbox | 包内 `Contents/MacOS` 直接执行 | 133 |
  | app-sandbox + inherit | `/tmp` 直接执行 | 133 |

  对照：`/bin/echo` 用同一份 entitlements 重签后同样 133；不签沙盒的 `/bin/echo` 正常。说明终止原因是沙盒上下文，不是 shim 代码。
- 反向验证：不带沙盒签名的 shim 放进包内 `Contents/MacOS`，由外部进程拉起可完整跑通 `initialize → tools/list(39) → new_project → insert_block → replace_block → save_project`，落盘文件与 GUI 均确认。这恰好是商店审核不接受的一种形态（包内未沙盒可执行文件）。

## 限定

以上是本机签名校验与沙盒行为，**不代表 App Review 的判定**。真实审核还会看团队证书、hardened runtime、全部可执行代码是否沙盒化，以及对「桥接进程由外部客户端拉起」这一形态的理解。不要把本记录当作审核结论。

## CLI 同样归站外分发（2026-11-15 追加）

1. 商店版**不提供**「安装终端命令」入口，也不向 `~/.local/bin` 写链接：沙盒应用没有容器外写权限，这个入口在商店包里只会失败。
2. 商店包**不携带** agent CLI 二进制（`resources/agent/*` 从商店构建的资源清单里移除）。`screenplay` CLI 与 `screenplay-mcp-shim` 一并归站外分发渠道，需要各自的签名与安装说明。
3. 商店构建（`--features app-store`）里 `install_agent_cli` 直接返回不可用错误；连接面板的 CLI 入口在商店渠道隐藏，`docs/APP_STORE_PLAN.md` 里「CLI 是保留目标」的说法同时作废。

## 影响

- `qa/health/20261115-0.1.58-mcp-http-protocol-acceptance.md` 里的协议级验收对应方案 1（应用内 HTTP），仍然有效；客户端级验收仍待补跑。
- 站外分发的 shim 需要独立签名与安装说明，不能依赖商店包的签名身份。
- 之前「MCP 已移除、CLI 是保留目标」的说法作废，见 `docs/APP_STORE_PLAN.md`。
- 本决策的落地形式：`src-tauri/tauri.store.conf.json`（独立 bundle id、无更新产物、沙盒 entitlement、隐私清单）+ `scripts/build-store.sh`；构建渠道开关是 Cargo feature `app-store`。
