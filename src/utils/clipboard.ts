/**
 * 写系统剪贴板并确认成功。
 *
 * 剪切/复制是事务：写不进去就不能删正文。`navigator.clipboard` 在部分
 * WebView 或权限状态下会拒绝，这里再退回到隐藏 textarea + execCommand，
 * 仍失败则返回 false，由调用方保留正文并提示。
 */
export async function writeClipboardText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* 权限被拒或 WebView 不支持，走兜底 */
  }
  try {
    const area = document.createElement('textarea');
    area.value = text;
    area.setAttribute('readonly', '');
    area.style.position = 'fixed';
    area.style.left = '-9999px';
    area.style.top = '0';
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand('copy');
    area.remove();
    return ok;
  } catch {
    return false;
  }
}
