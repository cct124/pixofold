# 第三方组件说明

PixoFold 自有代码采用 GPL-3.0-or-later；第三方组件适用其自身许可。工程依赖于 2026-09-21 核实，静态 PNG 核心依赖于 2026-09-22、原生选择依赖于 2026-09-23、内容凭据确认指纹依赖于 2026-09-24 补充核实；完整间接依赖版本以 `pnpm-lock.yaml` 与 `Cargo.lock` 为准。

| 组件 | 用途 | 许可 |
| --- | --- | --- |
| Tauri、tauri-build、Tauri JS API / CLI | 桌面运行时、IPC、构建工具 | MIT OR Apache-2.0 |
| tauri-plugin-dialog 2.7.3 | Rust侧原生文件/目录选择；显式gtk3特性，不启用xdg-portal或JS插件包 | MIT OR Apache-2.0 |
| tauri-plugin-opener 2.5.4 | Rust侧任务绑定结果/备份定位和批次目录打开；关闭默认特性，不安装JS插件或授予通用opener权限（2026-09-29引入） | MIT OR Apache-2.0 |
| tauri-plugin-fs 2.5.2、tauri-plugin 2.6.3（间接） | dialog的路径类型与插件构建；未注册fs插件或授予fs权限 | MIT OR Apache-2.0 |
| rfd 0.16.0（间接） | 原生对话框后端；Windows Common Controls v6、Linux GTK、macOS AppKit | MIT |
| windows-sys 0.60.2 / windows-targets 0.53.5 / 对应0.53.1架构包（间接） | rfd Windows绑定与链接支持 | MIT OR Apache-2.0 |
| React、React DOM、Zustand | 界面与偏好状态 | MIT |
| serde、serde_json | Rust 序列化与契约测试 | MIT OR Apache-2.0 |
| ts-rs | 开发时生成 TypeScript 类型 | MIT |
| oxipng 10.2.1 | 单线程 PNG/APNG 无损重过滤与重压缩，关闭 binary / parallel / zopfli 默认功能 | MIT |
| imagequant 4.4.1 | PNG 有损调色板量化，关闭默认 threads | GPL-3.0-or-later |
| png 0.18.1、crc32fast 1.5.2 | 完整 PNG 解码验证、chunk CRC 校验 | MIT OR Apache-2.0 |
| fdeflate 0.3.7（png既有同版间接依赖，APNG核心显式引用） | 严格zlib结束、长度与Adler32检查；A3从开发引用迁入核心，无版本升级 | MIT OR Apache-2.0 |
| Gifsicle 1.96 | GIF实验与核心独立CLI；当前安装包尚不包含GIF helper | GPL-2.0-only（本项目选择；上游另有替代条款） |
| gif 0.14.2、weezl 0.1.12 | 生产核心独立GIF帧结构读取及有界LZW解码；实验复用同一路径 | MIT OR Apache-2.0（选择MIT） |
| native/gif/budget.h、budget.c、entry.c（本项目新增） | 独立入口和有界分配；未重许可既有Rust主体 | GPL-2.0-only OR GPL-3.0-or-later，helper按GPLv2组合 |
| tempfile 3.27.0 | 独占临时文件与最终持久化 | MIT OR Apache-2.0 |
| same-file 1.0.6 | 基于文件句柄复查源/临时路径身份 | Unlicense OR MIT |
| sha2 0.10.9 | 内容凭据处理授权绑定源文件 SHA256；复用既有锁定版本，不验证或重签凭据 | MIT OR Apache-2.0 |
| sysinfo 0.35.1 | 桌面启动时RAM检测及开发性能工具的本进程RSS采样；只启用system，不启用multithread/disk/network等功能（2026-09-28核对） | MIT |
| tracing 0.1.44、tracing-subscriber 0.3.23 | 核心结构化事件与桌面白名单订阅；subscriber仅启用registry，不启用fmt/ANSI/env-filter/tracing-log（2026-09-28核对锁定源码） | MIT |
| sharded-slab 0.1.7、lazy_static 1.5.0、thread_local 1.1.10（间接） | tracing-subscriber registry的span与线程存储；本次新增锁文件条目 | sharded-slab：MIT；其余：MIT OR Apache-2.0 |
| time 0.3.55、rustix 1.1.5 | 复用既有版本：UTC结构化时间；Unix文件权限/UID及NOFOLLOW标志，安全封装不引入自有unsafe | time：MIT OR Apache-2.0；rustix：Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| ntapi 0.4.3、objc2-io-kit 0.3.2（间接） | sysinfo的Windows/macOS系统资源接口；版本锁定，不新增前端系统访问权限 | ntapi：MIT OR Apache-2.0；objc2-io-kit：MIT OR Apache-2.0 OR Zlib |
| libdeflater / libdeflate-sys 1.26.1（间接） | oxipng 的 DEFLATE 封装及静态 C 库构建 | 封装 Apache-2.0；内含 libdeflate 源码适用 MIT |
| TypeScript | 类型检查 | Apache-2.0 |
| Vite、React 插件、Vitest | 前端构建与测试 | MIT |
| Oxlint、Prettier | lint 与格式检查 | MIT |
| Testing Library、jsdom | DOM 与交互测试 | MIT |
| Node / React 类型定义 | 开发时类型声明 | MIT |

