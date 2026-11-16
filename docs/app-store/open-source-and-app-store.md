# 开源协议与 App Store 并行的商业结构方案

更新日期：2026-11-16
适用对象：好好写剧本（Screenplay Studio）macOS 桌面版与 App Store 商店版
性质：现状核查 + 选项分析 + 建议。**本文不是法律意见**，涉及版权、商标、商店条款的判断在提交前请让律师复核。

## 结论摘要

1. 当前仓库根 `LICENSE` 是 **MIT**，版权行写的是「Copyright (c) 2026 幕本 Screenplay Studio」。Cargo workspace、THIRD_PARTY.md、中英文 README 均声明 MIT。
2. MIT 不禁止任何人改名、改代码、打包上架（包括 Mac App Store），也不与 Apple 的 DRM / FairPlay 冲突；能约束「冒充官方」的只有商标与 Apple 审核规则，这两块目前仓库里没有明确文件。
3. 建议先走「全开源 + 商店卖省事」这条路，第一步不是改协议，而是补齐二进制分发义务与应用内声明（见第四节）。是否升级为「开源核心 + 商店独占能力」留到有真实收入信号后再评估。

---

## 一、当前许可事实（已核对）

| 位置 | 内容 | 说明 |
|---|---|---|
| [LICENSE](../../LICENSE) | MIT，`Copyright (c) 2026 幕本 Screenplay Studio` | 仓库唯一的正式许可文件 |
| [Cargo.toml](../../Cargo.toml#L17-L19) | `[workspace.package] license = "MIT"` | Rust 侧与根 LICENSE 一致 |
| [THIRD_PARTY.md](../../THIRD_PARTY.md) | 开头声明「本项目自身采用 MIT 许可证」 | 并列出依赖的 MIT / Apache-2.0 / BSD-3 / ISC / Unicode / MPL-2.0 |
| [README.zh-CN.md](../../README.zh-CN.md#L134) | 「代码采用 MIT」「分支项目请避免冒充官方」 | 后半句是请求，不是许可条款 |
| [README.md](../../README.md#L149) | "Code is MIT licensed; Forks should not imply official affiliation." | 同上 |
| [package.json](../../package.json) | 只有 `"private": true`，没有 `license` 字段 | 前端包不对外发布，影响有限，但会造成元数据不一致 |
| [src/assets/providers/README.md](../../src/assets/providers/README.md) | 图标来自 Lobe Icons，MIT；「品牌名与商标归各所有者」 | 这是仓库里唯一明确的商标保留声明，只覆盖模型厂商图标 |
| [assets/branding/haohaoxie-u.png](../../assets/branding/haohaoxie-u.png)、[src-tauri/icons/icon.icns](../../src-tauri/icons/icon.icns) | 无单独权利声明 | 按现状落入仓库 MIT 范围；**没有**「名称与图标保留所有权利」的书面表述 |
| git remote | `github.com/huangzongjingziliu-png/haohaoxie-screenplay`，公开仓库，历史 Release 可查最早 0.1.32，最近为 v0.1.59 | 历史提交与 tag 均已按 MIT 公开 |

依赖侧：THIRD_PARTY.md 的清单里**没有 GPL / AGPL / LGPL**；弱 Copyleft 只有 `resvg / usvg / tiny-skia`（MPL-2.0，文件级）。这是按其清单核对的结论，**没有**对 `Cargo.lock` 做全量许可证扫描（建议提交前用 `cargo-deny` 或 `cargo-about` 跑一遍，见第五节）。

### 一个已确认的合规缺口

MIT 的条件是「在所有副本或实质部分中保留版权声明与许可声明」。当前商店包 `Contents/Resources/` 只有 `icon.icns`、`PrivacyInfo.xcprivacy`、`resources/`，**没有 LICENSE、没有 THIRD_PARTY.md、没有 MPL-2.0 依赖的许可文本**。也就是说：代码许可允许你上架，但分发出去的二进制目前没带许可声明，严格说没满足 MIT 与 MPL 的分发义务。这是可以低成本修掉的问题，不涉及协议变更。

---

## 二、MIT 对三个问题的实际含义

### 1. 别人能否拿去改名上架？

**按 MIT 文本，可以。** MIT 明确授予「use, copy, modify, merge, publish, distribute, sublicense, and/or sell」，且不限制用途。对方只需保留版权与许可声明，就可以改产品名、换图标、自行签名，甚至收费。

能拦住对方的手段不在版权，而在：

- **商标**：产品名「好好写剧本」「幕本」、图标、界面识别度。目前仓库没有商标声明，也没有查到注册信息（我没有核实注册状态，需你或律师确认）。未注册不等于没有权利，但维权成本高。
- **Apple 审核规则**：App Store 审核指南 4.1（抄袭 / 复制品）与 4.3（垃圾应用、饱和类别）可能拒绝明显换皮的副本；这是 Apple 的裁量，不是你的版权权利，也不能提前保证结果。
- **署名与来源**：MIT 要求对方保留你的版权声明，所以对方不能完全抹掉「基于谁的代码」，但可以在 README 角落保留，产品上完全换名。

结论：**「防止被改名上架」这件事，MIT 给不了；商标 + 品牌 + 持续更新才给得了。** 这部分属于法律判断，标注「需律师复核」。

### 2. 能否在 Mac App Store 上架？

**你自己的代码可以。** MIT 不限制分发渠道，Apple 的商店条款也不与 MIT 冲突；你需要保留版权与许可声明，并遵守 Apple 的开发者协议与审核指南。

需要注意的不是协议冲突，而是三件事：

- 二进制里要带 MIT 声明（当前缺失，见上）。
- MPL-2.0 依赖（resvg 系）分发时要能提供其源代码与许可文本；MPL 是文件级 Copyleft，不要求你的整个应用开源，但要求被覆盖文件的源码可得。公开仓库 + 上游 crates.io 通常已满足「可得」，稳妥做法是在应用内或商店页面附第三方许可清单。
- 沙盒、签名、隐私清单等工程项与协议无关，见 [pre-submission-checklist.md](pre-submission-checklist.md)。

### 3. 是否与 App Store 的 DRM 冲突？

**不冲突。** MIT 没有反 DRM / 反规避条款，Apple 的 FairPlay 与商店分发方式不触发任何 MIT 条件。真正会与商店 DRM 产生张力的是 GPLv3 一类带反 Tivoization 条款的协议；本项目依赖清单中没有 GPL 系。这一点相对确定，但仍建议律师在提交材料时一并过目。

---

## 三、三条路的落地形态

### 路线 A：全开源 + 商店卖省事

**形态**：代码保持 MIT，GitHub 继续公开；App Store 卖的是「签好名、过公证、沙盒版、自动更新、商店账单、出问题有人管」这些便利，而不是代码本身。

**要改什么**：

1. 打包时把 `LICENSE`、`THIRD_PARTY.md`、MPL 依赖许可文本放进 `Contents/Resources/`（`tauri.store.conf.json` 的 `bundle.resources` 增加条目即可，直售包同理）。
2. 新增一份品牌声明（例如 `BRAND.md` 或 `NOTICE`）：名称、图标、宣传素材归幕本所有，MIT 只覆盖代码。**不改 LICENSE 也能做到**，用独立文件表达保留意图。
3. README 与商店文案统一口径：「代码 MIT，名称与图标保留」，避免读者误解为「连品牌一起送」。
4. 给 `package.json` 补 `"license": "MIT"`，消除元数据不一致。

**代价**：无法阻止别人改名上架；你的护城河是品牌、更新节奏、社区信任与商店履约能力。若出现换皮竞品，只能走商标投诉与 Apple 举报，结果不可控。

**对已发布历史的影响**：无。MIT 不可撤销，v0.1.32–v0.1.59 已公开的代码永远可以被 fork；这条路等于承认并利用这个事实。

### 路线 B：开源核心 + 商店独占能力

**形态**：编辑内核、文件格式、导入导出、CLI/MCP 保持 MIT；商店版独有的能力（例如云端协作、账号与同步、团队审阅、商店内购买的高级 AI 编排）放在闭源 crate 或服务端，用授权校验控制。

**要改什么**：

1. 按 crate 拆分许可：`crates/screenplay-core|store|import|export` 维持 MIT，`src-tauri` 或新增 `crates/screenplay-pro` 改为专有许可（每个 crate 放独立 LICENSE，根 LICENSE 说明分层）。
2. 引入贡献者协议（CLA 或 DCO），否则外部贡献的 MIT 代码无法合法并入闭源部分。
3. 商店独占能力必须真的「离开服务就不成立」（账号、同步、云端额度），否则 fork 只是少一个开关，商业价值有限。
4. 构建拆分：开源构建与商店构建走不同 feature，避免闭源代码混进公开产物。

**代价**：仓库结构复杂化；社区会质疑「说好 MIT 又收回」；维护两套构建与文档；CLA 会劝退部分贡献者。当前项目还没有形成外部贡献规模，这部分成本短期偏高。

**对已发布历史的影响**：**无法回收**。最后一个 MIT 提交之后，任何人都可以在那个快照上继续开发并上架。你能闭源的只有「新增代码」，不能闭源「已经 MIT 出去的 0.1.x」。如果目标真是防 fork，这条路只能止血，不能逆转。

### 路线 C：彻底闭源

**形态**：停止公开新代码，LICENSE 改为专有；GitHub 仓库转为私有或归档。

**要改什么**：

1. 新版本 LICENSE 换成专有条款；README 删除「MIT 开源」表述；商店文案同步。
2. 处理 GitHub 上的历史：已公开提交与 tag 无法收回（fork 与镜像可能已存在），转为私有只影响之后的访问。
3. 内部流程补齐：第三方依赖审计、贡献者授权（若有外部提交）、发布物许可清单。

**代价**：失去「开源」这一信任与传播杠杆；现有 README 的「MIT 开源」卖点需整体改写；对审核没有加分也没有减分；对已存在的 fork 无任何约束力。

**对已发布历史的影响**：MIT 授权对已发布版本永久有效。你可以在新版本用别的协议，但任何人仍可基于 v0.1.59 的 MIT 代码继续分发，包括上架。真正「彻底」闭源需要重写历史中已被公开的部分，成本和风险都高，通常不现实。

---

## 四、建议方案与第一步（建议，不是决定）

**建议先走路线 A。** 理由：

- 当前阶段的核心问题是「有没有人愿意长期用」，而不是「防不防得住换皮」。MIT 带来的传播与信任，比理论上的 fork 风险更值钱。
- 商店版与直售版的差异已经落在 bundle id 与功能开关上（`com.screenplay.studio.store`、`--features app-store`），商业结构不需要靠协议差异化来支撑。
- 路线 B/C 都会与「已经 MIT 发布的 0.1.x」产生叙事冲突，收益要到有明确付费能力时才成立。

**可执行的第一步（不动 LICENSE、不动业务代码）**：

1. 在 `tauri.store.conf.json` 和 `tauri.conf.json` 的 `bundle.resources` 中加 `LICENSE`、`THIRD_PARTY.md`，并在「关于」或设置页提供入口；重跑商店构建后用 `ls Contents/Resources` 验证。
2. 新增 `BRAND.md`：写明「代码 MIT；名称『好好写剧本』『幕本』、图标与宣传素材保留权利；分支不得暗示官方背书」。这是独立文件，不修改 LICENSE。
3. 统一 README 中英文口径与 `package.json` 的 `license` 字段。
4. 提交前跑一次依赖许可证审计（`cargo-deny` 或 `cargo-about`），把 MPL-2.0 依赖的许可文本纳入发布物。

以上四步都不改变现有授权，也不会让已发布的 MIT 版本产生任何回退。

---

## 五、需要律师或账号侧确认的清单

- 「好好写剧本」「幕本」是否已注册或申请商标；未注册时以什么主张阻止换皮（需律师）。
- 应用图标、`haohaoxie-u.png` 宣传图是否做过版权登记；与外包/委托创作的权利归属（需律师）。
- Apple 开发者协议中关于开源代码、重复应用、4.1/4.3 审核裁量的具体适用（以 Apple 最新条款为准）。
- MPL-2.0 依赖在商店二进制中的告知方式是否足够（通常「许可文本 + 源码可得」即可，需律师确认）。
- 若未来转向路线 B/C：CLA 文本、贡献者历史授权、闭源 crate 的边界设计。

## 附：本次核对方式

```bash
# 许可与声明
sed -n '1,25p' LICENSE
grep -n '"license"\|"private"' package.json
sed -n '15,22p' Cargo.toml
sed -n '1,20p' THIRD_PARTY.md
sed -n '1,10p' src/assets/providers/README.md
grep -n 'MIT\|license' README.md README.zh-CN.md

# 发布物内是否带许可声明（当前为空）
find target/release/bundle/macos/好好写剧本.app -iname '*LICENSE*' -o -iname '*THIRD*'

# 依赖许可证（建议补跑）
# cargo deny check licenses
# cargo about generate about.hbs > THIRD_PARTY.html
```
