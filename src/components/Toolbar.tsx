import UpdateDialog, {UpdateButton,checkUpdates} from './AppUpdates';
import EpisodeControls from './EpisodeControls';
import {tr,fmt} from '@/i18n';
import { useState, useEffect, useRef } from 'react';
import { useEditorStore } from '@/stores/editor';
import { flushPendingEdits } from '@/utils/pendingEdits';
import { ipc, ScreenplayError } from '@/ipc/client';
import { STORE_BUILD } from '@/buildChannel';
import { useEntitlementStore, usePro } from '@/stores/entitlement';

const EXPORT_FORMATS = [
  { ext: 'pdf', label: 'PDF' },
  { ext: 'docx', label: 'Word (.docx)' },
  { ext: 'fdx', label: 'Final Draft (.fdx)', proOnly: true },
  { ext: 'fountain', label: 'Fountain', proOnly: true },
  { ext: 'txt', label: '纯文本' },
  { ext: 'md', label: 'Markdown (.md)' },
  { ext: 'html', label: 'HTML' },
];

export default function Toolbar({
  onHelp,
  onNew,
  onCloseProject,
  onImport,
  onSettings,
  onSearch,
  onVersions,
  onAgent,
  agentOpen,
  onConnections,
  onBoard,
  onToggleNavigator,
  navigatorOpen,
}: {
  onHelp?: () => void;
  onNew?: () => void;
  onCloseProject?: () => void;
  onImport: () => void;
  onSettings?: () => void;
  onSearch?: () => void;
  onVersions?: () => void;
  onAgent?: () => void;
  agentOpen?: boolean;
  onConnections?: () => void;
  onBoard?: () => void;
  onToggleNavigator?: () => void;
  navigatorOpen?: boolean;
}) {
  const draftLabel=useEditorStore(s=>s.document?.draft_label);
  const project = useEditorStore((s) => s.project);
  const viewMode = useEditorStore((s) => s.viewMode);
  const setViewMode = useEditorStore((s) => s.setViewMode);
  const swapEditors = useEditorStore((s) => s.swapEditors);
  const zoom = useEditorStore((s) => s.zoom);
  const setZoom = useEditorStore((s) => s.setZoom);
  const save = useEditorStore((s) => s.save);
  const newProject = useEditorStore((s) => s.newProject);
  const closeProject = useEditorStore((s) => s.closeProject);
  const pushToast = useEditorStore((s) => s.pushToast);
  const undo = useEditorStore((s) => s.undo);
  const redo = useEditorStore((s) => s.redo);
  const history = useEditorStore((s) => s.history);
  const pro = usePro();
  const openPro = useEntitlementStore((s) => s.openDialog);

  const [moreOpen, setMoreOpen] = useState(false);
  const moreRef = useRef<HTMLDivElement>(null);
  const [zoomOpen, setZoomOpen] = useState(false);
  const zoomRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!moreOpen && !zoomOpen) return;
    const dismiss = (e: PointerEvent) => {
      if (!moreRef.current?.contains(e.target as Node)) setMoreOpen(false);
      if (!zoomRef.current?.contains(e.target as Node)) setZoomOpen(false);
    };
    const escape = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      setMoreOpen(false); setZoomOpen(false);
      (moreRef.current ?? zoomRef.current)?.querySelector('button')?.focus();
    };
    document.addEventListener('pointerdown', dismiss); document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('pointerdown', dismiss); document.removeEventListener('keydown', escape); };
  }, [moreOpen, zoomOpen]);
  const [busy, setBusy] = useState(false);

  const doSave = async () => {
    setBusy(true);
    try { await save(); }
    finally { setBusy(false); }
  };

  const doSaveAs = async () => {
    setBusy(true);
    try { await useEditorStore.getState().saveAs(); }
    finally { setBusy(false); }
  };

  // 同时对照两份稿子：在独立的新实例中打开，当前窗口的工程保持不变
  const doOpenInNewWindow = async () => {
    try {
      const { open: dialog } = await import('@tauri-apps/plugin-dialog');
      const path = await dialog({
        multiple: false,
        filters: [{ name: tr("剧本工程"), extensions: ['screenplay'] }],
      });
      if (!path || Array.isArray(path)) return;
      await ipc.openInNewInstance(path);
    } catch (e) {
      pushToast('error', e instanceof ScreenplayError ? e.message : String(e));
    }
  };

  const doExport = async (ext: string) => {
    try {
      const { save: dialog } = await import('@tauri-apps/plugin-dialog');
      const current=useEditorStore.getState().document;
      const episode=current?.series?.episodes.find(e=>e.id===current.series?.active_id);
      const name = ((project.title || tr("剧本"))+(episode?"-"+episode.title:"")+(pro&&current?.draft_label?"-"+current.draft_label:"")).replace(/[/\\]/g, '-');
      const path = await dialog({
        defaultPath: `${name}.${ext}`,
        filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
      });
      if (!path) return;
      setBusy(true);
      await flushPendingEdits();
      await ipc.exportFile(path, ext);
      pushToast('success', fmt("已导出到 {0}",path));
    } catch (e) {
      pushToast('error', e instanceof ScreenplayError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const doNew = async () => {
    try { await flushPendingEdits(); } catch (e) { pushToast('error', String(e)); return; }
    if (useEditorStore.getState().project.dirty && !window.confirm(tr("当前项目有未保存的修改，确定要新建吗？"))) return;
    if (onNew) onNew(); else await newProject(tr("未命名剧本"));
  };

  const zoomText = zoom === 0 ? tr("适应纸宽") : `${Math.round(zoom * 100)}%`;

  return (
    <div className="toolbar" role="toolbar" aria-label={tr("剧本工具栏")}>
      {onToggleNavigator && (
        <button
          className="navigator-toggle"
          aria-label={tr(navigatorOpen ? "折叠导航" : "展开导航")}
          title={tr(navigatorOpen ? "折叠导航" : "展开导航")}
          aria-expanded={navigatorOpen}
          onClick={onToggleNavigator}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8"><rect x="3" y="4" width="18" height="16" rx="3"/><path d="M9 4v16"/></svg>
        </button>
      )}
      <div className="toolbar-title" title={project.path ?? tr("未保存")}>
        {project.title || tr("未命名剧本")}
        {draftLabel&&<small className="draft-badge" title={tr("当前稿号")}>{draftLabel}</small>}
        {project.dirty && <span className="dirty-dot" title={tr("有未保存的修改")} />}
      </div>
      <div className="toolbar-group toolbar-file-group" aria-label={tr("文件")}>
        <button
          onClick={() => void undo()}
          disabled={!history?.can_undo}
          title={history?.undo_label ? fmt("撤销：{0} (⌘Z)",history.undo_label) : tr("撤销 (⌘Z)")}
          aria-label={tr("撤销")}
        >↶</button>
        <button
          onClick={() => void redo()}
          disabled={!history?.can_redo}
          title={history?.redo_label ? fmt("重做：{0} (⇧⌘Z)",history.redo_label) : tr("重做 (⇧⌘Z)")}
          aria-label={tr("重做")}
        >↷</button>
        {onVersions&&<button onClick={onVersions} disabled={project.state==='empty'} title={tr('查看旧稿、比较修改并恢复 · ⌘Y')}>{tr('◷ 历史记录')}</button>}
        <span className="toolbar-divider" aria-hidden="true" />
        <button onClick={() => void doNew()} title={tr("新建项目")}>{tr(" 新建 ")}</button>
        <button onClick={onImport} title={tr("导入剧本文件")}>{tr(" 导入 ")}</button>
        <button
          onClick={() => void doSave()}
          disabled={busy || project.state === 'empty'}
          className="toolbar-save"
          title={tr("保存 (⌘S)")}
        >{tr(" 保存 ")}</button>
        <select
          defaultValue=""
          disabled={busy || project.state === 'empty'}
          onChange={(e) => {
            const v = e.target.value;
            const item = EXPORT_FORMATS.find((f) => f.ext === v);
            if (item?.proOnly && !pro) openPro();
            else if (v) void doExport(v);
            e.target.value = '';
          }}
          title={!pro ? tr("导出为…（Fountain / FDX 为专业版功能）") : tr("导出为…")}
        >
          <option value="">{tr("导出…")}</option>
          {EXPORT_FORMATS.map((f) => (
            <option key={f.ext} value={f.ext}>
              {tr(f.label)}{f.proOnly && !pro ? ` · ${tr("专业版")}` : ''}
            </option>
          ))}
        </select>
        <UpdateButton/><UpdateDialog/>
        <div className="toolbar-more" ref={moreRef}>
          <button aria-expanded={moreOpen} aria-haspopup="true" onClick={() => setMoreOpen(!moreOpen)}>{tr("更多 ▾")}</button>
          {moreOpen && <div className="toolbar-more-menu" aria-label={tr("更多操作")} onClick={() => setMoreOpen(false)}>
            <p className="menu-section-label">{tr("项目")}</p>
            <button onClick={() => void doOpenInNewWindow()} title={tr("保留当前工程，在另一个窗口中打开另一份稿子")}>{tr("在新窗口中打开…")}</button>
            <button onClick={() => void doSaveAs()} disabled={busy || project.state === 'empty'}>{tr("另存为… ")}<small>⇧⌘S</small></button>
            <button onClick={() => onVersions?.()} disabled={project.state === 'empty'}>{tr("版本历史 ")}<small>⌘Y</small></button>
            <p className="menu-section-label">{tr("工具与设置")}</p>
            <button onClick={()=>void checkUpdates(true)}>{tr("检查更新")}</button>
            {STORE_BUILD && <button onClick={() => openPro()}>{pro ? tr("专业版 · 已解锁") : tr("专业版…")}</button>}
            {onHelp && <button onClick={onHelp}>{tr("帮助与教程")}</button>}
            {onSettings && <button onClick={onSettings}>{tr("设置… ")}<small>⌘,</small></button>}
            <hr />
            <button onClick={() => onCloseProject ? onCloseProject() : void closeProject()} disabled={project.state === 'empty'}>{tr("关闭当前项目")}</button>
          </div>}
        </div>
      </div>

      <span className="toolbar-divider toolbar-group-divider" aria-hidden="true" />

      <div className="toolbar-group toolbar-view-group" aria-label={tr("视图")}>
        <select
          value={viewMode}
          onChange={(e) => { const mode = e.target.value as typeof viewMode; void flushPendingEdits().then(() => setViewMode(mode)).catch((error) => pushToast("error", String(error))); }}
          title={tr("视图模式")}
        >
          <option value="split">{tr("左右分栏")}</option>
          <option value="left">{tr("标准剧本")}</option>
          <option value="right">{tr("自由模式")}</option>
          <option value="focus">{tr("专注模式")}</option>
        </select>
        {viewMode === 'split' && <button onClick={swapEditors} aria-label={tr("左右互换 ⇄")} title={tr("交换自由文本与纸面的位置")}>⇄</button>}
        <div className="toolbar-more toolbar-zoom-menu" ref={zoomRef}>
          <button aria-expanded={zoomOpen} aria-haspopup="menu" title={tr("缩放")} onClick={() => setZoomOpen(!zoomOpen)}>{zoom === 0 ? fmt("{0} ▾", zoomText) : fmt("缩放 {0} ▾", zoomText)}</button>
          {zoomOpen && <div className="toolbar-more-menu" role="menu" aria-label={tr("缩放")}>
            <p className="menu-section-label">{fmt("当前缩放 {0}", zoomText)}</p>
            <button role="menuitem" onClick={() => setZoom(0)} title={tr("适应纸宽")}>{tr("适应纸宽")}</button>
            <button role="menuitem" onClick={() => setZoom(Math.max(0.5, (zoom || 1) - 0.1))} title={tr("缩小")}>{tr("缩小")}</button>
            <button role="menuitem" onClick={() => setZoom(Math.min(2, (zoom || 1) + 0.1))} title={tr("放大")}>{tr("放大")}</button>
          </div>}
        </div>
        {onBoard && <button onClick={onBoard} disabled={project.state === 'empty'} title={tr("场景白板：拖动便签同步调整剧本顺序")}>{tr("白板")}</button>}
        {onSearch && <button onClick={onSearch} title={tr("搜索剧本 · ⌘F")}>{tr("⌕ 搜索")}</button>}
      </div>

      <span className="toolbar-divider toolbar-group-divider" aria-hidden="true" />

      <div className="toolbar-group toolbar-collab-group" aria-label={tr("协作")}>
        {onConnections && <button onClick={onConnections} title={tr("连接外部 Agent（MCP / CLI）")}>{tr("连接 Agent…")}</button>}
        {onAgent && <button className={agentOpen ? 'active' : undefined} aria-pressed={agentOpen} onClick={onAgent} title={tr(agentOpen ? "关闭编剧助理" : "编剧助理")}>{tr("编剧助理")}</button>}
      </div>

      <EpisodeControls/>
    </div>
  );
}
