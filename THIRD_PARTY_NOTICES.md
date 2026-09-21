# 第三方组件说明

当前为工程脚手架。PixoFold 自有代码采用 GPL-3.0-or-later；第三方组件适用其自身许可。以下为 2026-09-21 核实的直接依赖，完整间接依赖版本以 `pnpm-lock.yaml` 与 `Cargo.lock` 为准。

| 组件 | 用途 | 许可 |
| --- | --- | --- |
| Tauri、tauri-build、Tauri JS API / CLI | 桌面运行时、IPC、构建工具 | MIT OR Apache-2.0 |
| React、React DOM、Zustand | 界面与偏好状态 | MIT |
| serde、serde_json | Rust 序列化与契约测试 | MIT OR Apache-2.0 |
| ts-rs | 开发时生成 TypeScript 类型 | MIT |
| TypeScript | 类型检查 | Apache-2.0 |
| Vite、React 插件、Vitest | 前端构建与测试 | MIT |
| Oxlint、Prettier | lint 与格式检查 | MIT |
| Testing Library、jsdom | DOM 与交互测试 | MIT |
| Node / React 类型定义 | 开发时类型声明 | MIT |

应用图标沿用本仓库已确认的 PixoFold SVG 设计，桌面尺寸由 Tauri CLI 生成；本轮没有复制旧 png-palettes 业务代码。

当前未引入 imagequant、MozJPEG、Gifsicle 等压缩引擎。准备正式发行时，应根据实际平台构建产物收集全部第三方版权与许可正文、必要通知和对应源码资料；本表不是完整发行许可清单。
