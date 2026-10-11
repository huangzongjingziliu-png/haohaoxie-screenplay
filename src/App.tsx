import {useSplitSync} from '@/hooks/useSplitSync';
import {tr,useUiLanguage} from '@/i18n';
import ModelPicker from '@/components/ModelPicker';
import AgentContextMenu from '@/components/AgentContextMenu';
import AgentSelectionPanel from '@/components/AgentSelectionPanel';
import type {AgentTarget} from '@/utils/agentSelection';
import SceneBoard from '@/components/SceneBoard';
import { useEffect, useRef, useState, useCallback } from 'react';
import NewProjectDialog from '@/components/NewProjectDialog';
import ConnectionPanel from '@/components/ConnectionPanel';
import Toolbar from '@/components/Toolbar';
import Navigator from '@/components/Navigator';
import StatusBar from '@/components/StatusBar';
import ImportReview from '@/components/ImportReview';
import ScreenplayEditor from '@/editors/screenplay/ScreenplayEditor';
import SourceEditor from '@/editors/source/SourceEditor';
import ImportDialog from '@/components/ImportDialog';
import SettingsPanel from '@/components/UnifiedSettings';
import SearchBar from '@/components/SearchBar';
import Toasts from '@/components/Toasts';
import RecoveryPrompt from '@/components/RecoveryPrompt';
import VersionPanel from '@/components/VersionPanel';
import AgentPanel from '@/components/AgentPanel';
import { useEditorStore } from '@/stores/editor';
import { flushPendingEdits } from '@/utils/pendingEdits';
import { useAutoSave } from '@/hooks/useAutoSave';
import { ipc, onDocumentChanged, onProjectChanged, onCloseRequested } from '@/ipc/client';

