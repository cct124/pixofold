# PixoFold（轻图）

基于 Tauri、React 和 Rust 的本地批量图片压缩工具，计划支持 PNG、JPEG、GIF 和 APNG，由 png-palettes 重构演进。

当前已建立 **Tauri 2 + React + TypeScript + Rust workspace 脚手架**，实现可独立调用的 **静态 PNG 无损/有损核心及纯 Rust 批量任务服务**：内容识别、资源限制、版本化质量映射、保护性无损回退、真实产物验证、覆盖备份、副本提交，以及显式文件列表的有界执行、快照、取消和重试。桌面仍为工程启动页，包含亮暗主题、中英文、偏好持久化和构建信息 IPC；**目录导入、桌面压缩 IPC 与正式业务界面尚未实现。** 单文件核心已通过三平台 CI；新增批量服务目前只完成 Windows 本机验证，具体证据见开发记录。

UI 设计与可交互 HTML 原型保留作为实现依据：质量采用 0–100 连续滑块和精细输入，默认 80；当前质量描述以纯文字显示在标题行右侧。

HTML 原型采用导入即自动模拟的交互：空闲时显示中央识别区与底部操作技巧，拖入或选择图片、文件夹后切换为图片列表，逐张展示压缩效果；底部同步切换为总数量、体积变化与总进度，清除后恢复技巧，不设置开始压缩按钮。文件夹包含子目录；原型不会生成、覆盖或上传图片。

## 开发环境

2026-09-21 从官方 npm、crates.io 和工具链发布源核对最新稳定版本，并锁定在配置和锁文件中。Tauri 各组件版本独立发布，无需统一补丁号。

| 组件 | 锁定版本 |
| --- | --- |
| Node.js / pnpm | 26.9.0 / 12.5.1 |
| Rust / edition | 1.98.1 / 2024 |
| Tauri runtime / tauri-build | 2.11.6 / 2.6.3 |
| Tauri CLI / JS API | 2.11.5 / 2.11.1 |
| React / React DOM | 19.3.0 |
| TypeScript / Vite | 7.0.2 / 8.3.0 |
| Zustand | 5.0.15 |
| Oxlint / Prettier / Vitest | 1.83.0 / 3.9.8 / 5.0.1 |
| oxipng / png（2026-09-22 核对） | 10.2.1 / 0.18.1 |
| imagequant（2026-09-22 核对，关闭默认 threads） | 4.4.1 |
| tempfile / same-file / crc32fast | 3.27.0 / 1.0.6 / 1.5.2 |

Node 使用 Current 稳定版，版本记录于 `.node-version`；Rust 由 `rust-toolchain.toml` 固定，rustup 在工程目录运行时会选择该工具链。pnpm 版本由 `packageManager` 指定，只维护 `pnpm-lock.yaml`；Rust workspace 共用根目录的 `Cargo.lock`。所有直接依赖固定精确版本，间接依赖按兼容约束锁定。

安装 Node 26.9.0、rustup 后，准备包管理器及平台依赖：

```shell
npm install --global pnpm@12.5.1
rustup show
```

- Windows：Visual Studio 2022 的「使用 C++ 的桌面开发」、Windows SDK、WebView2 Runtime，使用 MSVC Rust target。
- macOS：Xcode Command Line Tools。
- Linux：WebKitGTK 4.1 及系统开发包；Ubuntu 24.04 的实际包名见 CI。

