# StoreKit 2 内购接入方案（非消耗型买断）

更新日期：2026-11-16
前提：商店版免费下载，一次买断解锁专业功能，不做订阅；AI 由用户自带 key；首版不带积分。
标注：**【已核实】**＝公开文档或仓库代码可查；**【判断】**＝我的方案/推断；**【待核实】**＝需实测或查最新文档。

---

## 1. 商品设计

| 项 | 建议 |
|---|---|
| 商品 ID | `com.screenplay.studio.pro`（**固定不变**，与 bundle id 解耦） |
| 类型 | 非消耗型（Non-Consumable） |
| 数量 | **只做一个**，一次买断解锁全部专业功能 |
| 参考名称 | 专业版 |
| 描述 | 解锁无水印 PDF、FDX/Fountain、稿号与版本对比导出、分集批量导出（HTML 与纯文本免费） |
| 价格 | **原价 68 元（用户已定）**；上线后前三个月早鸟 38 元，之后调回 68 元；**【待核实】** 对应 App Store Connect 人民币价格档 |
| 家庭共享 | **建议关闭**（判断：买断价低，开启会稀释收入；且未来同步按 Apple ID 隔离，家庭共享会造成理解混乱） |

**【已定 · 2026-11-19】** 1.0 只卖桌面版的交付质量，**不包括同步**；不要为「同步」再建第二个商品。同步与 iPad 版上线后作为同一买断的增量权益，不二次收费，避免早期买家觉得「买亏了」，也避免两次付费的审核与文案复杂度。

**【已定】** 模板属于免费内置内容，1.0 不作为付费卖点。若未来做「模板/内容包」，再单独评估独立非消耗型商品 ID；不要修改现有商品的内容描述来「扩容」，否则老用户会认为权益被稀释。

---

## 2. 技术架构选择

**【已核实】** 当前应用是 Tauri 2（Rust 后端 + WebView 前端），没有 Swift 代码；StoreKit 2 是 Swift API（`Product`、`Transaction`、`AppStore`）。

**【判断】** 三条可选路径：

| 方案 | 做法 | 优点 | 风险 |
|---|---|---|---|
| A. Swift 辅助进程 | bundle 内放一个小的 `storekit_helper`，通过 stdin/stdout JSON 与 Rust 通信 | 隔离清楚，StoreKit 代码独立 | 已否决：helper 无窗口，`product.purchase()` 会报 `invalidPresentationContext`（本机实测） |
| B. Swift framework + C ABI | `StoreKitBridge.framework` 随 App 主进程加载，Rust 通过 dlopen/dlsym 调用 | 单进程，购买有窗口上下文；契约稳定 | 需要维护 framework 构建与签名；Rust 侧手写 dlopen |
| C. 社区 Tauri IAP 插件 | 直接引入现成插件 | 上手快 | 多数基于 StoreKit 1 或收据校验，长期维护不可控【待核实】 |

**【已定】采用 B**：helper 方案在实测中确认无法在无窗口进程里发起购买，已删除，不再保留第二条生产路径。Swift 源码仍放在 `packaging/iap/Sources/`，由 `build-framework.sh` 编译成 framework。

### 2.1 当前实现（2026-11-16）

- **framework**：`packaging/iap/build-framework.sh` 生成 `Framework/StoreKitBridge.framework`（arm64 + x86_64，ad-hoc 签名），商店包通过 `bundle.macOS.frameworks` 放入 `Contents/Frameworks`。
- **C ABI**：`sk_abi_version` / `sk_products` / `sk_purchase` / `sk_entitlement` / `sk_restore` / `sk_free`，返回 JSON 字符串，由调用方释放。
- **Rust**：`src-tauri/src/entitlement.rs` 用 `dlopen` 加载，进程内完成四个动作；framework 缺失时降级为假 provider（`SCREENPLAY_PRO_FAKE=1` 模拟已购）。
- **IPC**：`entitlement_status` 契约不变（`pro / source / productId / checkedAt / verified`）；新增 `store_products`、`store_purchase`、`store_restore` 三个命令。
- **超时**：查询与恢复 4 秒；购买含用户交互，给 120 秒，避免 4 秒误伤正常付款流程。
- **缓存**：暂不做本地持久缓存；StoreKit 交易记录本身可离线读取。断网语义见第 3 节。
- **UI**：商店构建（`app-store` feature + `npm run build:store`）接入收费墙与购买入口；直售构建不加载付费墙，行为不变。当前进度见本文件 2.1 与代码。

