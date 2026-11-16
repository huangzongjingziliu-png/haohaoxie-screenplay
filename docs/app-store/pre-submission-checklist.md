# 提交前 Checklist（代码项 / 外部项分开）

更新日期：2026-11-16。勾选前先看"验证方式"，不要凭印象打勾。

## A. 代码项（能在本机做完）

| # | 事项 | 现状 | 验证方式 |
| --- | --- | --- | --- |
| A1 | 清理 `PrivacyInfo.xcprivacy` 里的 TODO 注释，按实际调用裁剪原因码 | 待办（现有 2 个原因码 + 1 条 TODO） | 按 `app-privacy-questionnaire.md` 第四节跑命令，`plutil -p` 看最终包内清单 |
| A2 | 判定并落实 UserDefaults / 磁盘空间 / 键盘监听是否申报 | 建议均不申报，待复核 | `nm -u` 查最终二进制与 sidecar，无命中则留空并记录输出 |
| A3 | 默认模型地址 `https://apihub.agnes-ai.com/v1` 的处理 | **需决策**：清空默认值或改为 `http://127.0.0.1` 占位，避免"预置服务商"质疑 | 改 `src-tauri/src/agent_config.rs` 默认值后重跑商店构建，确认设置页空配置可用 |
| A4 | 正式签名替换 ad-hoc | 当前 `tauri.store.conf.json` 为 `signingIdentity: "-"` | 改为 Apple Distribution 身份 + 对应 provisioning profile，`codesign --verify --deep --strict` 通过 |
| A5 | 嵌套 helper（`pdf_sidecar`、`ocr_sidecar`）签名与 entitlements | 待验证 | 对包内每个 Mach-O 逐个 `codesign -dv`、`codesign --verify`，确认无 `--deep` 掩盖的漏签 |
| A6 | 沙盒 entitlement 复核（当前仅 `com.apple.security.network.server`，供 MCP 监听） | 已最小化 | `codesign -d --entitlements :-` 对比 `packaging/app-store/app.entitlements.plist` |
| A7 | 商店构建隐藏 CLI / 自动更新入口 | 已做（0.1.58 起） | 商店包实测：菜单无"安装终端命令"、无"检查更新" |
| A8 | 应用内支持与隐私链接指向真实页面 | 占位 | 打开菜单逐项点击，确认可打开且不是 404 |
| A9 | 版本号与构建号 | `0.1.59`；建议商店首版 `1.0.0` / build `1` | `plutil -p Info.plist` 看 `CFBundleShortVersionString`、`CFBundleVersion`（纯整数递增） |
| A10 | 产品名是否去掉"商店版"后缀 | 待决策 | 改 `tauri.store.conf.json` 的 `productName` 后重建并验收 |
| A11 | 首次启动文件授权（安全书签）流程回归 | 0.1.58/0.1.59 已验收 | 沙盒构建下新建→保存到桌面→重启→最近项目重新打开→再保存 |
| A12 | 无计费、无账号、无广告 SDK 的最终确认 | 已核实无依赖 | `grep -rn "sentry\|posthog\|firebase\|amplitude"`，查 `package.json` / `Cargo.toml` |
| A13 | 测试残留清理（容器内测试工程、最近项目、令牌） | 每轮验收后清理 | 删除容器 Data 下测试工程与最近项目条目，保留真实用户文件 |
| A14 | 导出/分集等文案与商店构建能力一致 | 待逐条对照 | 按 `store-listing.md` 长描述逐项在商店包点一遍 |
| A15 | 图标与启动图（含 1024×1024） | 已有应用图标 | 确认商店包 `Assets.car` / `AppIcon.icns` 存在且无透明异形问题 |

## B. 外部项（需要账号、证书或后台操作）

| # | 事项 | 状态 | 说明 |
| --- | --- | --- | --- |
| B1 | 注册 Apple Developer Program | **未开通**（本机 0 个有效签名身份） | 组织账号才显示公司名；个人账号显示法定姓名 |
| B2 | 创建 Apple Distribution 证书与 App Store provisioning profile | 未开始 | 依赖 B1；不要把私钥/验证码发到聊天 |
| B3 | 在 ASC 注册 Bundle ID `com.screenplay.studio.store` | 未开始 | 见 `bundle-id-and-sku.md` |
| B4 | 创建应用记录并填 SKU `haohaoxie-screenplay-macos` | 未开始 | SKU 创建后不可改 |
| B5 | 托管隐私政策页与支持页 | 未开始 | 用 `privacy-policy-draft.md` 定稿后发布，需可公开访问 |
| B6 | 填写 App Privacy 问卷 | 未开始 | 按 `app-privacy-questionnaire.md` 填，保留每项依据 |
| B7 | 填写年龄分级问卷 | 未开始 | 按 `age-rating-and-ai-content.md`，建议 12+ |
| B8 | 商店文案、关键词、截图上传 | 未开始 | 用 `store-listing.md`、`screenshots-plan.md` |
| B9 | 出口合规（加密）申报 | 未开始 | 应用仅用系统 HTTPS，通常属豁免项；按 ASC 当前问题作答 |
| B10 | 审核备注与演示剧本 | 未开始 | 必须写明"无需登录、跳过 AI 可完成核心验收、不要求付费" |
| B11 | 价格与销售地区 | 未开始 | 首发免费、无内购；确认上架地区 |
| B12 | TestFlight 内测（可选） | 未开始 | 建议先内测一轮再提交审核 |
| B13 | 律师复核隐私政策与免责表述 | 未开始 | 隐私政策、AI 生成内容免责、儿童条款 |

## C. 提交顺序建议

1. B1 账号 → B2 证书 → B3/B4 后台建记录（这三步不阻塞写代码）
2. 同时做 A1–A15，每项留下验证输出
3. A3、A9、A10 三个决策项定稿后，出**正式签名构建**
4. B5–B8 备齐材料 → TestFlight（B12）→ 提交审核
5. 审核期间保持直售渠道可发版，互不影响

## D. 已具备的证据（可直接引用）

- 0.1.59 直售版界面实测：新建、中文输入法、保存/另存为、导入 txt/PDF、导出 PDF、版本恢复、界面可见性 —— 截图在 `0.1.59-GUI验收/截图/`
- 商店沙盒 MCP 六项端到端验收与"保存写回桌面原文件"：结论记录在 `docs/APP_STORE_MCP_DECISION.md` 与发布建议文档
- 隐私相关代码事实：无分析/崩溃/广告 SDK，无 UserDefaults/磁盘空间 API 直接调用，密钥入钥匙串（见 A1 复核命令）
