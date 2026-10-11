# Getting started / 入门

1. Create a screenplay and choose a save location, or open an existing `.screenplay` project. / 新建并选择保存位置，或打开已有工程。
2. More → Settings → Language sets interface language. Text and layout controls screenplay appearance independently. / 更多 → 设置 → 语言选择界面语言；文字与排版单独控制正文样式。
3. Use Source text to paste plain text, Markdown or Fountain; inspect the screenplay view before exporting. / 自由模式可粘贴纯文本、Markdown 或 Fountain，导出前检查识别结果。
4. Save with ⌘S. Keep a separate backup of valuable scripts. Recovery is not a substitute for a backup. / ⌘S 保存；重要稿件另存备份，恢复副本不能替代备份。
5. Import can replace your current screenplay. Save first. / 导入可能替换当前剧本，请先保存。
6. AI is optional. Configure your own model in Settings → Models and skills. Cloud requests transmit selected/context text to that provider; local Ollama needs a separately running local service. / AI 可选，在模型与技能中配置；云模型会收到所选内容或上下文，本机 Ollama 需要另行运行服务。
7. External agents must run locally on the same Mac. Copy the generated MCP configuration into the client's settings, not its chat. Ask it to verify project name, path and revision before editing. / 外部 Agent 必须能访问同一台 Mac；MCP 配置填入客户端设置，写入前确认项目和修订号。

Interface shortcuts / 界面快捷键: ⌘S save, ⇧⌘S save as, ⌘F search, ⌘, settings, Esc leave focus mode. Paragraph shortcuts are configurable in text settings.

## Public beta 0.1.32 / 公开测试版 0.1.32

- Free-writing text size defaults to the screenplay size. Use the bottom slider to adjust it; Reset restores the project default without changing export formatting. / 自由文本默认跟随剧本字号；底部滑杆可独立调整，恢复默认回到该项目字号，不改变导出排版。
- Settings save before switching sections or closing by clicking outside. A failed save keeps the dialog open. / 切换设置栏目或点击弹框外部会先保存；保存失败会保留窗口。
- Add custom skills from Settings → Models and skills by choosing or dropping a folder containing `SKILL.md` (or dropping `SKILL.md` itself). / 在设置 → 模型与技能点击添加，选择含 `SKILL.md` 的技能文件夹；也可将文件夹或 `SKILL.md` 拖到添加区域。
- Create skill is available in the writing assistant and settings. Describe a writing method, generate with your configured model, review the editable Markdown, then save and install. IDs must be unique. / 编剧助理和设置都有创建技能入口：描述写作方法，使用已配置模型生成，检查和编辑 Markdown，最后保存安装。技能 id 不能与已有技能重复。
- Double-click a card’s summary to edit it directly; click outside to save. Double-click its title to rename the actual screenplay scene. Summaries work without AI. Manual summaries are saved in the project and will not be overwritten by automatic generation. Clearing one lets automatic generation resume if enabled. / 双击便签内容即可手写，点击外部保存；双击场景标题修改时会同步到剧本。无需模型；手写内容随工程保存，自动概括不会覆盖。清空后，若自动概括已开启，模型可重新生成。

Long card summaries scroll inside the card in both preview and editing. Double-click enables a cursor in the same text area, preserving its scroll position. / 概括较长时，预览和编辑都在便签内部滚动；双击在原文字处出现光标，并保留滚动位置。