**【判断】** 无论哪条路，Rust 侧都应先定义一个 `EntitlementProvider` trait：

```rust
trait EntitlementProvider {
    fn status(&self) -> EntitlementStatus;                  // 查当前权益
    fn products(&self) -> Result<Vec<StoreProduct>, String>; // 查商品
    fn purchase(&self, product_id: Option<&str>) -> Result<PurchaseOutcome, String>;
    fn restore(&self) -> Result<EntitlementStatus, String>;
}
```

开发期用「读环境变量的假实现」，测试不依赖任何 Apple 账号。

---

## 3. 解锁状态的本地校验与缓存

**【已核实】** StoreKit 2 的 `VerificationResult` 会对交易 JWS 做本地签名校验，非消耗型权益可通过 `Transaction.currentEntitlements` 获取，不需要自建服务器。

**【判断】** 落地方案：

1. **权威来源**：`Transaction.currentEntitlements`（启动、回前台、购买/恢复后各查一次）。
2. **本地缓存（后续版本，当前未落盘）**：写入现有 SQLite（或独立小表）：
   - `pro_unlocked`（bool）
   - `original_transaction_id`
   - `purchase_date`
   - `last_verified_at`
   - `app_account_token`（若使用）
3. **离线可用**：非消耗型不过期，缓存为 `true` 时直接放行，不因断网降级。
4. **失败策略**：查询失败但缓存为 `true` → 保持解锁；查询明确返回「无权益」（例如退款撤销）→ 关闭专业功能，但**不锁用户文件**，只影响导出选项。
5. **不写死**：不要把解锁状态写进工程文件或导出物；它属于本机授权状态。

**【待核实】** StoreKit 对离线 `currentEntitlements` 的缓存行为在不同 macOS 版本上是否一致，需要在无网络、重启后各测一次。

---

## 4. 恢复购买与跨设备

- **应用内入口**：设置页与购买弹窗都要有「恢复购买」，调用 `AppStore.sync()`；该 API 会要求 Apple ID 验证，**不要频繁调用**（有节流）。
- **启动时静默检查**：只查 `currentEntitlements`，不弹系统登录框。
- **跨设备**：同一 Apple ID 登录新 Mac 后，启动检查即可恢复；若未自动出现，再让用户点「恢复购买」。
- **退款/撤销**：监听 `Transaction.updates`，收到撤销后更新缓存并提示，但不删除任何本地数据。

**【判断】** 不要做「一个授权码多机激活」这类自建方案；它既增加服务器成本，也会与 App Store 的购买恢复语义冲突。

---

## 5. 限时优惠价的实现

**【已核实】** 非消耗型商品没有「订阅首购优惠」那类原生促销 API。

**【判断】** 可行做法只有三种：

1. **改价**：在 App Store Connect 调低价格，活动结束后调回。已购买用户不受影响，新用户按当时价格购买。
2. **价格计划（如有）**：App Store Connect 支持为价格变更设置生效日期；**【待核实】** 非消耗型是否支持「开始/结束日期」的定时价格，需要在后台确认。
3. **促销代码**：生成一次性兑换码做定向赠送，不做公开限时。

**【判断】** 不要用「限时免费」再改成付费：非消耗型商品一旦免费发放，无法收回权益，且会引发「说好免费」的差评。用户提出的「改价而非限时免费」是正确方向。

**【已核实】** 38 元是中国区可用价格点（Apple Creator Studio 中国区订阅价 RMB 38/月，Apple 官方新闻稿 2026-01）。**【待核实】** 68 / 38 元在非消耗型商品价格选择器里的最终档位；改价后商店页展示由 Apple 自动更新，应用内应读取 `Product.displayPrice`，不要硬编码金额。早鸟期结束需人工改回原价，步骤见 [monetization-plan.md](monetization-plan.md) 4.1。

---

## 6. 测试方案

### 6.1 本地测试（不需要开发者账号）

**【已核实】** Xcode 的 StoreKit Configuration 文件可以在本地模拟商品、购买、退款、恢复，不需要真实账号或 App Store Connect 商品。

**【判断】** 对本项目的落地方式：

