import ModalBackdrop from './ModalBackdrop';
import {tr,fmt} from '@/i18n';
import AgentWorkStatus from './AgentWorkStatus';
import { useRef, useState } from 'react';
import { useEditorStore } from '@/stores/editor';
import { ipc, ScreenplayError, onImportProgress } from '@/ipc/client';
import { flushPendingEdits } from '@/utils/pendingEdits';

/**
 * 导入对话框。
 *
 * 所有格式统一走 `ipc.importFile(path)` → Rust `screenplay_import`。
 * 前端**不做任何解析**（原实现在前端解析，导致两套逻辑）。
 */
const FORMATS = [
  { ext: 'pdf', label: 'PDF（含扫描件 OCR）', group: '文档' },
  { ext: 'docx', label: 'Word (.docx)', group: '文档' },
  { ext: 'rtf', label: 'RTF', group: '文档' },
  { ext: 'fdx', label: 'Final Draft (.fdx)', group: '文档' },
  { ext: 'fountain', label: 'Fountain', group: '文本' },
  { ext: 'txt', label: '纯文本', group: '文本' },
  { ext: 'md', label: 'Markdown', group: '文本' },
  { ext: 'png', label: '图片（OCR）', group: '图片' },
  { ext: 'jpg', label: '图片（OCR）', group: '图片' },
  { ext: 'heic', label: 'HEIC 图片（OCR）', group: '图片' },
  { ext: 'tiff', label: 'TIFF 图片（OCR）', group: '图片' },
];

export default function ImportDialog({ onClose }: { onClose: () => void }) {
  const applyResult = useEditorStore((s) => s.applyResult);
  const newProject = useEditorStore((s) => s.newProject);
  const pushToast = useEditorStore((s) => s.pushToast);
  const request = useRef<string | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [pageProgress, setPageProgress] = useState<{ completed: number; total: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [unit,setUnit]=useState(tr("页"));
  const [error,setError]=useState<string|null>(null);
  const [progress, setProgress] = useState<string | null>(null);

  const runImport = async (path: string) => {
    setBusy(true);setError(null);
    setProgress(fmt("正在导入 {0} …",path.split('/').pop()));
    const id = crypto.randomUUID();
    request.current = id;
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await onImportProgress((event) => {
        if (event.requestId !== request.current) return;
        const labels = { credential: '正在检查模型配置…', reading: '正在读取文件…', parsing: '正在解析剧本…', pdf_pages: '正在提取 PDF 页面…', ocr: '正在识别扫描文字…', ready: '正在整理剧本…', agent: 'Agent 正在转换标准剧本…' };
        setProgress(labels[event.stage]);setUnit(event.stage==='agent'?tr("段"):tr("页"));
        setPageProgress(event.total > 0 ? { completed: event.completed, total: event.total } : null);
      });
      await flushPendingEdits();
      const r = await ipc.importFile(path, id);
      await applyResult(r);
      if (r.document.import_info && (r.document.blocks.some((b) => b.confidence > 0 && b.confidence < 0.75) || r.document.import_info.removed_noise.length > 0)) {
        window.dispatchEvent(new Event('import-review-requested'));
      }
      pushToast(
        'success',
        fmt("导入完成：{0} 个元素（rev {1}）",r.document.blocks.length,r.revision)
      );
      onClose();
    } catch (e) {
      const message=e instanceof ScreenplayError?e.message:String(e);
      setError(message.includes('免费模型请求额度') ? fmt("{0} 保持好好写剧本打开，使用同一模型再次导入同一文件可复用已完成分段；退出软件会清除临时进度。",message) : message);
      // 失败不破坏当前文档（Rust 侧导入是原子的）
      if (e instanceof ScreenplayError && e.message === tr("导入已取消")) {
        pushToast('info', tr("已取消导入，当前剧本未改变"));
      } else pushToast('error', e instanceof ScreenplayError ? e.message : String(e),
        e instanceof ScreenplayError ? e.code : undefined);
    } finally {
      unlisten?.();
      request.current = null;
      setBusy(false);
      setCancelling(false);
      setProgress(null);
      setPageProgress(null);
    }
  };

  const cancel = async () => {
    if (!request.current || cancelling) return;
    setCancelling(true);
    try {
      const accepted = await ipc.cancelImport(request.current);
      if (accepted) setProgress(tr("正在取消导入…"));
      else { setCancelling(false); pushToast('info', tr("导入已结束或尚未开始")); }
    } catch (e) { setCancelling(false); pushToast('error', String(e)); }
  };

  const pick = async (exts: string[], label: string) => {
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({
        multiple: false,
        filters: [{ name: label, extensions: exts }],
      });
      if (!selected) return;
      const path = Array.isArray(selected) ? selected[0] : selected;
      await flushPendingEdits();
      const current = useEditorStore.getState();
      if (current.project.dirty && current.project.blockCount > 0) {
        const { confirm } = await import('@tauri-apps/plugin-dialog');
        if (!await confirm(tr("导入会替换当前剧本。是否先保存当前内容？"), { title: '导入剧本', kind: 'warning' })) return;
        await current.save();
        if (useEditorStore.getState().project.dirty) return;
      }
      // 若还没有项目，先建一个承载导入结果
      const st = useEditorStore.getState().project.state;
      if (st === 'empty') {
        await newProject(path.split('/').pop() ?? tr("导入的剧本"));
      }
      await runImport(path);
    } catch (e) {
      pushToast('error', String(e));
    }
  };

  const groups = ["文档", "文本", "图片"] as const;

  return (
    <ModalBackdrop onDismiss={onClose} disabled={busy} message={tr("放弃当前导入选项并关闭？")}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>{tr("导入剧本")}</h2>
        <p className="modal-hint">{tr(" PDF和图片默认通过文字识别和内置Agent转换Skill整理为标准剧本，请先配置模型。识别文字会发送给您配置的模型。 ")}</p>

        {error&&<p role="alert" className="import-error">{error}</p>}
        {busy&&<AgentWorkStatus label={cancelling?tr("正在取消导入…"):progress??tr("正在准备导入…")} completed={pageProgress?.completed} total={pageProgress?.total} unit={unit} onCancel={()=>void cancel()} cancelling={cancelling}/>}
        {progress && !busy && <div className="modal-progress" role="status">{cancelling ? tr("正在取消导入…") : progress}
          {pageProgress && <><progress value={pageProgress.completed} max={pageProgress.total} aria-label={tr("PDF 页面进度")} /><span>{pageProgress.completed} / {pageProgress.total} {unit}</span></>}
        </div>}

        <div className="import-groups">
          {groups.map((g) => (
            <div key={g} className="import-group">
              <h3>{tr(g)}</h3>
              {FORMATS.filter((f) => f.group === g).map((f) => (
                <button
                  key={f.ext}
                  disabled={busy}
                  onClick={() => void pick([f.ext], tr(f.label))}
                >
                  {tr(f.label)}
                </button>
              ))}
            </div>
          ))}
        </div>

        <div className="modal-actions">
          <button
            disabled={busy}
            onClick={() =>
              void pick(
                ['pdf', 'docx', 'rtf', 'fdx', 'fountain', 'txt', 'md', 'png', 'jpg', 'heic', 'tiff'],
                tr("所有支持的格式")
              )
            }
          >{tr(" 浏览文件… ")}</button>
          {!busy && <button onClick={onClose}>{tr("关闭")}</button>}
        </div>
      </div>
    </ModalBackdrop>
  );
}