import ModalBackdrop from './ModalBackdrop';
import {tr,fmt} from '@/i18n';
import { useMemo, useState } from 'react';
import { useEditorStore } from '@/stores/editor';
import type { BlockType } from '@/ipc/types/BlockType';

const TYPE_LABEL: Record<BlockType, string> = {
  scene_heading: '场景标题', action: '动作', character: '人物', parenthetical: '括注',
  dialogue: '对白', dual_dialogue: '双人对白', transition: '转场', shot: '镜头',
  note: '注释', section: '章节', synopsis: '概要', general: '一般', page_break: '分页符',
};
const TYPES = Object.keys(TYPE_LABEL) as BlockType[];

export default function ImportReview({ onClose }: { onClose: () => void }) {
  const document = useEditorStore((s) => s.document);
  const changeBlockType = useEditorStore((s) => s.changeBlockType);
  const pushToast = useEditorStore((s) => s.pushToast);
  const [showAll,setShowAll]=useState(false),[limit,setLimit]=useState(100);
  const complex=document?.import_info?.source_type==='pdf'||document?.import_info?.source_type==='image';
  const candidates = useMemo(() => document?.blocks.filter((b) => showAll || b.confidence > 0 && b.confidence < 0.75) ?? [], [document,showAll]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [type, setType] = useState<BlockType | null>(null);
  if (!document?.import_info) return null;
  const active = candidates.find((b) => b.id === (activeId ?? candidates[0]?.id));

  const confirmType = async () => {
    if (!active) return;
    setSaving(true);
    try {
      await changeBlockType(active.id, type ?? active.block_type);
      const updated = useEditorStore.getState().document?.blocks.find((b) => b.id === active.id);
      if (updated?.confidence === 1) {
        setActiveId(null);
        setType(null);
        pushToast('success', fmt("已确认：{0}",TYPE_LABEL[updated.block_type]));
      }
    } finally { setSaving(false); }
  };

  const selectCandidate = (id: string) => {
    setActiveId(id);
    const block = document.blocks.find((b) => b.id === id);
    setType(block?.block_type ?? null);
  };

  return <ModalBackdrop className="modal-backdrop import-review-backdrop" onDismiss={onClose} disabled={saving}>
    <section className="modal import-review" role="dialog" aria-modal="true" aria-labelledby="import-review-title">
      <header className="import-review-header">
        <div>
          <h2 id="import-review-title">{tr("导入校对")}</h2>
          <p className="modal-hint">{tr("逐项确认结构识别结果。原文来自文本提取或 OCR 识别，源文件名随项目记录。")}</p>
        </div>
        <button aria-label={tr("关闭导入校对")} onClick={onClose}>×</button>
      </header>
      {complex&&<p className="modal-hint">{tr("PDF／图片由模型整理。解析分值只表示语法识别，不代表分类一定正确，可抽查全部元素。")}</p>}
      <label><input type="checkbox" checked={showAll} onChange={e=>{setShowAll(e.target.checked);setLimit(100);setActiveId(null);}}/>{tr("查看全部元素（抽查分类）")}</label>
      <div className="import-review-content">
        <aside className="import-review-list">
          <strong>{tr("待确认 · ")}{candidates.length}</strong>
          {candidates.length === 0 ? <p className="modal-hint">{tr("所有低置信度元素均已确认。")}</p> : candidates.slice(0,limit).map((block, index) => (
            <button key={block.id} className={(active?.id === block.id ? 'selected ' : '') + 'import-review-item'} onClick={() => selectCandidate(block.id)}>
              <span>{index + 1}. {tr(TYPE_LABEL[block.block_type])}</span>
              <small>{tr(complex?"语法解析分 ":"置信度 ")}{Math.round(block.confidence * 100)}%</small>
              <span className="import-review-text">{block.text}</span>
            </button>
          ))}
          {candidates.length>limit&&<button onClick={()=>setLimit(n=>n+100)}>{tr("显示更多元素")}</button>}
        </aside>
        <div className="import-review-detail">
          {active ? <>
            <div className="import-review-original-label">{tr("识别出的内容")}</div>
            <blockquote>{active.text}</blockquote>
            <label htmlFor="review-block-type">{tr("元素类型")}</label>
            <select id="review-block-type" value={type ?? active.block_type} onChange={(e) => setType(e.target.value as BlockType)}>
              {TYPES.map((candidate) => <option key={candidate} value={candidate}>{tr(TYPE_LABEL[candidate])}</option>)}
            </select>
            <button className="primary" disabled={saving} onClick={() => void confirmType()}>{saving ? tr("正在保存…") : tr("确认此类型")}</button>
          </> : <p className="modal-hint">{tr("可以在下方查看解析原文和去噪记录。")}</p>}
          <details className="import-review-source">
            <summary>{document.import_info.source_name ? fmt("来源：{0} · ",document.import_info.source_name) : ''}{tr("查看完整提取原文（")}{document.import_info.original_text.length.toLocaleString()}{tr(" 字符）")}</summary>
            <pre>{document.import_info.original_text || tr("该格式没有可恢复的文本原稿。")}</pre>
          </details>
          {document.import_info.removed_noise.length > 0 && <details className="import-review-source">
            <summary>{tr("去噪记录（")}{document.import_info.removed_noise.length}{tr(" 项）")}</summary>
            <ul>{document.import_info.removed_noise.map((noise, i) => <li key={`${i}-${noise}`}>{noise}</li>)}</ul>
          </details>}
        </div>
      </div>
      <footer className="modal-actions"><button disabled={saving} onClick={onClose}>{tr("完成")}</button></footer>
    </section>
  </ModalBackdrop>;
}
