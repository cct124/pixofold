import { invoke, isTauri } from '@tauri-apps/api/core';
import type { AppInfo } from './generated';

/** 浏览器预览返回 null；桌面模式返回真实 Rust 信息，调用错误由界面处理。 */
export async function getAppInfo(): Promise<AppInfo | null> {
  if (!isTauri()) return null;
  return invoke<AppInfo>('get_app_info');
}

/** 仅打开Rust固定常量中的项目仓库地址；无URL参数，浏览器预览不可用。 */
export async function openProjectRepository(): Promise<void> {
  if (!isTauri()) throw new Error('desktop_only');
  await invoke('open_project_repository');
}
