/** 手动跳转（点左侧场景、搜索结果、白板、分集证据）后，短暂抑制“滚动跟随高亮”，避免被立刻改回去。 */
let suppressedUntil = 0;

export function suppressSceneFollow(ms = 900) {
  suppressedUntil = Date.now() + ms;
}

export function sceneFollowSuppressed() {
  return Date.now() < suppressedUntil;
}
