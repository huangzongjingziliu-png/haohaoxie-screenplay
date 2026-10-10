# 好好写剧本

面向 Mac 的本地剧本编辑器。支持中英文界面、标准剧本和自由文本、场景白板，以及可选的编剧助理。

**公开测试版 0.1.25。** 安装包见 [Releases](https://github.com/huangzongjingziliu-png/haohaoxie-screenplay/releases)。 已验收的平台为 Apple 芯片 Mac；不是 App Store 版本。

[English](README.md) · [入门说明](docs/public/GETTING_STARTED.md) · [已知限制](docs/public/KNOWN_LIMITATIONS.md) · [隐私说明](docs/public/PRIVACY.md)

## 安装与使用

GitHub Release 提供 DMG 安装包。打开后拖入“应用程序”即可安装，无需安装 Rust、Node.js 或 Codex CLI。源码 ZIP 不是安装包。发布说明会如实标注签名和公证情况；不要关闭系统安全保护。

语言选项位于“更多 → 设置 → 通用 → 界面语言”：跟随系统、简体中文、English。只切换软件界面，不翻译剧本正文，也不改变剧本排版。部分后端诊断和内置技能说明在测试版中仍为中文。

主要功能包括剧本排版、自定义段落样式、场景拖动排序、白板、版本历史、恢复和多格式导出。项目格式为 `.screenplay`。

PDF／图片转写和 AI 功能仍属实验功能，使用后需要校对。没有模型也可使用核心编辑功能。模型需要自行配置，不提供共享密钥，不承诺无限免费额度。

## 开发与反馈

源码构建步骤见英文首页。请通过 GitHub Issues 反馈问题；说明版本、macOS 版本、复现步骤和期望结果。请用自制样例，不要上传私人剧本、API Key 或未授权素材。

目前测试版免费，不预售、不承诺云服务。我们希望了解用户是否持续使用，以及未来是否愿意为官方维护版或支持服务付费。

代码采用 MIT 许可证，第三方依赖见 THIRD_PARTY.md。MIT 允许商业使用及再分发；名称与图标不代表分支项目可冒充官方。
