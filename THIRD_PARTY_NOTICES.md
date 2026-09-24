# 第三方组件说明

PixoFold 自有代码采用 GPL-3.0-or-later；第三方组件适用其自身许可。工程依赖于 2026-09-21 核实，静态 PNG 核心依赖于 2026-09-22、原生选择依赖于 2026-09-23、内容凭据确认指纹依赖于 2026-09-24 补充核实；完整间接依赖版本以 `pnpm-lock.yaml` 与 `Cargo.lock` 为准。

| 组件 | 用途 | 许可 |
| --- | --- | --- |
| Tauri、tauri-build、Tauri JS API / CLI | 桌面运行时、IPC、构建工具 | MIT OR Apache-2.0 |
| tauri-plugin-dialog 2.7.3 | Rust侧原生文件/目录选择；显式gtk3特性，不启用xdg-portal或JS插件包 | MIT OR Apache-2.0 |
| tauri-plugin-fs 2.5.2、tauri-plugin 2.6.3（间接） | dialog的路径类型与插件构建；未注册fs插件或授予fs权限 | MIT OR Apache-2.0 |
| rfd 0.16.0（间接） | 原生对话框后端；Windows Common Controls v6、Linux GTK、macOS AppKit | MIT |
| windows-sys 0.60.2 / windows-targets 0.53.5 / 对应0.53.1架构包（间接） | rfd Windows绑定与链接支持 | MIT OR Apache-2.0 |
| React、React DOM、Zustand | 界面与偏好状态 | MIT |
| serde、serde_json | Rust 序列化与契约测试 | MIT OR Apache-2.0 |
| ts-rs | 开发时生成 TypeScript 类型 | MIT |
| oxipng 10.2.1 | 单线程 PNG 无损 IDAT 优化，关闭 binary / parallel / zopfli 默认功能 | MIT |
| imagequant 4.4.1 | PNG 有损调色板量化，关闭默认 threads | GPL-3.0-or-later |
| png 0.18.1、crc32fast 1.5.2 | 完整 PNG 解码验证、chunk CRC 校验 | MIT OR Apache-2.0 |
| tempfile 3.27.0 | 独占临时文件与最终持久化 | MIT OR Apache-2.0 |
| same-file 1.0.6 | 基于文件句柄复查源/临时路径身份 | Unlicense OR MIT |
| sha2 0.10.9 | 内容凭据处理授权绑定源文件 SHA256；复用既有锁定版本，不验证或重签凭据 | MIT OR Apache-2.0 |
| libdeflater / libdeflate-sys 1.26.1（间接） | oxipng 的 DEFLATE 封装及静态 C 库构建 | 封装 Apache-2.0；内含 libdeflate 源码适用 MIT |
| TypeScript | 类型检查 | Apache-2.0 |
| Vite、React 插件、Vitest | 前端构建与测试 | MIT |
| Oxlint、Prettier | lint 与格式检查 | MIT |
| Testing Library、jsdom | DOM 与交互测试 | MIT |
| Node / React 类型定义 | 开发时类型声明 | MIT |

应用图标沿用本仓库已确认的 PixoFold SVG 设计，桌面尺寸由 Tauri CLI 生成；本轮没有复制旧 png-palettes 业务代码。

imagequant 来自 [ImageOptim/libimagequant](https://github.com/ImageOptim/libimagequant)，以锁定的 crates.io 源码依赖使用，未复制或修改其实现。其 COPYRIGHT 记载 Kornel Lesiński 的 GPLv3+ 改动及 Jef Poskanzer、Greg Roelofs 的原始许可/版权；分发时需保留完整 COPYRIGHT 和相应源码，不只保留本表。

当前未引入 MozJPEG、Gifsicle 等其他格式压缩引擎。准备正式发行时，应根据实际平台构建产物收集全部第三方版权与许可正文、必要通知和对应源码资料；本表不是完整发行许可清单。
