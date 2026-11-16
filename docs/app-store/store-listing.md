# App Store 商店文案（草稿）

更新日期：2026-11-16。适用版本：0.1.59 商店构建（`com.screenplay.studio.store`）。
字段上限依据 App Store Connect 中文界面，提交前以后台实际计数为准。

## 应用名称（上限 30 字符）

好好写剧本

备选：好好写剧本 - 中文编剧工具（如重名被占用）

## 副标题（上限 30 字符）

中文剧本写作与分场管理

备选：
- 本机剧本编辑器，专注写作
- 写剧本、管分场、导 PDF

## 宣传文本（上限 170 字符，可随时更新）

按剧本格式写，不用调格式：场景、动作、人物、对白自动分行。支持分场管理、版本历史、PDF 导出与纯文本导入。写作数据保存在本机；需要 AI 时再接入你自己配置的模型。

## 长描述（上限 4000 字符）

好好写剧本是一款面向中文编剧的 macOS 剧本写作工具，专注"把字写出来"，不把时间花在排版上。

【按剧本格式写作】
- 场景标题、动作、人物、对白、括注自动套用行业常用格式，回车即换行到下一类元素
- 中文标点与全角字符处理，输入法直接输入
- 写作区、场景列表、人物与大纲同屏查看

【分场与结构】
- 按场组织剧本，快速跳转、重排、合并
- 场景板查看与调整场次顺序
- 字数、页数、预计时长实时统计

【版本与安全】
- 手动版本 + 自动检查点，随时预览旧稿并恢复
- 保存为单一 `.screenplay` 工程文件，可备份、可拷贝
- 崩溃恢复：异常退出后重新打开可找回未保存内容

【导入与导出】
- 导入：纯文本、Markdown、Word（docx）、RTF、Final Draft（.fdx）、Fountain、PDF 与图片（OCR 识别后校对）
- 导出：PDF（含标准剧本版式）、纯文本等格式
- 分集批量导出，适合剧集项目

【可选：接入你自己的 AI】
- 不内置模型、不代付费用。你可以在设置里填写兼容 OpenAI 接口的服务地址与密钥
- 只有你在界面上明确发起请求时，才会把所选内容发送到该服务
- 不配置也能完整使用全部写作、导入、导出功能

【可选：接入本机 Agent】
- 应用内置本地 MCP 服务，仅监听 127.0.0.1，需要访问令牌
- 供你自己信任的本机工具（如编辑器插件、自动化脚本）读取和修改当前工程
- 关闭应用即停止监听

【关于数据】
- 不接入广告、分析或崩溃上报 SDK
- 工程、最近项目、恢复数据、助理记忆都保存在本机
- 模型密钥保存在本机钥匙串，不写入导出的工程文件

说明：App Store 版本不包含命令行工具（CLI）与自动更新器，更新由 App Store 负责。需要 CLI 的用户请使用官网分发的版本。

## 英文版要点（如后台需要英文文案，建议精简翻译，不是逐句直译）

Haohaoxie Screenplay is a screenwriting app for Chinese-language scripts on macOS. It applies standard screenplay formatting as you type, organizes scenes, keeps manual versions plus automatic checkpoints, and imports/exports PDF, DOCX, FDX, Fountain, RTF, Markdown and plain text. AI assistance is optional and uses a model endpoint you configure yourself; the app ships no model and pays for no API usage. All writing stays on your Mac unless you explicitly send selected text to your own provider. No ads, analytics or crash-reporting SDKs.

## 关键词（上限 100 字符，逗号分隔，逗号后不加空格）

剧本,编剧,写作,分场,对白,剧本格式,中文剧本,大纲,版本,PDF导出,导入,文字处理

（提交前在后台计数器里核对总长度；如超限优先删"文字处理""导入"。）

## 分类建议

- 主要分类：效率
- 次要分类：不选（可选"娱乐"，但会让审核按内容类应用额外关注，非必要不选）

理由：核心行为是写作与文档管理，不是游戏或影音消费；效率类也更贴近同类编剧软件的定位。

## 支持网址与隐私政策网址（占位，需替换为可公开访问的地址）

- 支持网址：`https://<你的域名>/haohaoxie/support`（占位）
- 隐私政策网址：`https://<你的域名>/haohaoxie/privacy`（占位）
- 营销网址（可选）：`https://<你的域名>/haohaoxie`

注意：
1. 两个网址必须无需登录即可打开，且内容与 App 内实际行为一致。
2. 隐私政策需与 `docs/app-store/privacy-policy-draft.md` 定稿版本一致；改一处要同步另一处。
3. 如果暂时没有域名，可先用 GitHub Pages 托管这两页；具体文案仍需按草稿定稿。

## 版权与联系

- 版权行：`© 2026 <你的姓名或公司名>`
- 支持邮箱：`<待填>`
- 审核联系：与 Apple Developer 账号一致，审核备注里另附可用邮箱

## 待办

- [ ] 名称是否被占用，在 App Store Connect 里预检
- [ ] 副标题、关键词按后台字符计数复核
- [ ] 支持/隐私网址换成真实可访问地址
- [ ] 长描述中"分集批量导出"等能力按商店构建实测结果确认后再定稿
