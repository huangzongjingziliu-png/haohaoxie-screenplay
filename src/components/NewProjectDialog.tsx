import ModalBackdrop from './ModalBackdrop';
import {fmt, tr} from '@/i18n';
import { useEffect, useState } from 'react';
import { useEditorStore } from '@/stores/editor';
import { flushPendingEdits } from '@/utils/pendingEdits';
import { ipc, type TemplateSummary } from '@/ipc/client';
export default function NewProjectDialog({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const [title, setTitle] = useState('');
  const [busy, setBusy] = useState(false);
  const [templates, setTemplates] = useState<TemplateSummary[]>([]);
  const [templateId, setTemplateId] = useState('blank');
  const [templateDir, setTemplateDir] = useState('');
  const [loadingTemplates, setLoadingTemplates] = useState(true);
  useEffect(() => {
    let disposed = false;
    void ipc.listProjectTemplates().then((payload) => {
      if (disposed) return;
      setTemplates(payload.templates);
      setTemplateDir(payload.dir);
      setTemplateId((current) => payload.templates.some((t) => t.id === current)
        ? current
        : payload.templates[0]?.id ?? 'blank');
    }).catch(() => {
      // 读取失败时仍可创建空白工程
    }).finally(() => { if (!disposed) setLoadingTemplates(false); });
    return () => { disposed = true; };
  }, []);
  const create = async (choosePath: boolean) => {
    setBusy(true);
    try {
      await flushPendingEdits();
      const name = title.trim() || tr("未命名剧本");
      let path: string | null = null;
      if (choosePath) {
        const { save } = await import('@tauri-apps/plugin-dialog');
        path = await save({ defaultPath: `${name.replace(/[/\\]/g, '-')}.screenplay`, filters: [{ name: '好好写剧本项目', extensions: ['screenplay'] }] });
        if (!path) return;
      }
      const before = useEditorStore.getState().document?.id;
      await useEditorStore.getState().newProject(name, templateId);
      if (!useEditorStore.getState().document || useEditorStore.getState().document?.id === before) return;
      if (path) await useEditorStore.getState().save(path);
      onCreated(); onClose();
    } catch (e) { useEditorStore.getState().pushToast('error', String(e)); }
    finally { setBusy(false); }
  };
  return <ModalBackdrop onDismiss={onClose} disabled={busy} dirty={!!title.trim()} message={tr("放弃已填写的剧本名称并关闭新建窗口？")}><section className="modal" role="dialog" aria-modal="true" aria-labelledby="new-project-title">
    <h2 id="new-project-title">{tr("新建剧本")}</h2>
    <label>{tr("剧本名称")}<input autoFocus aria-label={tr("剧本名称")} value={title} onChange={(e) => setTitle(e.target.value)} /></label>
    <fieldset className="template-options">
      <legend>{tr("从模板开始")}</legend>
      {loadingTemplates && <p className="template-hint">{tr("正在读取模板…")}</p>}
      {templates.map((template) => <label key={template.id} className={`template-option${templateId === template.id ? ' selected' : ''}`}>
        <input type="radio" name="project-template" value={template.id} checked={templateId === template.id} onChange={() => setTemplateId(template.id)} />
        <span>
          <strong>{template.name}</strong>
          {template.description && <small>{template.description}</small>}
        </span>
      </label>)}
    </fieldset>
    {templateDir && <p className="template-hint">{fmt("把 JSON 模板文件放进 {0}，即可新增模板。", templateDir)}</p>}
    <p>{tr("选择文件夹后创建，方便以后查找、移动或删除。也可以暂不保存，先在好好写剧本里试写。")}</p>
    <div className="modal-actions">
      <button disabled={busy} onClick={onClose}>{tr("取消")}</button>
      <button disabled={busy} onClick={() => void create(false)}>{tr("暂不保存，开始写作")}</button>
      <button disabled={busy} className="primary" onClick={() => void create(true)}>{tr("选择保存位置并创建")}</button>
    </div>
  </section></ModalBackdrop>;
}
