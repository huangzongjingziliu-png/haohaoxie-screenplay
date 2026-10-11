import ModalBackdrop from './ModalBackdrop';
import {tr, effectiveLanguage} from '@/i18n';
import { useEffect, useState } from 'react';
import { ipc } from '@/ipc/client';

const quote = (value: string) => "'" + value.replace(/'/g, "'\\''") + "'";
export default function ConnectionPanel({ onClose }: { onClose: () => void }) {
  const [info, setInfo] = useState<{ cli: string; mcp: string; available: boolean; installDir: string } | null>(null);
  const [client, setClient] = useState('codex');
  const [message, setMessage] = useState('');
  const [busy, setBusy] = useState(false);
  const [localCheck, setLocalCheck] = useState(tr("尚未检查"));
  useEffect(() => { void ipc.agentConnectionInfo().then(setInfo).catch((e) => setMessage(String(e))); }, []);
  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); onClose(); } };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onClose]);
  const config = !info ? '' : client === 'codex' ? `[mcp_servers.muben]\ncommand = ${JSON.stringify(info.mcp)}\nargs = []` : JSON.stringify({ mcpServers: { muben: { command: info.mcp, args: [] } } }, null, 2);
  const conversation = !info ? '' : effectiveLanguage() === 'en' ? `Use the local tools bundled with Write Your Script to inspect the screenplay currently open in the app. Codex CLI and Rust are not required.

First run:
${quote(info.cli)} --help
${quote(info.cli)} status

Confirm status reports bridge(GUI 当前项目), and confirm the project name and path with me. Then use info, scenes and blocks to provide a brief summary. This request is read-only: do not modify or save the screenplay. For later explicitly requested edits, read the latest revision before writing; on conflict read again without overwriting concurrent edits. Do not create remote connections or change client configuration.

If you cannot execute local commands on this Mac, tell me to switch to a local working environment. Do not claim a connection was established.` : `请通过本机好好写剧本自带的工具，读取好好写剧本当前打开的剧本，帮我写作和修改。无需安装 Codex CLI，也无需安装 Rust。

先执行：
${quote(info.cli)} --help
${quote(info.cli)} status

确认 status 显示 bridge(GUI 当前项目)，向我确认项目名称和路径，再用 info、scenes、blocks 读取剧本并给出简要摘要。此次只读，不要修改或保存剧本。后续我明确提出修改要求时，再按帮助文档操作；写入前读取最新 revision，遇到冲突重新读取，不覆盖其他修改。不要自动创建远程连接或修改客户端配置。

如果你没有本地文件和命令执行能力，请明确告诉我需要切换到本地工作模式；不要声称已经连接。`;
  const run = async (action: () => Promise<string>) => {
    setBusy(true);
    try { setMessage(await action()); } catch (e) { setMessage(String(e)); }
    finally { setBusy(false); }
  };
  return <ModalBackdrop onDismiss={onClose}>
    <section className="modal connection-panel" role="dialog" aria-modal="true" aria-labelledby="connection-title" onClick={(e) => e.stopPropagation()}>
      <h2 id="connection-title">{tr("连接外部 Agent，让它操作好好写剧本")}</h2>
      <p>{tr("把好好写剧本接入你使用的 Agent，然后直接让它帮你写作、修改。CLI 和 MCP 都已内置，无需安装 Codex CLI。")}</p>
      <div className="connection-status-card">
        <strong>{tr("本机工具：")}{!info ? tr("正在读取…") : info.available ? tr("已就绪") : tr("不可用")}</strong>
        <span>{tr("外部 Agent：连接状态尚未确认")}</span>
        <small>{tr("好好写剧本只能检查本机工具；是否接通，需要外部 Agent 成功读取当前剧本后确认。")}</small>
        <button disabled={!info?.available || busy} onClick={() => void run(async () => { const result = await ipc.checkAgentConnection(); setLocalCheck(result); return tr("本机检查完成；这不代表外部 Agent 已连接。"); })}>{tr("检查本机工具与当前项目")}</button>
        <pre>{localCheck}</pre>
      </div>
      <label>{tr("你使用的客户端")}<select value={client} onChange={(e) => { setClient(e.target.value); setMessage(''); }}><option value="codex">Codex App</option><option value="trae">TRAE Work</option><option value="workbuddy">WorkBuddy</option></select></label>
      <div className="connection-method">
        <h3>{tr("① 配置 MCP · 推荐长期使用")}</h3>
        <p>{tr("先在好好写剧本打开目标剧本，再到外部客户端的 MCP 设置添加本地服务。配置一次，之后直接在对话中提出写作要求。")}</p>
        <p>{client === 'codex' ? tr("在 Codex App 设置的 MCP 服务器中添加本地 STDIO 服务：名称 muben，命令使用下方路径，参数留空。也可将 TOML 合并到 ~/.codex/config.toml。") : client === 'workbuddy' ? tr("在 WorkBuddy 的 MCP 服务器设置中合并下面 JSON 配置。") : tr("在 TRAE Work 的 MCP 设置中添加本地 stdio 服务，合并下面 JSON 配置。")}{tr("保留已有服务，保存后按客户端提示重新加载。")}</p>
        {info && <div className="connection-command"><code>{info.mcp}</code><button disabled={!info.available || busy} onClick={() => void run(async () => { await navigator.clipboard.writeText(info.mcp); return tr("服务路径已复制，请填写到客户端 MCP 设置的命令栏。"); })}>{tr("复制服务路径")}</button></div>}
        <textarea aria-label={tr("MCP接入配置")} readOnly value={config} rows={5} onFocus={(e) => e.target.select()} />
        <button disabled={!info?.available || busy} onClick={() => void run(async () => { await navigator.clipboard.writeText(config); return tr("配置已复制，请添加到客户端 MCP 设置；尚未建立连接。"); })}>{tr("复制 MCP 配置")}</button>
        <p className="connection-note">{tr("配置文字不能直接粘贴到聊天框来启用 MCP。当前服务只支持同一台 Mac 的本地连接。")}</p>
      </div>
      <div className="connection-method">
        <h3>{tr("② 在外部 Agent 中确认连接")}</h3>
        <p>{tr("配置完成后，对 Agent 说：“请使用好好写剧本 MCP，只读查看当前项目，告诉我项目名称、路径和场景数量，暂时不要修改。”能正确返回这些信息，才算接通。")}</p>
        <p className="connection-note">{tr("如果提示“没有那个文件或目录”，先确认 Agent 是否运行在这台 Mac 的本地工作模式。云端或隔离环境访问不到本机路径，换成 CLI 也不能解决。")}</p>
      </div>
      <details>
        <summary>{tr("临时接入：通过 CLI 复制操作说明")}</summary>
        <p>{tr("适合能执行这台 Mac 本地命令的 Agent。复制的是操作请求，不会自动配置 MCP，也不会授予电脑访问能力。")}</p>
        <textarea aria-label={tr("给外部 Agent 的接入说明")} readOnly value={conversation} rows={7} onFocus={(e) => e.target.select()} />
        <button disabled={!info?.available || busy} onClick={() => void run(async () => { await navigator.clipboard.writeText(conversation); return tr("操作说明已复制；请粘贴到能执行本机命令的 Agent，等待它确认当前项目。"); })}>{tr("复制给 Agent 对话框")}</button>
      </details>
      <details>
        <summary>{tr("终端工具（可选）")}</summary>
        {info && <><p>{tr("CLI可直接执行：")}</p><code>{quote(info.cli)} status</code><p>{tr("终端命令安装目录：")}{info.installDir}{tr("。将它加入 PATH 后可使用 screenplay --help。")}</p></>}
        <button disabled={!info?.available || busy} onClick={() => void run(ipc.installAgentCli)}>{tr("安装终端命令")}</button>
      </details>
      <p>{tr("复制说明只是向 Agent 发出操作请求，不代表已连接。请让 Agent 先确认当前项目。另一台 Mac 安装后重新复制那台机器生成的说明。")}</p>
      <pre role="status">{message}</pre>
      <div className="modal-actions"><button onClick={onClose}>{tr("关闭")}</button></div>
    </section>
  </ModalBackdrop>;
}