应用图标沿用本仓库已确认的 PixoFold SVG 设计，桌面尺寸由 Tauri CLI 生成；本轮没有复制旧 png-palettes 业务代码。

2026-10-13，GIF有损4A实验新增Pintoch在Wikimedia Commons发布的OpenRefine/Wikidata真实录屏，原件和[来源/作者/哈希清单](tests/fixtures/gif-lossy/SOURCES.json)、[CC BY-SA 4.0完整许可](tests/fixtures/gif-lossy/CC-BY-SA-4.0.txt)一同保存。原件未修改；实验目录另生成320×309缩放衍生图并保留署名/许可，不把缩放节省算作有损压缩收益。照片平移动画由已许可的Pillow hopper.gif派生，沿用其通知；独立合成UI/渐变/透明图由本项目生成，源规则和身份随报告记录。这些测试数据不随应用安装包分发。

2026-10-12将32张公开手测图片整理到 [tests/manual-images](tests/manual-images/README.md)。这些图片适用各自许可，作为独立测试数据保留，不因同仓库而重许可为GPL，也不进入应用安装资源。Wikimedia Commons的三张图片选择CC BY-SA 3.0，APNG弹跳球按来源页作者公有领域授权；PngSuite保留Willem van Schaik的使用/复制/修改/分发许可；Exif方向测试集保留MIT通知；Pillow 12.0.0样本保留上游MIT-CMU通知及原始路径。逐文件作者、来源、许可链接及未改写原始字节的SHA256见 [SOURCES.json](tests/manual-images/SOURCES.json)，归属和许可原文见 [素材说明](tests/manual-images/README.md)。