1. 建一个 `.storekit` 配置文件，商品 ID 与线上一致，价格为占位值。
2. 在 App 宿主内的 XCTest 中 `dlopen` framework，经 C ABI 验证四个动作与 JWS 校验（已跑通）。
3. Rust 侧目前只覆盖「framework 可加载、无权益时返回未购买」；未购买/已购买/离线/撤销/恢复失败的单测待补。
4. 前端用 mock 状态验证 UI：锁定态、购买中、成功、失败、恢复中。

### 6.1.1 已跑通的边界（2026-11-19 实测）

- **真实 StoreKit 交易**：`XcodeHost` + `Pro.storekit` 的 3 条 XCTest 全部通过（查商品 / 购买→权益 / 恢复购买），framework 用真实 StoreKit 2 接口。
- **应用内闭环**：在真实商店包（1.0.0 build 1，`com.screenplay.studio.store`）里跑通「未购 → 点购买 → 权益立即生效 → 导出菜单解锁 → PDF 水印消失 → 重启仍已购 → 恢复购买」。该轮交易的成败由 `SCREENPLAY_ENTITLEMENT_SOURCE=fake` 提供，**不是**真实 StoreKit 交易。
- **无法合并的一步**：本地 `.storekit` 配置只能经 `SKTestSession` 注入 XCTest 宿主，普通进程创建会 abort（崩溃栈落在 XCTestLibrary）。因此「真实交易 + 应用 UI」无法在同一进程内同时演示；需要沙盒账号后在 MAS 构建上做。
- 截图证据：`docs/app-store/evidence/`。

### 6.2 沙盒测试（需要开发者账号与商品配置）

- 在 App Store Connect 创建商品并进入「准备提交」，用沙盒 Apple ID 在 MAS 构建上购买。
- 测：首次购买、取消、重复购买（应提示已拥有）、恢复购买、退款撤销、断网启动。
- **【待核实】** 沙盒环境的购买弹窗与正式环境存在差异，最终仍需 TestFlight 复核。

---

## 7. MAS 沙盒注意事项

**【判断】** 基于现有沙盒配置与公开文档，需注意：

1. **不需要额外 entitlement**：IAP 不要求新的沙盒权限；现有 `network.client` 足够（若未来做服务端校验）。商品能力在 App Store Connect 开启，不在 entitlements 里。
2. **framework 必须签名并随包**：`StoreKitBridge.framework` 放进 `Contents/Frameworks`，构建时先跑 `packaging/iap/build-framework.sh`；正式签名时 framework 需与 App 同 Team、随包签名，否则 MAS 上可能被拒载。
3. **商品必须先存在**：1.0 若不带内购，不要初始化 StoreKit；1.1 提交时商品要与二进制同版本审核，否则购买入口会失败。
4. **审核可见性**：审核员用沙盒账号购买时不应被要求在应用外注册账号；购买流程必须完全在应用内完成。
5. **不出现站外购买**：商店版内不得出现「官网购买」「直售版更便宜」等文案或链接。
6. **价格展示**：用 `displayPrice`，避免汇率与档位变化导致文案错误。

---

## 8. 工作量估计（判断，不是承诺）

| 阶段 | 内容 | 量级 |
|---|---|---|
| 预研 | framework + C ABI 打通，StoreKit 配置下四动作跑通（已完成） | 0.5–1 天 |
| 后端 | 四个命令 + `EntitlementProvider` + 缓存表 | 1 人周 |
| 前端 | 购买弹窗、锁定态、恢复购买、错误提示 | 3–5 天 |
| 测试 | StoreKit 配置、沙盒购买与退款、沙盒签名包实测 | 3–5 天 |
| 审核准备 | 商品文案、截图、审核备注 | 1–2 天 |

**【判断】** 合计约 **2–4 人周**；framework 路线已去掉「helper 能否在沙盒里买」这一最大不确定性。

---

## 9. 待确认事项

1. 商品 ID 是否就用 `com.screenplay.studio.pro`。
2. ~~最终价格档~~ **已定：原价 68 元，上线前三个月早鸟 38 元，之后调回 68 元；早鸟用户永久保留权益。**
3. 是否关闭家庭共享（我建议关闭）。
4. ~~同步上线后是否并入同一买断~~ **已定：并入；1.0 不含同步与 iPad 版，留到后续版本，不二次收费。**
5. ~~helper 方案是否可接受~~ **已定：改用进程内 framework，helper 已删除。**
6. ~~收费墙何时接进 UI~~ **已定：1.0 随商店版一起接；直售构建不加载付费墙。**