export default function App() {
  useUiLanguage();
  useSplitSync();
  const project = useEditorStore((s) => s.project);
  const viewMode = useEditorStore((s) => s.viewMode);
  const pushToast = useEditorStore((s) => s.pushToast);
  const sourceFirst = useEditorStore((s) => s.sourceFirst);
  const exitFocus = useEditorStore((s) => s.exitFocus);
  const leftSplitRatio = useEditorStore((s) => s.leftSplitRatio);
  const setLeftSplitRatio = useEditorStore((s) => s.setLeftSplitRatio);
  const [showBoard, setShowBoard] = useState(false);
  const showNavigator = useEditorStore((s) => s.showNavigator);
  const setShowNavigator = useEditorStore((s) => s.setShowNavigator);
  const setViewMode = useEditorStore((s) => s.setViewMode);
  const bootstrap = useEditorStore((s) => s.bootstrap);
  const refreshProjectInfo = useEditorStore((s) => s.refreshProjectInfo);

  // 自动保存 / 自动版本
  useAutoSave();

  const [showNew, setShowNew] = useState(false);
  const [exitTarget, setExitTarget] = useState<"quit" | "project">("quit");
  const [showExit, setShowExit] = useState(false);
  const [exiting, setExiting] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [showImportReview, setShowImportReview] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [settingsTab,setSettingsTab]=useState('text');
  const [showSearch,setShowSearch]=useState(false);
  useEffect(()=>{const open=()=>{setSettingsTab('text');setShowSettings(true);};const models=()=>{setSettingsTab('models');setShowSettings(true);};window.addEventListener('open-settings',open);window.addEventListener('open-model-settings',models);return()=>{window.removeEventListener('open-settings',open);window.removeEventListener('open-model-settings',models);};},[]);
  const [showVersions, setShowVersions] = useState(false);
  const [showConnections, setShowConnections] = useState(false);
  const [showAgent, setShowAgent] = useState(false);
  const [agentTarget,setAgentTarget]=useState<AgentTarget|null>(null);
  useEffect(()=>{const receive=(e:Event)=>{setAgentTarget((e as CustomEvent<AgentTarget>).detail);setShowBoard(false);};window.addEventListener('agent-selection',receive);return()=>window.removeEventListener('agent-selection',receive);},[]);
  const [recent, setRecent] = useState<{ path: string; title: string }[]>([]);

  // 启动时从后端拉取权威状态
  useEffect(() => {
    let disposed = false;
    let checking = false;
    const check = async () => {
      if (checking || disposed) return;
      checking = true;
      try {
        const paths = await ipc.takeFileOpenRequests();
        for (const path of paths) {
          await flushPendingEdits();
          await refreshProjectInfo();
          const state = useEditorStore.getState();
          if (state.project.path === path) continue;
          if (state.project.dirty) { pushToast('info', tr("当前剧本有未保存的修改，请先保存或关闭，再双击目标工程文件。")); continue; }
          await state.openProject(path);
        }
        setRecent(await ipc.listRecent());
      } catch (e) { if (!disposed) pushToast('error', String(e)); }
      finally { checking = false; }
    };
    let timer: ReturnType<typeof setInterval> | undefined;
    void bootstrap().then(() => { if (!disposed) { void check(); timer = setInterval(() => void check(), 1500); } });
    return () => { disposed = true; if (timer) clearInterval(timer); };
  }, [bootstrap, refreshProjectInfo, pushToast]);

  // 订阅后端变更事件（为阶段 C 的外部写入实时同步预留）
  useEffect(() => {
    const unsubs: Array<() => void> = [];
    let disposed = false;
    const register = (u: () => void) => { if (disposed) u(); else unsubs.push(u); };
    void onDocumentChanged(() => {
      void useEditorStore.getState().refreshDocument();
    }).then(register);
    void onProjectChanged(() => {
      void useEditorStore.getState().refreshDocument();
    }).then(register);
    void onCloseRequested(() => {
      void (async () => {
        try {
          await flushPendingEdits();
          await refreshProjectInfo();
          if (useEditorStore.getState().project.dirty) { setExitTarget("quit"); setShowExit(true); }
          else await ipc.confirmClose();
        } catch (e) { useEditorStore.getState().pushToast('error', String(e)); }
      })();
    }).then(register);
    return () => { disposed = true; unsubs.forEach((u) => u()); };
  }, [refreshProjectInfo]);

  useEffect(() => {
    const open = () => setShowImportReview(true);
    window.addEventListener('import-review-requested', open);
    return () => window.removeEventListener('import-review-requested', open);
  }, []);

  // 定期同步 dirty 状态（保存进行中时也能正确反映）
  useEffect(() => {
    const t = window.setInterval(() => void refreshProjectInfo(), 2000);
    return () => window.clearInterval(t);
  }, [refreshProjectInfo]);

  // 键盘快捷键
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && useEditorStore.getState().viewMode === 'focus') {
        exitFocus();
        return;
      }
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      const target = e.target as HTMLElement | null;
      if (target?.closest('.modal, .agent-panel') && ['z', 'y', 's'].includes(e.key.toLowerCase())) return;
      if (e.key === 'z') {
        e.preventDefault();
        const st = useEditorStore.getState();
        if (e.shiftKey) void st.redo();
        else void st.undo();
        return;
      }
      if (e.key === 'y') {
        e.preventDefault();
        setShowVersions(true);
        return;
      }
      if (e.key === 's') {
        e.preventDefault();
        const { save, saveAs } = useEditorStore.getState();
        if (e.shiftKey) void saveAs();
        else void save();
      }
      if (e.key.toLowerCase() === 'f') {e.preventDefault();setShowSearch(true);return;}
      if (e.key === ',') {
        e.preventDefault();
        setShowSettings(true);
      }
      if (e.key === 'b') {
        e.preventDefault();
        setShowNavigator(!showNavigator);
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [showNavigator]);

  // 拖拽分隔条
  const resizerRef = useRef<HTMLDivElement>(null);
  const onResizerMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const startX = e.clientX;
      const container = resizerRef.current;
      if (!container) return;
      const startWidth = container.offsetWidth;
      const onMove = (ev: MouseEvent) => {
        const dx = ev.clientX - startX;
        setLeftSplitRatio((leftSplitRatio * startWidth + (sourceFirst ? -dx : dx)) / startWidth);
      };
      const onUp = () => {
        window.removeEventListener('mousemove', onMove);
        window.removeEventListener('mouseup', onUp);
      };
      window.addEventListener('mousemove', onMove);
      window.addEventListener('mouseup', onUp);
    },
    [leftSplitRatio, setLeftSplitRatio, sourceFirst]
  );

  const finishClose = async (discard = false) => {
    if (exitTarget === 'quit') await ipc.confirmClose(discard);
    else { await ipc.closeProject(discard); await useEditorStore.getState().bootstrap(); setShowExit(false); }
  };
  const requestProjectClose = async () => {
    try {
      await flushPendingEdits(); await refreshProjectInfo(); setExitTarget('project');
      if (useEditorStore.getState().project.dirty) setShowExit(true);
      else { await ipc.closeProject(); await useEditorStore.getState().bootstrap(); }
    } catch (e) { pushToast('error', String(e)); }
  };
  const exitDialog = showExit && (
    <div className="modal-backdrop"><section className="modal" role="dialog" aria-modal="true" aria-labelledby="exit-title">
      <h2 id="exit-title">{tr(exitTarget === 'quit' ? "保存修改后退出？" : "保存修改后关闭？")}</h2>
      <p className="modal-hint">{tr("不保存会放弃尚未落盘的修改，不保留本次恢复副本。已经保存到文件的内容不受影响。")}</p>
      <div className="modal-actions">
        <button disabled={exiting} onClick={() => setShowExit(false)}>{tr("继续编辑")}</button>
        <button disabled={exiting} onClick={async () => { setExiting(true); try { await finishClose(true); } catch (e) { pushToast('error', String(e)); } finally { setExiting(false); } }}>{tr("不保存")}</button>
        <button disabled={exiting} onClick={async () => { setExiting(true); try { await finishClose(); } catch (e) { pushToast("error", String(e)); } finally { setExiting(false); } }}>{tr(exitTarget === 'quit' ? "保留恢复副本并退出" : "保留恢复副本并关闭")}</button>
        <button className="primary" disabled={exiting} onClick={async () => {
          setExiting(true); try {
            const saved = await useEditorStore.getState().save();
            if (saved && !useEditorStore.getState().project.dirty) await finishClose();
          } finally { setExiting(false); }
        }}>{tr(exitTarget === 'quit' ? "保存并退出" : "保存并关闭")}</button>
      </div>
    </section></div>
  );

  // Keep import state mounted when an empty workspace becomes a project.
  const content = project.state === 'empty' ? (
      <div className="app">
        <Toolbar onNew={() => setShowNew(true)} onImport={() => setShowImport(true)} onSettings={()=>{setSettingsTab('text');setShowSettings(true);}} />
        <Welcome onNew={() => setShowNew(true)} onConnections={() => setShowConnections(true)} recent={recent} onOpened={() => void ipc.listRecent().then(setRecent)} />
        <div className="status-bar"><span className="spacer" /><ModelPicker /></div>
      </div>
    ) : renderEditor();

  return <>{content}{showImport && <ImportDialog onClose={() => setShowImport(false)} />}<Toasts /><RecoveryPrompt />{showNew && <NewProjectDialog onClose={() => setShowNew(false)} onCreated={() => void ipc.listRecent().then(setRecent)} />}{showImportReview && <ImportReview onClose={() => setShowImportReview(false)} />}{exitDialog}{project.state === 'empty' && showSettings && <SettingsPanel initial={settingsTab} onClose={()=>setShowSettings(false)}/ >}{showConnections && <ConnectionPanel onClose={() => setShowConnections(false)} />}</>;

  function renderEditor() {

  const showLeft = viewMode === 'split' || viewMode === 'left' || viewMode === 'focus';
  const showRight = viewMode === 'split' || viewMode === 'right';
  const showNav = viewMode !== 'focus' && showNavigator;

  return (
    <div className="app">
      {viewMode !== 'focus' && (
        <Toolbar
          onNew={() => setShowNew(true)}
          onCloseProject={() => void requestProjectClose()}
          onImport={() => setShowImport(true)}
          onSettings={() => {setSettingsTab('text');setShowSettings(true);}}
          onSearch={()=>setShowSearch(true)}
          onVersions={() => setShowVersions(true)}
          onBoard={() => setShowBoard(true)}
          onAgent={() => setShowAgent(true)}
          onConnections={() => setShowConnections(true)}
        />
      )}
      {showSearch && <SearchBar onClose={()=>setShowSearch(false)}/>}
      <div className="main">
        {showNav && <Navigator />}
        <div className="editors" ref={resizerRef}>
          {showLeft && (
            <div className="editor-pane" style={{ flex: showRight ? leftSplitRatio : 1, order: sourceFirst ? 2 : 0 }}>
              {viewMode !== "focus" && <div className="pane-heading"><strong>{tr("标准剧本 · 纸面排版")}</strong><button onClick={() => void flushPendingEdits().then(() => setViewMode("right")).catch((error) => pushToast("error", String(error)))} title={tr("隐藏纸面，保留自由文本")}>{tr("关闭纸面 ×")}</button></div>}
              <ScreenplayEditor />
            </div>
          )}
          {showLeft && showRight && (
            <div className="resizer" style={{ order: 1 }} onMouseDown={onResizerMouseDown} />
          )}
          {showRight && (
            <div className="editor-pane" style={{ flex: showLeft ? 1 - leftSplitRatio : 1, order: sourceFirst ? 0 : 2 }}>
              <div className="pane-heading"><strong>{tr("自由文本 · 写作")}</strong><button onClick={() => void flushPendingEdits().then(() => setViewMode("left")).catch((error) => pushToast("error", String(error)))} title={tr("隐藏自由文本，保留纸面")}>{tr("关闭文本 ×")}</button></div>
              <SourceEditor />
            </div>
          )}
        </div>
      </div>
      <StatusBar />
      {showBoard && <SceneBoard onJump={id=> {setShowBoard(false);setViewMode('left');useEditorStore.getState().setActiveBlock(id);let attempts=0;const jump=()=>{const el=document.querySelector<HTMLElement>(`.screenplay-editor [data-block-id="${id}"] .screenplay-block`);if(el){el.scrollIntoView({block:'center',behavior:'smooth'});el.focus();}else if(attempts++<30)requestAnimationFrame(jump);};requestAnimationFrame(jump);}} onClose={() => setShowBoard(false)} onConfigure={() => {setShowBoard(false);setSettingsTab('models');setShowSettings(true);}} />}
      {viewMode === 'focus' && <button className="exit-focus" onClick={exitFocus}>{tr("退出专注 · 返回原界面（Esc）")}</button>}
      {showSettings && <SettingsPanel initial={settingsTab} onClose={() => setShowSettings(false)} />}
      {showVersions && <VersionPanel onClose={() => setShowVersions(false)} />}
      <AgentContextMenu/>
      {agentTarget && <AgentSelectionPanel target={agentTarget} onClose={()=>setAgentTarget(null)} onConfigure={()=>{setAgentTarget(null);setSettingsTab('models');setShowSettings(true);}}/>}
      {showAgent && viewMode !== 'focus' && <AgentPanel onClose={() => setShowAgent(false)} />}
    </div>
  );
  }
}

