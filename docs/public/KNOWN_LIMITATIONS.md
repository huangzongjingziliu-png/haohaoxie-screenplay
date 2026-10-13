# Public beta limitations / 测试版限制

- Apple Silicon macOS is tested. Intel, Windows and Linux builds are not release-tested. / 已测试 Apple 芯片 Mac，其它平台未完成发行验收。
- Not a sandboxed App Store build. A separate sandbox experiment found file bookmark and external-agent issues; it is not included in the public installer. / 不是沙盒商店版，独立沙盒原型不包含在公开安装包。
- PDF/OCR/AI conversions can misclassify paragraphs or omit content. Review output against the original. / 转写可能误判或遗漏，必须对照原文校对。
- AI requires your own provider configuration and is subject to its availability, pricing and limits. / AI 需自行配置，服务和额度由提供商决定。
- Project memory travels with `.screenplay` files; user-written personalization stays local. Automatic memory needs a configured model and author review. / 项目记忆随工程保存，个性化留在本机；自动记忆需可用模型并由作者审阅。
- Some backend error details, progress messages and skill descriptions remain Chinese. Core interface text supports Chinese and English. / 部分后端诊断、进度和技能说明仍为中文。
- Signing/notarization is reported in each release; do not assume source builds are signed. / 签名和公证状态以版本说明为准。
- Style availability depends on installed fonts. Fountain/FDX/plain-text exports do not preserve all custom visual styling. / 外观取决于已安装字体，结构格式导出不保留所有自定义外观。

- Series planning, reports and draft labels require 0.1.49 or newer. Use the latest version to edit episodic projects; older versions may discard newer fields. / 分集规划、检查报告与稿号需 0.1.49 或更新版本。分集工程请用最新版编辑，旧版可能丢失新增字段。
- Cross-episode checks require a configured model, process 2–8 episodes per batch (up to 80 source chunks), and provide advisory findings. Relevant scripts, planning and project canon go to the configured provider. / 跨集检查需模型，每批 2 至 8 集且最多 80 个原文分段，报告仅供审阅。相关正文、规划与项目设定会发给所配置的服务商。
- Rename/replacement is case-sensitive literal matching, not semantic entity recognition. Review selected occurrences. Project memory, season planning and character biographies are not automatically rewritten. / 重命名和替换按字面匹配并区分大小写，需检查所选位置，不会自动改写项目记忆、季纲和人物小传。
