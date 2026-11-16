# Bundle ID 与 SKU 定稿建议

更新日期：2026-11-16。

## 一、现状

| 渠道 | Bundle ID | 产品名 | 说明 |
| --- | --- | --- | --- |
| 直售（GitHub DMG） | `com.screenplay.studio` | 好好写剧本 | 已发布到 0.1.59，自带更新器与 CLI |
| 商店（沙盒测试） | `com.screenplay.studio.store` | 好好写剧本商店版 | 沙盒、无更新器、无 CLI |

两者的数据目录不同：直售版在 `~/Library/Application Support/ScreenplayStudio` 一类路径，商店版在 `~/Library/Containers/com.screenplay.studio.store/Data`；钥匙串条目也相互隔离。

## 二、建议：正式提交沿用 `com.screenplay.studio.store`

结论：**不要改成 `com.screenplay.studio`，商店版继续用 `.store`。**

理由：

1. **两个渠道必须能同时安装。** 同一个 Bundle ID 的 App Store 版本与直售 DMG 版本会互相覆盖、更新互相打架，macOS 只认一个同 ID 应用。保留不同 ID，用户可先装 DMG 再装商店版对比，或从直售迁移到商店版而不卸载旧版。
2. **容器与钥匙串天然隔离。** 沙盒容器路径由 Bundle ID 决定，`.store` 的测试结果（container 路径、书签、令牌位置）可以原样复用到正式包，不需要重跑一遍沙盒验收。
3. **收据与更新链路清晰。** App Store 收据、TestFlight、审核记录都绑定这个 ID；直售的 minisign 更新清单 `latest.json` 与商店更新互不干扰。
4. **回滚成本低。** 若商店审核受阻，直售渠道照常发版，不受影响。
5. **改名代价高。** Bundle ID 一旦提交就基本不可改（改 ID 等于新建应用记录，用户、评论、收据全部重来），现在改没有收益。

代价与应对：

| 代价 | 应对 |
| --- | --- |
| 两套 ID，用户可能困惑 | 商店描述里写明"App Store 版不含命令行工具与自动更新"，官网下载页同样说明 |
| 数据不互通（最近项目、钥匙串） | 商店版首次启动做一次"从旧版本导入"引导，或在发布说明里写清"两个版本各自独立" |
| 签名与权限需分别维护 | 已在 `packaging/app-store/` 与 `tauri.store.conf.json` 分流，保持现状 |

什么时候该改回 `com.screenplay.studio`：只有当决定**停止直售、商店成为唯一渠道**时。那时也应改产品名（去掉"商店版"）、把 `com.screenplay.studio` 留给商店版，并为老用户做数据迁移说明。现阶段不做。

### 命名规范说明

`com.screenplay.studio.store` 里的 `store` 是渠道后缀，不是产品名的一部分。Apple 不检查后缀含义，但建议在应用记录里加备注"channel-specific bundle id"，避免以后自己混淆。

## 三、SKU 建议

SKU 在 App Store Connect 创建应用时填写，**创建后不可修改**，只用于你自己的报表与内部对账，不展示给用户。

建议：`haohaoxie-screenplay-macos`

备选：`HXXJUBEN-MAC-001`（如果公司内部有编号体系，用内部编号更好）

规则：

- 只用 ASCII 字母、数字、连字符，不用中文、空格、下划线混排。
- 一个渠道一个 SKU；不要用同一 SKU 建第二个应用记录。
- 若未来出 iOS/iPadOS 版，另用 `haohaoxie-screenplay-ios`，不要复用。

## 四、连带需要定的两项（提交前）

1. **版本号**：商店首次提交建议 `CFBundleShortVersionString` 用 `1.0.0`，`CFBundleVersion` 用 `1`；不要沿用 `0.1.59` 这种带"零头"的版本作为商店首版，后续商店更新用 1.0.1、1.1.0 递增。若坚持沿用 0.1.59，也要保证 `CFBundleVersion` 是纯整数且逐次递增。
2. **产品名**：商店包当前叫"好好写剧本商店版"，提交前建议把 `productName` 改为"好好写剧本"，渠道差异只体现在 Bundle ID 与功能开关上，避免用户看到两个不同名字。

## 五、待办

- [ ] 在 App Store Connect 注册 `com.screenplay.studio.store`（需账号）
- [ ] 创建应用记录时填 SKU `haohaoxie-screenplay-macos`
- [ ] 决定商店首版版本号，并同步 `tauri.store.conf.json` 的版本
- [ ] 决定是否把 `productName` 去掉"商店版"后缀