function Welcome({
  onNew,
  onConnections,
  recent,
  onOpened,
}: {
  onNew: () => void;
  recent: { path: string; title: string }[];
  onConnections: () => void;
  onOpened: () => void;
}) {
  const openProject = useEditorStore((s) => s.openProject);
  return (
    <div className="welcome">
      <h1>{tr("好好写剧本")}</h1>
      <p style={{ color: '#86868b' }}>{tr("让作者用最舒服的方法写，让软件负责把它变成专业剧本")}</p>
      <div className="welcome-actions">
        <button onClick={onConnections}>{tr("连接外部 Agent，让它操作好好写剧本")}</button>
        <button onClick={onNew}>{tr("新建剧本")}</button>
        <button onClick={async () => {
          const { open } = await import('@tauri-apps/plugin-dialog');
          const path = await open({ multiple: false, filters: [{ name: '好好写剧本剧本工程', extensions: ['screenplay'] }] });
          if (typeof path === 'string') { await openProject(path); onOpened(); }
        }}>{tr("打开剧本…")}</button>
      </div>
      {(
        <div className="recent-list">
          <h3 style={{ marginBottom: 8, fontSize: 13, color: '#86868b' }}>{tr("最近项目")}</h3>
          {recent.map((r) => (
            <button
              type="button" title={r.path}
              key={r.path}
              className="recent-item"
              onClick={async () => {
                await openProject(r.path);
                onOpened();
              }}
            >
              <span>{r.title || tr("未命名剧本")}</span>
              <span style={{ color: '#86868b', fontSize: 11 }}>
                {r.path.split('/').pop()}
              </span>
            </button>
          ))}
          {recent.length === 0 && <p>{tr("还没有最近项目。新建并保存，或打开已有 .screenplay 工程后，会显示在这里。")}</p>}
        </div>
      )}
    </div>
  );
}