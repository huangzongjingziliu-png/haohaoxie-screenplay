# External agents / 外部 Agent

Requires app 0.1.51 or newer for the workbench tools below. CLI and MCP use the same feature adapter, command service, revision sequence and undo behavior as the GUI. No extra model configuration is needed for these editing APIs; your external agent provides its own reasoning.

以下工作台接口需要 0.1.51 或更新版本。CLI 与 MCP 共用编辑逻辑。它们用于让外部 Agent 操作剧本，不需要先配置软件内置模型。

## Confirm the target / 先确认目标

Use `project_info` or CLI `status` to check bridge versus standalone mode, project name/path, `projectId`, `activeEpisodeId` and revision. Then call `workbench_overview` to obtain the episode directory, planning, reports and draft label. Do not assume the current episode is Episode 1.

先确认模式、工程名称与路径，再读取工作台总览。旧的 block 编辑工具只操作当前集。`read_episode` 可以读取其他集的正文和白板，且不会切换 GUI。

## MCP tools

| Tool | Purpose |
| --- | --- |
| `workbench_overview` | Read project identity, revision, active episode, episode directory and planning |
| `read_episode` | Read one episode without changing the active GUI episode |
| `edit_episodes` | enable, add, switch, rename, reorder, delete, disable |
| `save_planning` | Compare-and-set season and episode planning fields |
| `preview_replacement` | Preview individual literal matches in current/all-episode scope |
| `apply_replacement` | Apply selected preview keys at the same revision |
| `compare_drafts` | Compare named snapshots by episode identity |

The read-only resource `screenplay://workbench` also provides the overview. Tool schemas describe their required inputs. Writes require `project_id` and `expected_revision`. Planning patches require each field's original `expected_value` as well. Disabling episodes requires one remaining episode and removes season planning/reports; review this before doing it.

## CLI examples

Use the bundled `screenplay` path shown in the app's external-agent connection panel. These examples assume it is on PATH.

```sh
screenplay --help
screenplay status
screenplay workbench '{"action":"overview"}'
screenplay workbench '{"action":"episode","project_id":"PROJECT_ID","episode_id":"EPISODE_ID"}'
screenplay workbench '{"action":"preview_replacement","project_id":"PROJECT_ID","query":"张三","replacement":"李四","scope":"all"}'
```

After reviewing the preview, send its selected occurrence keys and exact revision with `apply_replacement`. Do not guess keys or merely increase revision after a conflict. Re-preview against the new content.

预览只是读取，确认才会修改。遇到冲突重新预览，不能把旧选择套到新稿上。字面匹配会区分大小写，相近名称与上下文仍需审阅。项目记忆、季纲和人物小传不在替换范围中。

```sh
screenplay workbench '{"action":"apply_replacement","project_id":"PROJECT_ID","query":"张三","replacement":"李四","scope":"all","selected":["KEY_FROM_PREVIEW"],"expected_revision":12}'
screenplay workbench '{"action":"edit_episodes","project_id":"PROJECT_ID","operation":"switch","episode_id":"EPISODE_ID","expected_revision":13}'
screenplay workbench '{"action":"save_planning","project_id":"PROJECT_ID","patches":[{"episode_id":"EPISODE_ID","field":"synopsis","value":"New synopsis","expected_value":"Old synopsis"}],"expected_revision":14}'
```

## Saving and checking / 保存与核对

In GUI bridge mode, writes appear in the running app and share its undo history. Save using the normal GUI or save command. The `--project` path does not override the GUI's active project.

When the GUI is not running, CLI write commands require `--project PATH` and save successful changes to that locked project before exiting. MCP is a persistent session; call `save_project` after editing. Save-as acquires the destination lock before writing and binds the new path for later saves.

软件运行时，操作的是 GUI 当前工程，保存方式与界面一致。软件未运行时，CLI 的写命令需指定已保存工程，成功修改会落盘；MCP 会话中的修改请显式保存。保存后重新读取状态，核对工程、当前集、revision 和 dirty。

The built-in model-assisted continuity check is still started from the GUI. External agents can use `read_episode` and planning to conduct their own analysis with their own model; existing reports are available in the overview. No connection is established merely by pasting MCP configuration into ordinary chat.

Standalone CLI processes do not share an in-memory undo stack across invocations. Create a named version before important standalone edits if you need a persistent rollback point. GUI bridge and a live MCP session retain their shared/live undo history.

独立 CLI 每次运行都是新进程，撤销栈不会跨命令保留。重要修改前先创建命名版本，方便恢复；日常交互编辑建议连接运行中的 GUI，或使用持续运行的 MCP 会话。

App 0.1.52 also binds replacement keys to the exact preview parameters and uses file locks in GUI mode. Keep the same query, replacement, scope and revision when applying selected keys. Open/save errors containing PROJECT_IN_USE mean another process owns the file; finish or close that session before trying again. Restart external MCP clients after updating the app so they load the new bundled server.

0.1.52 会把替换位置绑定到预览参数，GUI 也会检查工程文件是否被占用。确认时不要改动查找词、替换词、范围或 revision。遇到 PROJECT_IN_USE，先结束占用文件的会话。更新软件后，重新加载外部客户端的 MCP 服务，以使用新的内置程序。
