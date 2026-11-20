/** 全局快捷键（⌘Z / ⌘Y）是否应让位给输入控件。
 *
 *  搜索框、白板便签输入框、设置页输入框都自带浏览器原生撤销；
 *  若被全局快捷键抢走，会变成撤销整篇剧本。
 *  剧本正文块例外：它由编辑器自己做应用级撤销。
 */
export function shouldBypassUndoShortcut(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  const entry = target.closest(
    'input, textarea, select, [contenteditable="true"], [contenteditable=""], [role="textbox"]',
  );
  if (!entry) return false;
  return !entry.closest('.screenplay-block');
}
