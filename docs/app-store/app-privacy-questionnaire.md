# App Privacy 问卷答案建议 + PrivacyInfo.xcprivacy TODO 判定

更新日期：2026-11-16。对应构建：商店版 0.1.59（`com.screenplay.studio.store`）。
依据：代码检索 + `src-tauri/PrivacyInfo.xcprivacy` 现状。提交前必须按本节末尾的"复核命令"重跑一遍，不能只照抄结论。

## 一、App Store Connect「App 隐私」怎么填

| 问卷项 | 建议答案 | 依据 |
| --- | --- | --- |
| 是否收集数据 | **否 / 不收集** | 无服务端，无账号；分析、广告、崩溃 SDK 均未接入 |
| 是否用于追踪 | **否** | 无跨应用/网站追踪，无广告标识符 |
| 收集的数据类型 | 全不勾选 | 见上 |
| 第三方 SDK 隐私清单 | 无第三方数据 SDK | 依赖为 Tauri/WebView、PDF/OCR 处理组件，不属数据收集类 SDK |
| 隐私政策网址 | 必填 | 用 `privacy-policy-draft.md` 定稿后托管 |

判断边界（重要）：

- **把内容发给"用户自己配置的模型服务"不等于开发者收集。** 数据接收方是用户选择的服务商，开发者不接收、不中转、不存储。问卷按"不收集"填写是成立的，前提是：应用不内置任何厂商密钥、不代付、不默认开启、不把用户内容发到开发者控制的端点。
- 当前代码默认 `cloud_ai_enabled: false`、`provider_kind: "compatible"`、默认 `base_url` 指向第三方聚合服务 `https://apihub.agnes-ai.com/v1`。**默认关闭即可按"不收集"申报**；但这个默认地址是硬编码的，需要在提交前决定是否清空默认值或改为 `http://localhost` 占位——否则审核可能质疑"预置了服务商"，见 `pre-submission-checklist.md` 代码项。
- 如果以后加"匿名使用统计"或"崩溃自动上报"，必须改为"收集：使用数据 / 诊断"，并同步隐私政策和清单。

## 二、PrivacyInfo.xcprivacy 现状

- `NSPrivacyTracking` = false，`NSPrivacyTrackingDomains` 为空，`NSPrivacyCollectedDataTypes` 为空 —— 与"不收集"一致，保留。
- `NSPrivacyAccessedAPITypes` 目前只声明了 `FileTimestamp`，原因码 `C617.1`、`3B52.1`。

## 三、Required Reason API 逐项判定

Apple 的清单只针对**应用自身代码与内置 SDK 直接调用**的 API。判定方法：在最终 `.app` 二进制与自带 sidecar 上查符号与字符串，而不是只看 Rust 源码。

### 1. 文件时间戳（FileTimestamp）—— 保留，但按调用点裁剪

| 原因码 | 含义 | 是否保留 | 理由 |
| --- | --- | --- | --- |
| `C617.1` | 访问应用容器内的文件 | 保留（若确有容器内文件读取） | 工程、最近项目、恢复数据、令牌文件都在容器内，读取时用 `std::fs::metadata` 拿大小/时间 |
| `3B52.1` | 访问用户通过文件选择框明确授权的文件 | 保留（若确有） | 打开/导入/导出工程时读取用户文件元数据 |

裁剪动作：逐个调用点确认后，删掉没有实际调用的原因码。只声明用到的，多声明同样可能被问。

### 2. UserDefaults —— 建议：**先不声明**，复核后再定

- 检索结果：`src-tauri/src`、`crates` 中未发现 `UserDefaults` / `NSUserDefaults` 直接调用（见复核命令 1）。
- 结论：如果最终二进制也没有相关符号，则不声明；Tauri/WebView 自身的 UserDefaults 使用由框架负责，不应由本应用代为声明。
- 需要改判的信号：最终二进制出现 `NSUserDefaults` 符号且来自本应用代码路径（例如窗口状态保存改成了 defaults）。

### 3. 磁盘空间（DiskSpace）—— 建议：**先不声明**

- 检索结果：未发现 `statvfs` / `statfs` / `NSFileSystemFreeSize` / `volumeAvailableCapacity` 调用。
- 结论：不声明。如果后续为"保存前空间不足提示"加了 `statvfs`，则需加入 `E174.1`（写入前检查可用空间）或对应原因码。
- 注意：`fs2`/`sysinfo` 之类的 crate 若被引入，会间接调用；复核命令 3 覆盖这种情况。

### 4. 键盘输入（KeyboardInput / ActiveKeyboard）—— 建议：不声明

- 应用不监听全局键盘事件，输入法输入走系统 WebView 文本域，不调用 `CGEventTap`、`NSEvent.addGlobalMonitor`。
- 复核命令 4 用于确认没有全局事件监听。

### 5. 其他常见项（当前均判定为"不需要"）

| API 类别 | 判定 | 说明 |
| --- | --- | --- |
| 系统启动时间（SystemBootTime） | 不需要 | 未做启动时长统计 |
| 活跃键盘（ActiveKeyboard） | 不需要 | 未读取当前输入法 |
| 剪贴板（Pasteboard） | 不需要 | 未主动读取剪贴板内容；如后续加"粘贴为剧本块"并主动读剪贴板，需申报 |

## 四、复核命令（提交前在最终构建产物上执行）

```bash
APP="target/release/bundle/macos/好好写剧本商店版.app"
BIN="$APP/Contents/MacOS/$(defaults read "$APP/Contents/Info.plist" CFBundleExecutable)"

# 1. UserDefaults
grep -rn "UserDefaults" src-tauri/src crates src | grep -v target
nm -u "$BIN" | grep -i -E "NSUserDefaults|CFPreferences" | head

# 2. 文件时间戳 / 元数据
grep -rn "fs::metadata\|modified()\|created()" src-tauri/src crates | head

# 3. 磁盘空间
nm -u "$BIN" | grep -i -E "statvfs|statfs|volumeAvailableCapacity" | head
grep -rn "statvfs\|fs2\|sysinfo" Cargo.toml crates/*/Cargo.toml

# 4. 全局键盘监听
nm -u "$BIN" | grep -i -E "CGEventTap|addGlobalMonitor" | head

# 5. 自带 sidecar 单独查一遍（它们也是随包二进制）
for f in "$APP"/Contents/Resources/resources/*/*; do file "$f" | grep -q Mach-O && nm -u "$f" | grep -i -E "UserDefaults|statvfs" ; done
```

判定规则：**命令有命中且来自本应用代码 → 声明并选原因码；无命中 → 不声明。** 每个"不声明"都要在提交记录里留一次命令输出截图或文本。

## 五、与清单文件的联动

- 改 `PrivacyInfo.xcprivacy` 后，商店构建通过 `tauri.store.conf.json` 的 `bundle.macOS.files` 打进 `Contents/Resources/PrivacyInfo.xcprivacy`；用 `plutil -lint` 与 `plutil -p` 校验语法和最终内容。
- 清单中删除的 TODO 注释必须在提交前清干净，不要留"TODO(上架前)"进正式包。
