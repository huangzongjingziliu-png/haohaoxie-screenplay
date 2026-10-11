import {tr,fmt} from '@/i18n';
import { ELEMENT_TYPES, readShortcuts, saveShortcuts, shortcutError, keyCombo, shortcutLabel, DEFAULT_SHORTCUTS } from '@/utils/elementShortcuts';
import { useEffect, useState } from 'react';
import { useEditorStore } from '@/stores/editor';
import { flushPendingEdits } from '@/utils/pendingEdits';
import { ELEMENT_LABELS, validElementStyle, defaultElementStyle, readPresets, writePresets, readUiSize, saveUiSize } from '@/utils/appearance';
import type { ElementStyle } from '@/ipc/types/ElementStyle';
import { ipc, ScreenplayError } from '@/ipc/client';

/**
 * 设置面板。
 *
 * 修改走 `ipc.updateSettings` → Rust `CommandService::mutate_document`，
 * 前端不直接改文档对象。
 *
 * 注意：`ScreenplaySettings` 是**扁平**结构（纸张尺寸直接在 settings 上），
 * 不是嵌套 `paper` 子对象。
 */
export function TypographySettings({ onClose, embedded=false, registerSave }: { onClose: () => void; embedded?:boolean; registerSave?:(save:()=>Promise<boolean>)=>void }) {
  const doc = useEditorStore((s) => s.document);
  const pushToast = useEditorStore((s) => s.pushToast);

  const [shortcuts, setShortcuts] = useState(readShortcuts);
  const [shortcutMessage, setShortcutMessage] = useState('');
  const shortcutSettings = <details className="shortcut-settings"><summary>{tr("元素快捷键 · 点击展开")}</summary><p className="modal-hint">{tr("点击快捷键框，再按 Command/Ctrl + 数字。可加 Option/Alt 或 Shift；仅在纸面编辑时生效。按退格清除。")}</p>{ELEMENT_TYPES.map((t) => <label className="field" key={t}><span>{tr(ELEMENT_LABELS[t])}</span><input aria-label={fmt("{0}快捷键",tr(ELEMENT_LABELS[t]))} readOnly value={shortcutLabel(shortcuts[t])} onKeyDown={e => { if (e.key === 'Tab') return; e.preventDefault(); if (['Backspace','Delete'].includes(e.key)) { setShortcuts({...shortcuts, [t]: ''}); return; } const combo = keyCombo(e.nativeEvent); if (combo) { const next = {...shortcuts,[t]:combo}; const error = shortcutError(next); setShortcutMessage(error); if (!error) setShortcuts(next); } }} /></label>)}<button onClick={() => { setShortcuts({...DEFAULT_SHORTCUTS}); setShortcutMessage(''); }}>{tr("恢复默认快捷键")}</button>{shortcutMessage && <p role="alert">{shortcutMessage}</p>}</details>;
  const [uiSize, setUiSize] = useState(readUiSize);
  const [styles, setStyles] = useState<Record<string, ElementStyle>>({});
  const [presets, setPresets] = useState(readPresets);
  const [presetName, setPresetName] = useState('');
  const [element, setElement] = useState('action');
  const [presetChoice, setPresetChoice] = useState('');
  const [presetError, setPresetError] = useState('');
  const updateStyle = (patch: Partial<ElementStyle>) => setStyles({ ...styles, [element]: { ...style, ...patch } });
  const savePreset = () => {
    const name = presetName.trim();
    if (!name) { setPresetError(tr("请先为预设命名")); return; }
    if (!Object.values(styles).every(validElementStyle)) { setPresetError(tr("请检查元素字体、字号和颜色")); return; }
    try {
      const next = [...presets.filter(p => p.name !== name), { name, typography, styles }];
      writePresets(next); setPresets(next); setPresetChoice(name); setPresetError(''); pushToast('success', fmt("已保存预设“{0}”",name));
    } catch { setPresetError(tr("预设保存失败，请检查可用空间")); }
  };
  const [title, setTitle] = useState('');
  const [author, setAuthor] = useState('');
  const [language, setLanguage] = useState('zh');
  const [sceneNumbers, setSceneNumbers] = useState(false);
  const [typography, setTypography] = useState('hollywood');
  const style = styles[element] ?? defaultElementStyle(element, typography);
  const [template, setTemplate] = useState<'us-letter' | 'a4'>('us-letter');
  const [margins, setMargins] = useState({ top: 1, bottom: 1, left: 1.5, right: 1 });
  const [ocrLangs, setOcrLangs] = useState<string[]>(['zh-Hans', 'en-US']);
  const [autoCfg, setAutoCfg] = useState({
    enabled: true,
    saveIntervalSecs: 30,
    versionIntervalSecs: 300,
  });

  // 从后端读取真实配置（Rust 是权威）
  useEffect(() => {
    void ipc
      .autosaveConfig()
      .then(setAutoCfg)
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!doc) return;
    setStyles(Object.fromEntries(Object.entries(doc.settings.element_styles ?? {}).filter((entry): entry is [string, ElementStyle] => !!entry[1])));
    setPresetName(doc.settings.style_preset_name ?? '');
    setSceneNumbers(doc.settings.show_scene_numbers ?? false);
    setTitle(doc.metadata.title);
    setTypography(doc.settings.typography ?? "hollywood");
    setAuthor(doc.metadata.author);
    setLanguage(doc.settings.language || 'zh');
    setTemplate(doc.settings.template === 'a4' ? 'a4' : 'us-letter');
    setMargins({
      top: doc.settings.margin_top_inch,
      bottom: doc.settings.margin_bottom_inch,
      left: doc.settings.margin_left_inch,
      right: doc.settings.margin_right_inch,
    });
    setOcrLangs(doc.settings.ocr_languages ?? ['zh-Hans', 'en-US']);
  }, [doc?.id]);

  const save = async (close=true):Promise<boolean> => {
    if (shortcutError(shortcuts)) { setShortcutMessage(shortcutError(shortcuts)); return false; }
    if (!doc) { saveShortcuts(shortcuts); saveUiSize(uiSize); if(close)onClose(); return true; }
    if (!Object.values(styles).every(validElementStyle)) {
      setPresetError(tr("请检查元素字体、字号和颜色：字号范围为 8–36 pt")); return false;
    }
    const isA4 = template === 'a4';
    try {
      await flushPendingEdits();
      await ipc.updateSettings({
        title: title.trim() || tr("未命名剧本"),
        author,
        language,
        typography,
        elementStyles: styles,
        stylePresetName: presetName.trim(),
        ocrLanguages: ocrLangs,
        showSceneNumbers: sceneNumbers,
        paper: {
          template,
          page_width_inch: isA4 ? 8.27 : 8.5,
          page_height_inch: isA4 ? 11.69 : 11.0,
          margin_top_inch: margins.top,
          margin_bottom_inch: margins.bottom,
          margin_left_inch: margins.left,
          margin_right_inch: margins.right,
        },
      });
      // 自动保存配置走后端命令（立即生效）
      await ipc.configureAutosave({
        enabled: autoCfg.enabled,
        saveIntervalSecs: autoCfg.saveIntervalSecs,
        versionIntervalSecs: autoCfg.versionIntervalSecs,
      });
      saveShortcuts(shortcuts);
      saveUiSize(uiSize);
      pushToast('success', tr("设置已保存"));
      if(close)onClose();
      return true;
    } catch (e) {
      pushToast('error', e instanceof ScreenplayError ? e.message : String(e));
      return false;
    }
  };

  useEffect(()=>{registerSave?.(()=>save(false));});

  if (!doc) {
    return (
      <div className={embedded?"settings-body":"modal-backdrop"} onClick={embedded?undefined:onClose}>
        <div className={embedded?"settings-content":"modal"} onClick={(e) => e.stopPropagation()}>
          <h2>{tr("设置")}</h2>
          <label className="field"><span>{tr("界面字号（不影响纸面和导出）")}</span><select value={uiSize} onChange={e => setUiSize(Number(e.target.value))}>{[13,14,15,16,18].map(n => <option key={n} value={n}>{n} px</option>)}</select></label>
          {shortcutSettings}
          <div className="modal-actions">
            <button onClick={() => void save()}>{tr("保存界面设置")}</button>
          </div>
        </div>
      </div>
    );
  }

  const toggleOcrLang = (l: string) => {
    setOcrLangs((prev) =>
      prev.includes(l) ? prev.filter((x) => x !== l) : [...prev, l]
    );
  };

  return (
    <div className={embedded?"settings-body":"modal-backdrop"} onClick={embedded?undefined:onClose}>
      <div className={embedded?"settings-content appearance-settings":"modal appearance-settings"} onClick={(e) => e.stopPropagation()}>
        <h2>{tr("文字与剧本样式")}</h2>
        <label className="field"><span>{tr("界面字号（标签、按钮，不影响纸面和导出）")}</span><select value={uiSize} onChange={e => setUiSize(Number(e.target.value))}>{[13,14,15,16,18].map(n => <option key={n} value={n}>{n} px{n === 14 ? tr(" · 默认") : ''}</option>)}</select></label>

        {shortcutSettings}
        <label className="field">
          <span>{tr("剧本名称")}</span>
          <input value={title} onChange={(e) => setTitle(e.target.value)} />
        </label>

        <label className="field">
          <span>{tr("编剧")}</span>
          <input value={author} onChange={(e) => setAuthor(e.target.value)} />
        </label>

        <label className="field">
          <span>{tr("剧本排版")}</span>
          <select aria-label={tr("剧本排版模板")} value={typography} onChange={(e) => { setTypography(e.target.value); setStyles({}); setPresetName(''); setPresetChoice(''); }}>
            <option value="hollywood">{tr("美式剧本 · Courier")}</option>
            <option value="chinese-reading">{tr("中文阅读版 · 参考页格式")}</option>
          </select>

        </label>
        <section className="custom-style-settings">
          <h3>{tr("自定义元素样式")}</h3>
          <p className="modal-hint">{tr("基于上方默认格式修改。样式随工程保存；命名预设可用于其他剧本。大字号会相应增加行距。")}</p>
          <label className="field"><span>{tr("使用已保存的预设")}</span><select aria-label={tr("样式预设")} value={presetChoice} onChange={e => { const p = presets.find(p => p.name === e.target.value); setPresetChoice(e.target.value); if (p) { setTypography(p.typography); setStyles(p.styles); setPresetName(p.name); } }}><option value="">{tr("选择预设…")}</option>{presets.map(p => <option key={p.name}>{p.name}</option>)}</select></label>
          <label className="field"><span>{tr("元素标签")}</span><select aria-label={tr("样式元素")} value={element} onChange={e => setElement(e.target.value)}>{Object.entries(ELEMENT_LABELS).map(([key, label]) => <option key={key} value={key}>{tr(label)}{styles[key] ? tr(" · 已自定义") : ''}</option>)}</select></label>
          <div className="style-fields">
            <label className="field"><span>{tr("字体")}</span><input aria-label={tr("元素字体")} list="screenplay-fonts" value={style.font_family} onChange={e => updateStyle({ font_family: e.target.value })}/><datalist id="screenplay-fonts">{['Songti SC','PingFang SC','Heiti SC','Kaiti SC','Courier','Menlo','Arial','Times New Roman'].map(f => <option key={f} value={f}/>)}</datalist></label>
            <label className="field"><span>{tr("字号（pt）")}</span><input aria-label={tr("元素字号")} type="number" min={8} max={36} step={0.5} value={style.font_size} onChange={e => updateStyle({ font_size: Number(e.target.value) })}/></label>
            <label className="field"><span>{tr("颜色")}</span><input aria-label={tr("元素颜色")} type="color" value={style.color} onChange={e => updateStyle({ color: e.target.value })}/></label>
          </div>
          <div className="style-color-swatches" aria-label={tr("常用文字颜色")}>{[['#000000',tr("黑色")],['#555555',tr("灰色")],['#2355aa',tr("蓝色")],['#9f2936',tr("暗红")],['#276347',tr("绿色")],['#9b6819',tr("棕色")]].map(([color,label]) => <button key={color} aria-label={fmt("文字颜色：{0}",tr(label))} title={tr(label)} onClick={() => updateStyle({color})} style={{backgroundColor:color}} className={style.color.toLowerCase() === color ? 'selected' : ''}/>)}</div>
          <div className="style-toggles"><label><input type="checkbox" checked={element !== 'parenthetical' && style.bold} disabled={element === 'parenthetical'} onChange={e => updateStyle({ bold: e.target.checked })}/>{tr("加粗")}</label><label><input type="checkbox" checked={style.italic} onChange={e => updateStyle({ italic: e.target.checked })}/>{tr("斜体")}</label><button onClick={() => { const next = { ...styles }; delete next[element]; setStyles(next); }}>{tr("恢复此元素默认")}</button></div>
          <div className="style-preview" style={{ fontFamily: style.font_family, fontSize: `${style.font_size}pt`, fontWeight: element !== 'parenthetical' && style.bold ? 700 : 400, fontStyle: style.italic ? 'italic' : 'normal', color: style.color }}>{element === 'parenthetical' ? '（轻声）' : element === 'character' ? '剑客' : element === 'dialogue' ? '我是个爱做梦的人。' : '风从旷野吹来，卷起沙尘。'}</div>
          <label className="field"><span>{tr("预设名称")}</span><input aria-label={tr("预设名称")} placeholder={tr("例如：我的中文剧本")} maxLength={80} value={presetName} onChange={e => setPresetName(e.target.value)}/></label>
          <div className="style-toggles"><button onClick={savePreset}>{tr("保存为命名预设")}</button><button disabled={!presetChoice} onClick={() => { const next = presets.filter(p => p.name !== presetChoice); try { writePresets(next); setPresets(next); setPresetChoice(''); } catch { setPresetError(tr("删除预设失败")); } }}>{tr("删除所选预设")}</button><button onClick={() => { setStyles({}); setPresetName(''); setPresetChoice(''); }}>{tr("恢复全部默认样式")}</button></div>
          {presetError && <p role="alert">{presetError}</p>}
          <p className="modal-hint">{tr("可输入这台 Mac 已安装的字体名称；缺失的字体会由系统替代。PDF、Word、HTML 支持自定义外观；Fountain、FDX 和纯文本导出保留文字与结构，不包含这些自定义外观。")}</p>
        </section>
          <label className="setting-checkbox"><input type="checkbox" checked={sceneNumbers} onChange={(e) => setSceneNumbers(e.target.checked)} />{tr("显示场号（中文模板为“第几场”）")}</label>
        <label className="field">
          <span>{tr("语言")}</span>
          <select value={language} onChange={(e) => setLanguage(e.target.value)}>
            <option value="zh">{tr("中文")}</option>
            <option value="en">English</option>
          </select>
        </label>

        <div className="field">
          <span>{tr("纸张")}</span>
          <div className="paper-choices">
            <button
              className={template === 'us-letter' ? 'active' : ''}
              onClick={() => setTemplate('us-letter')}
            >
              US Letter (8.5×11″)
            </button>
            <button
              className={template === 'a4' ? 'active' : ''}
              onClick={() => setTemplate('a4')}
            >
              A4 (210×297mm)
            </button>
          </div>
        </div>

        <details className="field">
          <summary>{tr("页边距（英寸）")}</summary>
          <div className="margin-grid">
            {(
              [
                ['top', tr("上")],
                ['bottom', tr("下")],
                ['left', tr("左")],
                ['right', tr("右")],
              ] as const
            ).map(([key, label]) => (
              <label key={key}>
                <span>{tr(label)}</span>
                <input
                  type="number"
                  step={0.1}
                  min={0}
                  value={margins[key]}
                  onChange={(e) =>
                    setMargins({ ...margins, [key]: Number(e.target.value) || 0 })
                  }
                />
              </label>
            ))}
          </div>
        </details>

        <div className="field">
          <span>{tr("OCR 语言（导入图片/扫描件时使用）")}</span>
          <div className="ocr-langs">
            {[
              ['zh-Hans', tr("简体中文")],
              ['zh-Hant', tr("繁体中文")],
              ['en-US', 'English'],
              ['ja-JP', '日本語'],
            ].map(([code, label]) => (
              <label key={code} className="checkbox">
                <input
                  type="checkbox"
                  checked={ocrLangs.includes(code)}
                  onChange={() => toggleOcrLang(code)}
                />
                <span>{tr(label)}</span>
              </label>
            ))}
          </div>
        </div>

        <div className="field">
          <span>{tr("版本与保存")}</span>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={autoCfg.enabled}
              onChange={(e) => setAutoCfg({ ...autoCfg, enabled: e.target.checked })}
            />
            <span>{tr("自动保存")}</span>
          </label>
          <label className="inline">
            <span>{tr("自动保存间隔（秒，0=关闭）")}</span>
            <input
              type="number"
              min={0}
              step={10}
              value={autoCfg.saveIntervalSecs}
              onChange={(e) =>
                setAutoCfg({ ...autoCfg, saveIntervalSecs: Math.max(0, Number(e.target.value) || 0) })
              }
            />
          </label>
          <label className="inline">
            <span>{tr("自动版本间隔（秒，0=关闭）")}</span>
            <input
              type="number"
              min={0}
              step={60}
              value={autoCfg.versionIntervalSecs}
              onChange={(e) =>
                setAutoCfg({
                  ...autoCfg,
                  versionIntervalSecs: Math.max(0, Number(e.target.value) || 0),
                })
              }
            />
          </label>
        </div>

        <div className="modal-actions">
          <button onClick={onClose}>{tr("取消")}</button>
          <button className="primary" onClick={() => void save()}>{tr(" 保存设置 ")}</button>
        </div>
      </div>
    </div>
  );
}
export default TypographySettings;