具体系统准备见 [Tauri 官方前置要求](https://v2.tauri.app/start/prerequisites/)。本次本机测试通过项目内 `.tools/` 便携 Node 和并存 Rust 工具链运行，不修改原有全局默认 Node/Rust；`.tools/` 不提交，不是工程依赖来源。

本机若暂不切换全局 Node，可在仓库根目录的 PowerShell 中使用已准备的便携环境（仅对当前终端有效）：

```powershell
$env:PATH = (Join-Path (Get-Location) '.tools/node-v26.9.0-win-x64') + [IO.Path]::PathSeparator + $env:PATH
npm exec --yes --package=pnpm@12.5.1 -- pnpm desktop:dev
```

## 安装与运行

在仓库根目录执行：

```shell
pnpm install --frozen-lockfile
pnpm desktop:dev
```

`desktop:dev` 自动启动 Vite 和桌面应用；开发地址固定为 `http://127.0.0.1:1420`，端口被占用时直接报错，避免桌面端连到错误页面。

| 命令 | 用途 |
| --- | --- |
| `pnpm dev` | 仅启动浏览器预览，明确显示没有连接原生核心 |
| `pnpm desktop:dev` | 启动 Tauri 桌面开发模式 |
| `pnpm build` | 前端类型检查与生产构建，输出 `dist/` |
| `pnpm check` | 格式、lint、类型、前端测试、生成类型一致性、Clippy 和 Rust 测试 |
| `pnpm types:generate` | 从 Rust DTO 更新 `src/lib/ipc/generated.ts` |
| `pnpm types:check` | 只读核对生成类型，过期时失败，不修改文件 |
| `pnpm format` | 主动格式化工程源文件及 Rust；不格式化历史设计文档或 HTML 原型 |
| `pnpm tauri build --no-bundle --ci` | 构建桌面可执行文件，不生成安装包 |
| `pnpm desktop:build` | 构建当前平台的应用和安装包 |
| `cargo test -p pixofold-core --locked` | 独立测试核心，不需要 GUI |
| `pnpm fixtures:check` | 只读重生成并核对 PNG 语料与 SHA256 清单 |
| `cargo run -p pixofold-core --release --locked --example png_baseline` | 在隔离目录测量静态 PNG 体积与耗时基线 |
| `cargo run -p pixofold-core --release --locked --example png_quality_baseline` | q 锚点、实际回退、库评分及黑白背景误差基线 |

Windows 可执行文件位于 `target/release/pixofold.exe`，安装包位于 `target/release/bundle/`。安装包工具可能需要首次联网下载；签名、自动更新与正式发行尚未配置。

## 静态 PNG 核心开发入口

单文件 Rust API 为 `pipeline::optimize_png`，不经过 Tauri，也不创建任务队列；上层批量 API 见下一节。手动运行必须显式选择输出策略：

```powershell
cargo run -p pixofold-core --release --locked --example optimize_png -- tests/fixtures/png/rgba8.png --copy output.png
# 显式有损；质量必须为 0–100 整数，不允许与 --lossless 同时使用。
cargo run -p pixofold-core --release --locked --example optimize_png -- tests/fixtures/png/gradient-binary-alpha.png --copy quantized.png --lossy 80
# 覆盖实验只使用自己复制到隔离目录的图片，不要覆盖已提交的 fixtures。
cargo run -p pixofold-core --release --locked --example optimize_png -- path/to/test.png --overwrite
```

- 只接收真实静态 PNG，扩展名不是判断依据；APNG 明确拒绝。默认无损保留原始像素、位深、隐藏 RGB、调色板及全部非 IDAT chunk；未知不可安全搬运的元数据拒绝处理。
- 有损模式采用 imagequant 的 min=0、target=q（映射版本 1），独立固定 speed=4、dither=1.0。实际 remapping 评分低于 q、透明端点/半透明保护不通过，或体积不优于源文件及无损候选时，明确回退到严格无损。100 不等同于通用无损；评分也不等同于 SSIM。
- 16-bit、ICC/cHRM/HDR、依赖原始表示的 sBIT/bKGD 等元数据保守回退；不隐式降位深或删除颜色信息。支持有效 gAMA 和 sRGB，保留适用元数据及其 IDAT 前后位置。纯透明 alpha=0、不透明 alpha=255 必须保持；半透明仍在 1–254 且 alpha 误差不超过 8。有损不承诺透明像素的隐藏 RGB 不变。
- `PngRequest::new` 继续默认无损，显式设置 `PngMode::Lossy` 才进行量化；`QualityValue` 默认 80。产品界面默认有损 80 的设计尚未接入。报告区分实际量化、保护性回退与无收益，并返回输入/实际输出属性。
- 默认请求采用原图覆盖，但每次成功覆盖都保留同目录 `.pixofold-backup-*.png` 原始备份，路径在结果中返回，核心不会自动删除。提交失败也返回备份路径。确认新图可用后再由用户处理备份。
- 副本必须给出完整新路径、父目录必须存在，同名、目录或链接冲突均拒绝；无收益不创建最终副本或备份。只读输入允许另存，副本不继承只读属性。
- 输入默认 64 MiB，单个解码缓冲区 128 MiB，16M 像素、单边 16384；有损 RGBA 展开另按缓冲区上限检查。单文件同步、编码器单线程；oxipng 有 30 秒软期限，imagequant 在进度回调协作取消，但无硬超时。这不是进程硬内存限额或即时取消保证；批量调用由下述任务服务额外限制文件并发和估算工作集。
- 无损/有损单文件核心均已通过 Windows 本机及三平台 CI 核心文件测试/桌面构建。文件同步不等于目录元数据的断电事务；源文件关闭到替换仍有外部竞争窗口，不承诺网络文件系统、特殊 ACL/ADS、xattr 或断电恢复。正式桌面接入前继续平台和真实业务样本验收。

样本与再生成方式见 [语料说明](tests/fixtures/README.md)，实际基线、错误边界及后续动作见 [无损核心归档](docs/devlog/_fin/260922/png-core-foundation.md)与 [PNG 有损质量归档](docs/devlog/_fin/260922/png-lossy-quality.md)。

## 纯 Rust 批量开发入口

`batch::BatchService` 接收 `BatchRequest { items, parameters }`，每项显式提供源路径与 `OutputPolicy`。API 用法及可编译示例见 [服务入口](crates/pixofold-core/src/batch/mod.rs)，真实混合批次及备份回归见 [集成测试](crates/pixofold-core/tests/png_batch.rs)。不扫描目录、不生成副本文件名、不连接窗口；核心参数默认无损，产品有损 80 仍由后续适配层显式传入。

| API | 契约 |
| --- | --- |
| `new` / `start` | 创建固定线程池；同步只读预检显式文件列表，再后台处理。同一服务只允许一个活动批次，只保留最近批次。调用方应在非 UI 线程执行预检。 |
| `snapshot` / `wait` | Rust 权威快照含批次 ID、稳定行 ID、attempt、revision、真实阶段/结果/汇总。`wait` 超时不代表计算停止。 |
| `cancel` | 未开始项立即取消；运行项显示取消中，实际返回/清理后才释放槽位；已提交成功不改写为取消。 |
| `retry` | 批次结束后，只重试显式选择的失败/取消行，固定新的设置和目标；成功/无收益行保留参数、结果和备份信息。 |
| `clear` | 仅清除已结束记录并返回最后快照，不删除产物或备份；换批同样不会删除文件。 |
| `shutdown` / `Drop` | 服务自身取消并锁外等待所有线程；无法硬中断的编码可能延长退出。显式关闭可返回基础设施错误。 |

- 默认 1 worker、最多 1000 行、4 GiB **估算准入预算**；可配置 worker 为 1–32、行数为 1–100000，预算必须非零。估算按请求上限计算：`8×max_input_bytes + 8×max_decoded_bytes + 有损时128×max_pixels + 16 MiB`。预算不实际预分配，也不是进程 RSS 上限；没有峰值 RSS 实测前不提高默认并发。默认有损上限下，即使配置两个 worker 也可能因预算而串行。
- 预检拒绝重复源/硬链接、重复目标及输出覆盖其他输入；单项坏文件、目标已存在、超预算等独立失败，不中断其余项。预检关闭全部身份句柄后才编码，最终文件仍仅由 output 层复查和提交。Windows 比较键保守折叠大小写，可能过度拒绝；其他平台不存在的目标别名可能直到最终提交才冲突，不承诺跨文件系统预检完全一致或抵御外部路径竞争。
- 单项执行异常转为失败并释放预算；`WorkerPanicked` 的文件结果未知，需先检查源/目标/备份。锁故障停止接纳但仍可读快照；`CleanupFailed` 保留原始错误和残留路径，不伪装成成功取消。
- 汇总仅成功项计节省量，其余保留源大小；未知大小/溢出返回 `None`。阶段与数量不是耗时百分比。批量模型保留 `PathBuf`、`Duration` 和错误上下文，**不是 IPC DTO**，TS 生成文件本阶段不变。

本阶段 Windows 通过新增 19 项批量回归及 1 项编译型 doctest；目录扫描、Tauri DTO/订阅和正式界面依次见 [批量桌面计划](docs/devlog/_plan/260922/png-batch-desktop.md)。

## 工程边界

```text
src/                        React 界面
  app/                      启动页、语言和主题装配
  components/ui/            通用可访问控件
  lib/ipc/                  原生调用边界与生成类型
  stores/                   版本化偏好设置
  styles/tokens/            亮暗主题变量
src-tauri/                  Tauri 2 桌面壳、命令、权限和图标
crates/pixofold-core/        不依赖 Tauri 的 Rust 核心与数据模型
tools/                      类型生成/一致性检查
.github/workflows/ci.yml     三平台检查与可执行文件构建配置
docs/                       方案、HTML 原型和开发交接
```

最小链路为 `getAppInfo()` → Tauri `get_app_info` → `pixofold_core::app_info()`，返回真实核心版本及规划能力，不提供假压缩。主窗口只授权该命令，尚未接入文件系统、dialog、opener 或 shell 插件。生产 CSP 保持同源资源，开发 CSP 仅额外允许本机 Vite/HMR 所需连接和样式。

Rust DTO 通过可选 `bindings` feature 使用 ts-rs 12 生成 TypeScript，普通核心与桌面发布不启用该工具。TypeScript 7 超过当前 typescript-eslint 的声明兼容范围，因此采用 Oxlint 做 lint，并由 TypeScript 编译器执行严格类型检查。

2026-09-21 脚手架已通过冻结安装、统一检查、Windows 可执行文件构建/启动及浏览器外观交互验证。2026-09-22 无损提交 `3f6c617`、包含有损功能的 `ab9b2bb` 分别通过 Windows、macOS、Ubuntu 的 CI 统一检查和桌面构建。新增批量代码通过 Windows `pnpm check`（54 项 Rust 测试、1 项编译型 doctest、5 项前端测试及 32 份语料清单）及 `pnpm tauri build --no-bundle --ci`；尚无本轮批量代码的远端 CI。本轮未重新执行 GUI 视觉/运行验收，安装包仍未验证。完整证据与后续交接见 [开发记录](docs/devlog/README.md)。

## 项目方案

设计、图片引擎选型、架构、实施阶段和验收标准见 [项目方案](docs/架构设计文档/pixofold-proposal.md)。

- [UI 交互与质量映射设计](docs/架构设计文档/ui-interaction-design.md)
- [亮暗主题界面设计图](docs/UI界面设计/README.md)
- [HTML 交互原型](docs/UI界面设计/PixoFold.html) · [使用与修改说明](docs/UI界面设计/HTML原型说明.md)

## 开源协议

除另有声明外，本项目依据 GNU General Public License 第 3 版或其后版本（`GPL-3.0-or-later`）发布，完整条款见 [LICENSE](LICENSE)。

第三方组件保留各自的版权和许可声明，当前直接依赖见 [第三方组件说明](THIRD_PARTY_NOTICES.md)。本阶段接入 oxipng / png / imagequant，并通过 libdeflater 静态构建 libdeflate；imagequant 按 GPLv3+ 使用，尚未分发独立编码工具。
