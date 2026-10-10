import {tr} from '@/i18n';
import { useState } from 'react';
import { useEditorStore } from '@/stores/editor';
import { flushPendingEdits } from '@/utils/pendingEdits';
export default function NewProjectDialog({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const [title, setTitle] = useState('');
  const [busy, setBusy] = useState(false);
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
      await useEditorStore.getState().newProject(name);
      if (!useEditorStore.getState().document || useEditorStore.getState().document?.id === before) return;
      if (path) await useEditorStore.getState().save(path);
      onCreated(); onClose();
    } catch (e) { useEditorStore.getState().pushToast('error', String(e)); }
    finally { setBusy(false); }
  };
  return <div className="modal-backdrop"><section className="modal" role="dialog" aria-modal="true" aria-labelledby="new-project-title">
    <h2 id="new-project-title">{tr("新建剧本")}</h2>
    <label>{tr("剧本名称")}<input autoFocus aria-label={tr("剧本名称")} value={title} onChange={(e) => setTitle(e.target.value)} /></label>
    <p>{tr("选择文件夹后创建，方便以后查找、移动或删除。也可以暂不保存，先在好好写剧本里试写。")}</p>
    <div className="modal-actions">
      <button disabled={busy} onClick={onClose}>{tr("取消")}</button>
      <button disabled={busy} onClick={() => void create(false)}>{tr("暂不保存，开始写作")}</button>
      <button disabled={busy} className="primary" onClick={() => void create(true)}>{tr("选择保存位置并创建")}</button>
    </div>
  </section></div>;
}
