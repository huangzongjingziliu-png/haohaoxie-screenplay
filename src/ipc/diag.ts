/**
 * 开发期诊断文件写入。
 *
 * 通过 Rust 命令写文件（比 Tauri fs 插件更可靠，且打包后也可用）。
 */

import { invoke } from '@tauri-apps/api/core';

/** 展开 `~` 为 home 目录 */
async function expand(path: string): Promise<string> {
  if (!path.startsWith('~')) return path;
  const home = await invoke<string>('home_dir');
  return path.replace('~', home);
}

/** 写文本文件（自动创建父目录） */
export async function writeTextFile(path: string, content: string): Promise<string> {
  const full = await expand(path);
  await invoke('write_text_file', { path: full, content });
  return full;
}

/** 读文本文件 */
export async function readTextFile(path: string): Promise<string> {
  const full = await expand(path);
  return invoke<string>('read_text_file', { path: full });
}