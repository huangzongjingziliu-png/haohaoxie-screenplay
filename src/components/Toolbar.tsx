import {tr,fmt} from '@/i18n';
import { useState, useEffect, useRef } from 'react';
import { useEditorStore } from '@/stores/editor';
import { flushPendingEdits } from '@/utils/pendingEdits';
import { ipc, ScreenplayError } from '@/ipc/client';

const EXPORT_FORMATS = [
  { ext: 'pdf', label: 'PDF' },
  { ext: 'docx', label: 'Word (.docx)' },
  { ext: 'fdx', label: 'Final Draft (.fdx)' },
  { ext: 'fountain', label: 'Fountain' },
  { ext: 'txt', label: '纯文本' },
  { ext: 'html', label: 'HTML' },
];

export default function Toolbar({
  onNew,
  onCloseProject,
  onImport,
  onSettings,
  onSearch,
  onVersions,
  onAgent,
  onConnections,
  onBoard,
}: {
  onNew?: () => void;
  onCloseProject?: () => void;
  onImport: () => void;
  onSettings?: () => void;
  onSearch?: () => void;
  onVersions?: () => void;
  onAgent?: () => void;
  onConnections?: () => void;
  onBoard?: () => void;
}) {
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

  const [moreOpen, setMoreOpen] = useState(false);
  const moreRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!moreOpen) return;
    const dismiss = (e: PointerEvent) => { if (!moreRef.current?.contains(e.target as Node)) setMoreOpen(false); };
    const escape = (e: KeyboardEvent) => { if (e.key === 'Escape') { setMoreOpen(false); moreRef.current?.querySelector('button')?.focus(); } };
    document.addEventListener('pointerdown', dismiss); document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('pointerdown', dismiss); document.removeEventListener('keydown', escape); };
  }, [moreOpen]);
  const [busy, setBusy] = useState(false);

  const doSave = async () => {
    setBusy(true);
    await save();
    setBusy(false);
  };

  const doSaveAs = async () => {
    setBusy(true);
    try { await useEditorStore.getState().saveAs(); }
    finally { setBusy(false); }
  };

  const doExport = async (ext: string) => {
    try {
      const { save: dialog } = await import('@tauri-apps/plugin-dialog');
      const name = (project.title || tr("剧本")).replace(/[/\\]/g, '-');
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

  return (
    <div className="toolbar" role="toolbar" aria-label={tr("剧本工具栏")}>
      <div className="toolbar-group">
        <button
          onClick={() => void undo()}
          disabled={!history?.can_undo}
          title={history?.undo_label ? fmt("撤销：{0} (⌘Z)",history.undo_label) : tr("撤销 (⌘Z)")}
        >{tr(" ↶ 撤销 ")}</button>
        <button
          onClick={() => void redo()}
          disabled={!history?.can_redo}
          title={history?.redo_label ? fmt("重做：{0} (⇧⌘Z)",history.redo_label) : tr("重做 (⇧⌘Z)")}
        >{tr(" ↷ 重做 ")}</button>
      </div>

      <div className="toolbar-group">
        <button onClick={() => void doNew()} title={tr("新建项目")}>{tr(" 新建 ")}</button>
        <button onClick={onImport} title={tr("导入剧本文件")}>{tr(" 导入 ")}</button>
        <button
          onClick={() => void doSave()}
          disabled={busy || project.state === 'empty'}
          className="toolbar-save"
          title={tr("保存 (⌘S)")}
        >{tr(" 保存 ")}</button>
      </div>

      <div className="toolbar-title" title={project.path ?? tr("未保存")}>
        {project.title || tr("未命名剧本")}
        {project.dirty && <span className="dirty-dot" title={tr("有未保存的修改")} />}
      </div>

      <div className="toolbar-group">
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
        {viewMode === 'split' && <button onClick={swapEditors} title={tr("交换自由文本与纸面的位置")}>{tr("左右互换 ⇄")}</button>}
        <button onClick={() => setZoom(0)} title={tr("适应纸宽")}>{tr("适应")}</button>
        <button onClick={() => setZoom(Math.max(0.5, (zoom || 1) - 0.1))} title={tr("缩小")}>
          −
        </button>
        <span className="zoom-label">{zoom === 0 ? tr("适应纸宽") : `${Math.round(zoom * 100)}%`}</span>
        <button onClick={() => setZoom(Math.min(2, (zoom || 1) + 0.1))} title={tr("放大")}>
          +
        </button>
      </div>

      <div className="toolbar-group">
        <select
          defaultValue=""
          disabled={busy || project.state === 'empty'}
          onChange={(e) => {
            const v = e.target.value;
            if (v) void doExport(v);
            e.target.value = '';
          }}
          title={tr("导出为…")}
        >
          <option value="">{tr("导出…")}</option>
          {EXPORT_FORMATS.map((f) => (
            <option key={f.ext} value={f.ext}>
              {tr(f.label)}
            </option>
          ))}
        </select>
        {onBoard && <button onClick={onBoard} disabled={project.state === 'empty'} title={tr("场景白板：拖动便签同步调整剧本顺序")}>{tr("白板")}</button>}
        {onAgent && <button onClick={onAgent} title={tr("编剧助理")}>{tr("编剧助理")}</button>}
        {onSearch && <button onClick={onSearch} title={tr("搜索剧本 · ⌘F")}>{tr("⌕ 搜索")}</button>}
        <div className="toolbar-more" ref={moreRef}>
          <button aria-expanded={moreOpen} aria-haspopup="true" onClick={() => setMoreOpen(!moreOpen)}>{tr("更多 ▾")}</button>
          {moreOpen && <div className="toolbar-more-menu" aria-label={tr("更多操作")} onClick={() => setMoreOpen(false)}>
            <button onClick={() => void doSaveAs()} disabled={busy || project.state === 'empty'}>{tr("另存为… ")}<small>⇧⌘S</small></button>
            <button onClick={() => onVersions?.()} disabled={project.state === 'empty'}>{tr("版本历史 ")}<small>⌘Y</small></button>
            {onConnections && <button onClick={onConnections}>{tr("连接外部 Agent…")}</button>}
            {onSettings && <button onClick={onSettings}>{tr("设置… ")}<small>⌘,</small></button>}
            <hr />
            <button onClick={() => onCloseProject ? onCloseProject() : void closeProject()} disabled={project.state === 'empty'}>{tr("关闭当前项目")}</button>
          </div>}
        </div>
      </div>
    </div>
  );
}