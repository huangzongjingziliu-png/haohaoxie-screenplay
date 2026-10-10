# Privacy / 隐私说明

This desktop app stores projects, recent-file records, recovery data, assistant memories and model configuration locally. API keys are kept in a local encrypted vault, not in exported screenplay projects. The local account's storage security remains important.

The app does not add product analytics as part of this beta. GitHub, app stores and configured model providers have their own policies.

AI requests may send screenplay context, selected text, recognized attachment text, memory and skill instructions to the provider you configure. Configure cloud processing deliberately; do not send confidential scripts without permission. A locally configured Ollama service is a separate service, not an included model.

External local CLI/MCP clients can read and modify projects through the GUI bridge. Enable them only in trusted clients. Configuration in a chat message does not itself create a connection.

Bug reports are public. Remove API keys, private screenplay text, personal file paths and identifying data before uploading files or logs.

本软件在本机保存工程、最近项目、恢复数据、助理记忆和模型配置。密钥保存在本机加密存储中，不包含在导出的剧本工程里。仍需保护本机账户和存储。

本测试版不新增产品使用追踪。GitHub、商店和模型服务商各自有独立政策。AI 请求可能发送剧本上下文、所选文字、识别附件、记忆和技能内容给用户配置的服务商。没有授权时不要发送机密稿件。外部 CLI/MCP 能操作本机工程，只在可信客户端中启用。

问题报告是公开的；不要上传密钥、私人稿件、个人路径和身份信息。