imagequant 来自 [ImageOptim/libimagequant](https://github.com/ImageOptim/libimagequant)，以锁定的 crates.io 源码依赖使用。2026-10-15 APNG有损实验的score.rs按4.4.1 pal.rs/quant.rs中的标量颜色距离和质量映射公式改编，增加对最终重解码像素的计分及公共API回归；保留GPL-3.0-or-later与Kornel Lesiński版权说明。完整[COPYRIGHT](tools/apng-lossy-lab/imagequant-COPYRIGHT.txt)随实验报告复制；其中还记载Jef Poskanzer、Greg Roelofs的原始许可/版权。未修改依赖源码，分发仍须保留完整通知和相应源码，不只保留本表。

2026-10-15 APNG实验复用已许可的透明骰子、Pillow transparent.png及palette_sepia.png，在隔离目录生成192×144采样内容于208×152透明画布平移的六帧APNG，并保留独立默认图。衍生参数、源作者/许可/哈希及衍生哈希见每次报告的derived-sources.json；元数据省略仅是受控语料构造，不是生产元数据策略。压缩收益只相对衍生APNG，不把缩放或构造节省算作压缩；CC BY-SA图像衍生继续遵循相同许可，其他图片沿用各自通知，不随MSI分发。

JPEG开发实验使用MozJPEG v4.1.5（提交`6c9f0897afa1c2738d7222a0a9ab49e8b536a267`），源码位置与SHA256固定于`tools/jpeg-lab/engine.json`。实验在`.tools/jpeg-lab/`构建cjpeg/djpeg/jpegtran与系数验证工具；J2第一段只将helper及相关许可/来源资料随桌面资源分发。上游`LICENSE.md`、`README.ijg`和各文件通知包含IJG、BSD-3-Clause、Zlib及Mozilla贡献的许可/版权；实验不修改或移除这些源码通知。后续实际随包配置须据此保留完整正文、IJG致谢及对应源码，不能以本表替代许可材料。

J1a新增`native/jpeg/helper.c`及独立Rust核心适配，helper链接同一固定MozJPEG静态库，开发构建另记录其源码和二进制身份；J2第一段开始随桌面资源分发helper并由应用加载验证，实验工具不进入安装包。J2第一段当时仅开放PNG；第二段现已按可信引擎开放PNG/JPEG混合工作流，实际支持边界见 [JPEG核心说明](native/jpeg/README.md)。准备正式发行时，应根据实际平台构建产物收集全部第三方版权与许可正文、必要通知和对应源码资料；本表不是完整发行许可清单。

J1b保守有损延用相同helper/静态库，没有新增引擎依赖。核心回归另使用固定上游源码中的testimages/testorig.jpg（SHA256 acc6ec555d41d15b368320edaa3b20958ee6fa97cb6e4a18d1213d5ae8bec73b）；原图不进入本仓库，回归证据包保留上游README.ijg、LICENSE.md及来源/衍生输出说明。该夹具不等同本项目原创图片，保留其原有版权和许可。

2026-10-10核对GIF实验依赖。Gifsicle 1.96来自Eddie Kohler官方归档，来源与SHA256固定于 [engine.json](tools/gif-lab/engine.json)。上游README记载Copyright (C) 1997–2025 Eddie Kohler，GPL Version 2（仅Version 2）及替代条款，本项目按GPL-2.0-only使用独立工具进程，未复制其实现到Rust代码。固定源码树保留全部版权，实验报告复制完整 `COPYING` 和 `README.md`，记录归档、源码树、构建配方与二进制身份；这两份原文与全部对应源码都不能用本表替代。2026-10-12第三阶段将有界helper纳入桌面gif/runtime资源，同时提供完整上游对应源码归档、封装、构建配方与许可原文，不能仅以下载链接或SHA256替代。

独立验证器锁定crates.io的gif 0.14.2与weezl 0.1.12，第二阶段已迁入Rust生产依赖；gif关闭默认功能，仅开启std，weezl使用std有界缓冲接口，不引入其async功能。Cargo.lock保留版本与源码校验值。两者上游均允许MIT或Apache-2.0，本实验按MIT使用并保留 [gif完整MIT通知](tools/gif-lab/licenses/gif-LICENSE-MIT)（Copyright (c) 2015 nwin）及 [weezl完整MIT通知](tools/gif-lab/licenses/weezl-LICENSE-MIT)（Copyright (c) HeroicKatora 2020）；实验输出另复制这些通知。原始crate归档包含 `LICENSE-MIT`、`LICENSE-APACHE` 与源码，分别可从crates.io固定版本取得；后续若分发独立验证器，仍须根据其实际全部依赖收集通知。

第二阶段新增native/gif封装另提供GPL-2.0-only OR GPL-3.0-or-later，独立helper按GPLv2与固定Gifsicle组合；既有Rust主体许可不变，Rust经GIF字节管道调用独立进程。分发资源保留完整上游对应源码、本封装/配方及COPYING/README等原文，build.rs检查其完整性和身份。封装与当前执行范围见 [GIF核心说明](native/gif/README.md)，实际打包/安装证据见 [动画开发记录](docs/devlog/_plan/261009/animation-foundation.md)。
