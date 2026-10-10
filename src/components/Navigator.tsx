import {tr} from '@/i18n';
import { useSceneDrag } from '@/utils/useSceneDrag';
import { moveScene } from '@/utils/scenes';
import { useMemo, useState } from 'react';
import { useEditorStore } from '@/stores/editor';

/** 大纲导航：场景 / 人物 / 地点 / 结构。数据全部来自后端 AST。 */
export default function Navigator() {
  const {startDrag,pointerTarget,insertion} = useSceneDrag('vertical');
  const [dragged, setDragged] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const doc = useEditorStore((s) => s.document);
  const tab = useEditorStore((s) => s.navigatorTab);
  const setTab = useEditorStore((s) => s.setNavigatorTab);
  const setActiveBlock = useEditorStore((s) => s.setActiveBlock);
  const activeBlockId = useEditorStore((s) => s.activeBlockId);

  const scenes = useMemo(
    () =>
      (doc?.blocks ?? [])
        .filter((b) => b.block_type === 'scene_heading')
        .map((b, i) => ({ id: b.id, label: b.text, no: i + 1 })),
    [doc]
  );

  const characters = useMemo(() => doc?.characters ?? [], [doc]);
  const locations = useMemo(() => doc?.locations ?? [], [doc]);

  const outline = useMemo(
    () =>
      (doc?.blocks ?? []).map((b, i) => ({
        id: b.id,
        index: i,
        type: b.block_type,
        text: b.text,
      })),
    [doc]
  );

  const jump = (id: string) => {
    setActiveBlock(id);
    // 滚动到对应块
    requestAnimationFrame(() => {
      const el = document.querySelector(`[data-block-id="${id}"]`);
      el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
      const row = document.querySelector('.block-row.active');
      row?.scrollIntoView({ behavior: 'smooth', block: 'center' });
    });
  };

  return (
    <div className="navigator">
      <div className="navigator-tabs">
        {(['scenes', 'characters', 'locations', 'outline'] as const).map((t) => (
          <button key={t} className={tab === t ? 'active' : ''} onClick={() => setTab(t)}>
            {tr({ scenes: '场景', characters: '人物', locations: '地点', outline: '大纲' }[t])}
          </button>
        ))}
      </div>

      <div className="navigator-body">
        {tab === 'scenes' && (
          <ul>
            {scenes.length === 0 && <li className="empty">{tr("暂无场景")}</li>}
            {scenes.map((s) => (
              <li
                key={s.id}
                draggable
                data-scene-id={s.id}
                tabIndex={0}
                onPointerDown={e => startDrag(e,s.id)}
                title={tr("拖动以调整场景顺序")}
                onDragStart={e => { setDragged(s.id); e.dataTransfer.setData('application/x-muben-scene', s.id); e.dataTransfer.effectAllowed = 'move'; }}
                onDragOver={e => { if (dragged) { e.preventDefault(); setDropTarget(s.id); } }}
                onDragEnd={() => { setDragged(null); setDropTarget(null); }}
                onDrop={e => { e.preventDefault(); const id = e.dataTransfer.getData('application/x-muben-scene') || dragged; setDragged(null); setDropTarget(null); if (id) void moveScene(id,s.id).catch(error => useEditorStore.getState().pushToast('error', String(error))); }}
                data-insertion={insertion?.id === s.id ? insertion.side : undefined}
                data-drop-target={dropTarget === s.id || pointerTarget === s.id}
                className={activeBlockId === s.id ? 'active' : ''}
                onClick={() => jump(s.id)}
              >
                <span className="num">{s.no}</span>
                <span className="label">{s.label}</span>
              </li>
            ))}
          </ul>
        )}

        {tab === 'characters' && (
          <ul>
            {characters.length === 0 && <li className="empty">{tr("暂无人物")}</li>}
            {characters.map((c) => (
              <li key={c.id}>
                <span className="label">{c.name}</span>
                {c.aliases.length > 0 && (
                  <span className="alias">{tr("别名：")}{c.aliases.join('、')}</span>
                )}
              </li>
            ))}
          </ul>
        )}

        {tab === 'locations' && (
          <ul>
            {locations.length === 0 && <li className="empty">{tr("暂无地点")}</li>}
            {locations.map((l) => (
              <li key={l.id}>
                <span className="label">{l.name}</span>
                <span className="count">{l.count}{tr(" 次")}</span>
              </li>
            ))}
          </ul>
        )}

        {tab === 'outline' && (
          <ul className="outline">
            {outline.map((o) => (
              <li
                key={o.id}
                className={`${activeBlockId === o.id ? 'active' : ''} type-${o.type}`}
                onClick={() => jump(o.id)}
              >
                <span className="idx">{o.index + 1}</span>
                <span className="type">{o.type}</span>
                <span className="text">{o.text.slice(0, 40)}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}