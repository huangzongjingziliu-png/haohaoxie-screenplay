# iCloud（CloudKit）同步可行性评估

更新日期：2026-11-16
范围：只做代码与架构评估，**不改代码、不写同步实现**。
标注：**【已核实】**＝仓库代码可查；**【判断】**＝我的推断；**【待核实】**＝需以 Apple 文档或实测确认。

---

## 结论

**【判断】可行，但属于「中型到偏大型」工程，不是加一个开关的事。**

现有架构已经具备三个对同步很关键的基础：文档块有稳定 UUID、块级 revision、以及写入时的 revision 冲突拒绝。缺的是：云端数据模型、冲突合并策略、离线队列、以及 CloudKit 在 MAS 沙盒与 Developer ID 两种签名下的能力验证。

最小可用版本（只同步单个工程的整包文档、后写覆盖 + 冲突留档）**【判断】约 4–8 人周**；做到块级增量合并再增加 **2–4 人周**；实时协作不在本次评估范围。

---

## 1. 现有架构的事实

### 1.1 工程文件是 ZIP 容器

**【已核实】** `.screenplay` 是 ZIP 容器，内部包含：

- `manifest.json`（标题、时间、格式版本、写入时的 app 版本）
- `document.json`（**整份文档的 JSON**）
- `project.sqlite`（版本、操作日志、分支、快照）
- `story-bible/bible.md`、`PROJECT_RULES.md`
- `imports/original.txt`、`imports/source.bin`、`assets/`、`skills/`

