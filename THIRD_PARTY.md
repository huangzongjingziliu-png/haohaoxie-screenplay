# 第三方依赖许可证声明

幕本 (Screenplay Studio) 使用以下第三方开源库。本项目自身采用 MIT 许可证
（见 [LICENSE](LICENSE)）。

## 许可证分类汇总

| 许可证 | 说明 |
|---|---|
| MIT | 宽松许可证，保留版权声明即可 |
| Apache-2.0 | 宽松许可证，含专利授权 |
| MIT OR Apache-2.0 | 双许可证，任选其一（Rust 生态主流） |
| BSD-3-Clause | 宽松许可证 |
| ISC | 类 MIT 宽松许可证 |
| Unicode-DFS-2016 | Unicode 数据文件许可证 |
| MPL-2.0 | 弱 Copyleft，文件级 |

## 直接依赖

| 依赖 | 许可证 | 用途 |
|---|---|---|
| anyhow | MIT OR Apache-2.0 | 错误处理 |
| async-trait | MIT OR Apache-2.0 | 异步 trait |
| chrono | MIT OR Apache-2.0 | 日期时间 |
| clap | MIT OR Apache-2.0 | CLI 参数解析 |
| dirs | MIT OR Apache-2.0 | 系统目录 |
| lopdf | MIT | PDF 生成 |
| printpdf | MIT | PDF 生成 |
| quick-xml | MIT | XML 解析（DOCX） |
| regex | MIT OR Apache-2.0 | 正则表达式 |
| reqwest | MIT OR Apache-2.0 | HTTP 客户端（模型调用） |
| rmcp | MIT | MCP 协议 |
| rusqlite | MIT | SQLite 数据库 |
| serde | MIT OR Apache-2.0 | 序列化框架 |
| serde_json | MIT OR Apache-2.0 | JSON 序列化 |
| sha2 | MIT OR Apache-2.0 | SHA-2 哈希 |
| similar | MIT | 文本 diff |
| tempfile | MIT OR Apache-2.0 | 临时文件 |
| thiserror | MIT OR Apache-2.0 | 错误派生宏 |
| tokio | MIT | 异步运行时 |
| tracing | MIT | 日志追踪 |
| tracing-subscriber | MIT | 日志订阅器 |
| ts-rs | MIT | TypeScript 类型生成 |
| uuid | MIT OR Apache-2.0 | UUID 生成 |
| walkdir | MIT OR Apache-2.0 | 目录遍历 |
| zip | MIT | ZIP 压缩（.screenplay 容器） |
| tauri | MIT OR Apache-2.0 | 桌面应用框架 |
| tauri-plugin-dialog | MIT OR Apache-2.0 | 对话框 |
| tauri-plugin-fs | MIT OR Apache-2.0 | 文件系统 |
| tauri-plugin-shell | MIT OR Apache-2.0 | Shell 执行 |
| libc | MIT OR Apache-2.0 | libc 绑定 |
| ring | ISC | 本机模型密钥 AES-256-GCM 加密 |

## 主要传递依赖（许可证说明）

以下为部分重要传递依赖及其许可证：

| 依赖 | 许可证 |
|---|---|
| hyper | MIT |
| rustls | Apache-2.0 OR ISC OR MIT |
| webpki-roots | ISC |
| ring | ISC |
| libsqlite3-sys | MIT |
| icu_* (ICU4X) | Unicode-DFS-2016 |
| unicode-normalization | MIT OR Apache-2.0 |
| encoding_rs | Apache-2.0 OR MIT |
| brotli | MIT |
| flate2 | MIT OR Apache-2.0 |
| image | MIT OR Apache-2.0 |
| resvg / usvg / tiny-skia | MPL-2.0 |
| fontdb / skrifa / ttf-parser | MIT |
| objc2 / objc2-* | MIT |
| core-foundation / core-graphics | MIT OR Apache-2.0 |

## 许可证文本获取

各依赖的完整许可证文本可在其源代码仓库或 `cargo` 缓存中查阅：

```
~/.cargo/registry/src/*/
```

## 声明

本软件按"原样"提供，不附带任何明示或暗示的担保。
各第三方依赖的版权归其各自作者所有。
