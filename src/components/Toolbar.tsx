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

/** 工具栏图标：内联 SVG，不引第三方图标库；颜色继承 currentColor。 */
function ToolIcon({ name }: { name: 'history' | 'new' | 'import' | 'save' | 'more' | 'board' | 'search' | 'connect' | 'agent' }) {
  const common = { width: 17, height: 17, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', strokeWidth: 1.8, strokeLinecap: 'round' as const, strokeLinejoin: 'round' as const, 'aria-hidden': true };
  switch (name) {
    case 'history':
      return <svg {...common}><circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/></svg>;
    case 'new':
      return <svg {...common}><path d="M12 5v14M5 12h14"/></svg>;
    case 'import':
      return <svg {...common}><path d="M12 3v11"/><path d="m8 10 4 4 4-4"/><path d="M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2"/></svg>;
    case 'save':
      return <svg {...common}><path d="M5 3h11l3 3v15H5z"/><path d="M8 3v6h8V3"/><path d="M8 21v-7h8v7"/></svg>;
    case 'more':
      return <svg {...common}><circle cx="5" cy="12" r="1.2" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1.2" fill="currentColor" stroke="none"/><circle cx="19" cy="12" r="1.2" fill="currentColor" stroke="none"/></svg>;
    case 'board':
      return <svg {...common}><rect x="3.5" y="4" width="7" height="7" rx="1.5"/><rect x="13.5" y="4" width="7" height="7" rx="1.5"/><rect x="3.5" y="14" width="7" height="6" rx="1.5"/><rect x="13.5" y="14" width="7" height="6" rx="1.5"/></svg>;
    case 'search':
      return <svg {...common}><circle cx="11" cy="11" r="6.5"/><path d="m16 16 4.5 4.5"/></svg>;
    case 'connect':
      return <svg {...common}><path d="M9 15 15 9"/><path d="M11 6.5 12.5 5a4 4 0 0 1 5.7 5.7L16.5 12"/><path d="M13 17.5 11.5 19a4 4 0 0 1-5.7-5.7L7.5 12"/></svg>;
    case 'agent':
      return <svg {...common}><path d="m12 3 1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z"/><path d="m18.5 16.5.7 2 2 .7-2 .7-.7 2-.7-2-2-.7 2-.7z"/></svg>;
  }
}

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
        {onVersions&&<button className="toolbar-icon-btn" onClick={onVersions} disabled={project.state==='empty'} title={tr('查看旧稿、比较修改并恢复 · ⌘Y')} aria-label={tr('历史记录')}><ToolIcon name="history"/></button>}
        <span className="toolbar-divider" aria-hidden="true" />
        <button className="toolbar-icon-btn" onClick={() => void doNew()} title={tr("新建项目")} aria-label={tr("新建项目")}><ToolIcon name="new"/></button>
        <button className="toolbar-icon-btn" onClick={onImport} title={tr("导入剧本文件")} aria-label={tr("导入剧本文件")}><ToolIcon name="import"/></button>
        <button
          onClick={() => void doSave()}
          disabled={busy || project.state === 'empty'}
          className="toolbar-icon-btn toolbar-save"
          title={tr("保存 (⌘S)")}
          aria-label={tr("保存")}
        ><ToolIcon name="save"/></button>
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
          <button className="toolbar-icon-btn" aria-expanded={moreOpen} aria-haspopup="true" aria-label={tr("更多操作")} title={tr("更多操作")} onClick={() => setMoreOpen(!moreOpen)}><ToolIcon name="more"/></button>
          {moreOpen && <div className="toolbar-more-menu" aria-label={tr("更多操作")} onClick={() => setMoreOpen(false)}>
            <p className="menu-section-label">{tr("项目")}</p>
            <button onClick={() => void doOpenInNewWindow()} title={tr("保留当前工程，在另一个窗口中打开另一份稿子")}>{tr("在新窗口中打开…")}</button>
            <button onClick={() => void doSaveAs()} disabled={busy || project.state === 'empty'}>{tr("另存为… ")}<small>⇧⌘S</small></button>
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
        {onBoard && <button className="toolbar-icon-btn" onClick={onBoard} disabled={project.state === 'empty'} title={tr("场景白板：拖动便签同步调整剧本顺序")} aria-label={tr("场景白板")}><ToolIcon name="board"/></button>}
        {onSearch && <button className="toolbar-icon-btn" onClick={onSearch} title={tr("搜索剧本 · ⌘F")} aria-label={tr("搜索剧本")}><ToolIcon name="search"/></button>}
      </div>

      <span className="toolbar-divider toolbar-group-divider" aria-hidden="true" />

      <div className="toolbar-group toolbar-collab-group" aria-label={tr("协作")}>
        {onConnections && <button className="toolbar-icon-btn" onClick={onConnections} title={tr("连接外部 Agent（MCP / CLI）")} aria-label={tr("连接外部 Agent")}><ToolIcon name="connect"/></button>}
        {onAgent && <button className={`toolbar-icon-btn ${agentOpen ? 'active' : ''}`} aria-pressed={agentOpen} onClick={onAgent} title={tr(agentOpen ? "关闭编剧助理" : "编剧助理")} aria-label={tr("编剧助理")}><ToolIcon name="agent"/></button>}
      </div>

      <EpisodeControls/>
    </div>
  );
}