见 [project.rs#L28-L66](../../crates/screenplay-store/src/project.rs#L28-L66)。保存时**整份 `document.json` 重写**，不是按块增量写；写入是原子的（先写临时文件再替换）。

### 1.2 revision 是全局递增的文档版本号

**【已核实】** `doc.revision` 每次提交 `+1`，见 [command.rs#L133-L143](../../crates/screenplay-store/src/command.rs#L133-L143)。写入前用 `check_revision` 比对期望值，不一致直接返回 `RevisionConflict`（[command.rs#L98-L110](../../crates/screenplay-store/src/command.rs#L98-L110)），提示「文档已被其他来源修改，拒绝覆盖」。

**【判断】** 这是典型的乐观并发：适合「同一时刻只有一个写者」的场景，但它只负责**拒绝**，不负责**合并**。

### 1.3 版本与历史存在 SQLite 里

**【已核实】** `project.sqlite` 建了五张表：`meta`、`versions`、`operations`、`branches`、`snapshots`（[db.rs#L101-L157](../../crates/screenplay-store/src/db.rs#L101-L157)）。

- `versions`：`id / branch / parent_version / revision / origin / trigger / auto`，用于版本历史与分支。
- `operations`：命令级操作日志（`old_value`、`new_value`、`revision`）。
- `snapshots`：`payload` 存整份文档 JSON，用于恢复旧稿。

**【判断】** 这套结构对同步的用处是「冲突时可留档、可回滚」，但它本身不是同步协议：`operations` 是命令日志，不是可在另一台设备重放的补丁格式。

### 1.4 块有稳定 UUID 和块级 revision

**【已核实】** `ScreenplayBlock` 有全局唯一 `id`（注释明确写「非顺序，便于稳定同步」）与块级 `revision`，见 [ast.rs#L199-L217](../../crates/screenplay-core/src/ast.rs#L199-L217)。

**【判断】** 这是未来做块级增量同步最重要的基础：可以按 block id 对齐两台设备的差异，而不必整篇比较文本。

### 1.5 现有 diff 是「给人看的」，不是合并器

**【已核实】** `crates/screenplay-core/src/diff.rs` 产出的是语义化展示结构（`SceneAdded / SceneRemoved / SceneMoved / BlockChanged / BlockRetyped` 等），用于界面展示（[diff.rs#L1-L50](../../crates/screenplay-core/src/diff.rs#L1-L50)）。

**【判断】** 它**不能**直接当三方合并用：没有 base 版本、没有 patch 应用、没有冲突标记。做合并需要新写一层。

### 1.6 自动保存与恢复

**【已核实】** `ProjectService` 持有 `RecoveryManager` 与自动保存状态（[service.rs#L108-L120](../../src-tauri/src/service.rs#L108-L120)）；自动保存按间隔触发，并会调用 `auto_checkpoint` 生成自动版本（[service.rs#L218-L235](../../src-tauri/src/service.rs#L218-L235)）。恢复数据写在临时目录（测试路径见 [service.rs#L150](../../src-tauri/src/service.rs#L150)）。

### 1.7 目前没有任何云同步代码

**【已核实】** 全仓库 `src/`、`src-tauri/`、`crates/` 中没有 `CloudKit` / `iCloud` / `CKRecord` 相关实现或占位；`updates.rs` 是应用更新，不是数据同步。

---

## 2. CloudKit 落在 MAS 沙盒下的前提

**【待核实，需以 Apple 最新文档与开发者账号实测为准】**

1. **iCloud capability**：需要在开发者后台创建 CloudKit 容器（`iCloud.com.screenplay.studio.store` 之类），并在两套构建里声明 `com.apple.developer.icloud-services`（CloudKit）与 `com.apple.developer.icloud-container-identifiers`。
2. **签名方式**：MAS 版走 App Store 签名没问题；**直售版（Developer ID）能否使用 CloudKit，需要实测确认**——这取决于该 entitlement 对 Developer ID 描述文件的开放情况。
3. **沙盒**：CloudKit 走系统网络，现有 `com.apple.security.network.client` 已足够；不需要额外文件系统权限。
4. **账号前提**：用户必须登录 iCloud 且开启 iCloud Drive/CloudKit；未登录时功能应降级为「本地可用、同步不可用」，不能阻塞写作。
5. **测试**：需要 App Store Connect 的沙盒测试账号；同一 Apple ID 在两台设备上的同步测试无法完全用模拟器替代。

**【判断】** 如果直售版不能使用同一套 CloudKit 容器，就必须二选一：同步只给商店版（符合当前商业化方向），或为直售版另做一套（不建议）。

---

## 3. 主要风险

| 风险 | 现状 | 影响 | 判断 |
|---|---|---|---|
| **冲突合并** | 只有 revision 拒绝，没有合并器 | 两端同时改，后写者会覆盖或被迫放弃 | 最高风险；MVP 必须先做「冲突留档 + 用户选择」，不能静默覆盖 |
| **离线编辑** | 完整本地能力，无云队列 | 离线改完再上线，冲突概率高 | 必须有本地待同步队列与「上次同步基线 revision」 |
| **整包体积** | 整份 `document.json`，含导入原文/附件 | CloudKit 单记录有大小上限【待核实：常见说法为记录 1 MB、资源 250 MB】 | 大工程必须走 `CKAsset` 或分块，不能塞进单个字段 |
| **配额** | 无账号/无配额概念 | 私有库占用用户 iCloud 空间【待核实】 | 需要在文案里说明，避免「同步把 iCloud 塞满」投诉 |
| **数据模型演进** | `manifest.format_version` 已存在 | 新版本写出的文档，旧版本可能读不了 | 同步层要带 format_version，拒绝不兼容写入 |
| **附件与导入原文** | 存在容器内 | 体积大、变化少 | MVP 不同步，只同步正文；后续按需 |
| **版本历史** | SQLite 五张表 | 两边历史无法直接合并 | MVP 不合并历史，只保证当前文档一致 |
| **测试成本** | 无多设备测试设施 | 冲突与离线问题难复现 | 需要可注入的假云端（内存实现）先做逻辑测试 |

---

## 4. 最小可用路径（第一版同步）

### 4.1 做

1. **同步单位**：单个工程的 `document.json` + 少量元数据（标题、更新时间、format_version、revision）。**不同步** `project.sqlite`、附件、导入原文、skills。
2. **云端模型**：一个工程一条记录；正文用 `CKAsset` 或分块存储（规避单记录体积上限）。字段至少包含 `revision`、`baseRevision`、`deviceId`、`updatedAt`。
3. **触发时机**：保存后、窗口失焦、退出前、启动时各拉一次；带节流，不做实时。
4. **冲突策略**：本地记录「上次同步的 revision」。发现远端 revision 前进且本地也改过时：
   - 先自动创建一个本地版本（复用 `auto_checkpoint`）；
   - 以「保留两份、让用户选」呈现，默认不覆盖；
   - 绝不静默后写覆盖。
5. **降级**：未登录 iCloud、无网络、容器不可用时，同步状态显示「未同步」，写作功能不受影响。
6. **可测试性**：把云端访问抽成 trait，先用内存实现跑冲突/离线/重试用例，再接真 CloudKit。

### 4.2 第一版不做

- 块级增量同步与三方合并（依赖 block UUID，但需要新写合并器）；
- 实时协作、光标/在线状态；
- 附件、导入原文、技能包、版本历史同步；
- 多工程批量迁移与「换机向导」；
- Windows/Linux 端。

### 4.3 前置验证（建议先做，1–2 天量级）

1. 用最小 Swift/Rust 程序验证：MAS 签名 + 沙盒下能否读写 CloudKit 私有库。
2. 验证直售（Developer ID）构建是否同样可用；若不可用，锁定「同步仅商店版」。
3. 用一份接近真实体量的剧本（例如 3 万字、含导入原文）测 `document.json` 体积，确认是否必须走 `CKAsset`。

---

## 5. 工作量量级（判断，不是承诺）

| 阶段 | 内容 | 量级 |
|---|---|---|
| 前置验证 | CloudKit 权限、直售可用性、体积实测 | 数天 |
| MVP | 整包同步 + 冲突留档 + 离线队列 + 设置界面 | 4–8 人周 |
| 块级增量 | 基于 block UUID 的三方合并、冲突可视化 | 再 2–4 人周 |
| 稳定性 | 多设备测试、迁移、错误恢复、配额文案 | 再 1–2 人周 |

**【判断】** 如果目标是 1.1 就收费，**只做 MVP**，并且第一版就把「冲突不静默覆盖」做对；块级合并可以留到 1.2。

---

## 6. 结论

- **可行**：数据结构对同步友好（块 UUID、块级 revision、整包原子写、版本快照可回滚）。
- **不便宜**：真正的工作量在冲突、离线与测试，不在 CloudKit API 本身。
- **建议顺序**：先做 1.2 节的前置验证，再决定「同步只给商店版」还是「两渠道共用」；商业化上这与 [monetization-plan.md](monetization-plan.md) 的「同步买断」方案可以直接对齐。
