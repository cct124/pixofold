# PixoFold（轻图）

基于 Tauri、React 和 Rust 的本地批量图片压缩工具，计划支持 PNG、JPEG、GIF 和 APNG，由 png-palettes 重构演进。

当前已接通 **静态 PNG 真实工作台**：Tauri 2 + React 界面经受控原生选择、应用级任务协调和有界快照驱动独立 Rust 无损/有损核心，支持文件/目录导入、批量处理、备份覆盖/同目录副本、取消、重试与清除记录。大小、状态和进度均来自实际任务。**仅支持静态 PNG；拖放、自选输出目录、缩略图及其他格式尚未接入。** compressionAvailable=true仅表示已有一种可用压缩格式，不表示全部plannedFormats已实现。含原生入口的584d1e9已通过三平台CI；本轮工作台验证见[开发记录](docs/devlog/_plan/260922/png-batch-desktop.md)，不沿用旧CI作为新代码证据。

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
| `pnpm types:generate` | 从 Rust DTO 更新 `src/lib/ipc/generated.ts` 与 `tasks.generated.ts` |
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
- 含 C2PA 内容凭据容器（`caBX`）的 PNG 当前不能安全更新凭据并压缩，会保留原图并明确提示不支持；切换无损或调整质量不能解决，不会自动剥除凭据。其他未知 unsafe-to-copy 元数据同样拒绝。识别块类型不表示已验证凭据签名/真实性，这属于输入能力边界，不是产物验证失败。
- 有损模式采用 imagequant 的 min=0、target=q（映射版本 1），独立固定 speed=4、dither=1.0。实际 remapping 评分低于 q、透明端点/半透明保护不通过，或体积不优于源文件及无损候选时，明确回退到严格无损。100 不等同于通用无损；评分也不等同于 SSIM。
- 16-bit、ICC/cHRM/HDR、依赖原始表示的 sBIT/bKGD 等元数据保守回退；不隐式降位深或删除颜色信息。支持有效 gAMA 和 sRGB，保留适用元数据及其 IDAT 前后位置。纯透明 alpha=0、不透明 alpha=255 必须保持；半透明仍在 1–254 且 alpha 误差不超过 8。有损不承诺透明像素的隐藏 RGB 不变。
- `PngRequest::new` 继续默认无损，显式设置 `PngMode::Lossy` 才进行量化；`QualityValue` 默认 80，工作台默认有损80。报告区分实际量化、保护性回退与无收益，并返回输入/实际输出属性。
- 默认请求采用原图覆盖，但每次成功覆盖都保留同目录 `.pixofold-backup-*.png` 原始备份，路径在结果中返回，核心不会自动删除。提交失败也返回备份路径。确认新图可用后再由用户处理备份。
- `OutputPolicy::Copy` 副本必须给出完整新路径、父目录必须存在，同名、目录或链接冲突均拒绝；新增的 `CopyTree` 显式允许输出层在既有目标根内创建结构目录，契约见下方导入入口。无收益不创建最终副本或备份；只读输入允许另存，副本不继承只读属性。
- 输入默认 64 MiB，单个解码缓冲区 128 MiB，16M 像素、单边 16384；有损 RGBA 展开另按缓冲区上限检查。单文件同步、编码器单线程；oxipng 有 30 秒软期限，imagequant 在进度回调协作取消，但无硬超时。这不是进程硬内存限额或即时取消保证；批量调用由下述任务服务额外限制文件并发和估算工作集。
- 无损/有损单文件核心均已通过 Windows 本机及三平台 CI 核心文件测试/桌面构建。文件同步不等于目录元数据的断电事务；源文件关闭到替换仍有外部竞争窗口，不承诺网络文件系统、特殊 ACL/ADS、xattr 或断电恢复。正式桌面接入前继续平台和真实业务样本验收。

样本与再生成方式见 [语料说明](tests/fixtures/README.md)，实际基线、错误边界及后续动作见 [无损核心归档](docs/devlog/_fin/260922/png-core-foundation.md)与 [PNG 有损质量归档](docs/devlog/_fin/260922/png-lossy-quality.md)。

## 纯 Rust 批量开发入口

`batch::BatchService` 接收 `BatchRequest { items, parameters }`，每项显式提供源路径与 `OutputPolicy`。API 用法及可编译示例见 [服务入口](crates/pixofold-core/src/batch/mod.rs)，真实混合批次及备份回归见 [集成测试](crates/pixofold-core/tests/png_batch.rs)。服务本身不扫描目录、不生成副本文件名、不连接窗口；文件/目录入口由下一节的 `import` 模块提供。核心参数默认无损，产品有损 80 仍由后续适配层显式传入。

| API | 契约 |
| --- | --- |
| `new` / `start` | 创建固定线程池；同步只读预检显式文件列表，再后台处理。同一服务只允许一个活动批次，只保留最近批次。调用方应在非 UI 线程执行预检。 |
| `start_with_cancel` / `retry_with_cancel` | 共享应用取消令牌；准入前取消返回 `BatchError::Cancelled`，不创建新批次/尝试；入队后由核心协作取消，已提交成功不改写。旧 `start`/`retry` 用法不变。 |
| `snapshot` / `wait` | Rust 权威快照含批次 ID、稳定行 ID、attempt、revision、真实阶段/结果/汇总。`wait` 超时不代表计算停止。 |
| `cancel` | 未开始项立即取消；运行项显示取消中，实际返回/清理后才释放槽位；已提交成功不改写为取消。 |
| `retry` | 批次结束后，只重试显式选择的失败/取消行，固定新的设置和目标；成功/无收益行保留参数、结果和备份信息。 |
| `clear` | 仅清除已结束记录并返回最后快照，不删除产物或备份；换批同样不会删除文件。 |
| `shutdown` / `Drop` | 服务自身取消并锁外等待所有线程；无法硬中断的编码可能延长退出。显式关闭可返回基础设施错误。 |

- 默认 1 worker、最多 1000 行、4 GiB **估算准入预算**；可配置 worker 为 1–32、行数为 1–100000，预算必须非零。估算按请求上限计算：`8×max_input_bytes + 8×max_decoded_bytes + 有损时128×max_pixels + 16 MiB`。预算不实际预分配，也不是进程 RSS 上限；没有峰值 RSS 实测前不提高默认并发。默认有损上限下，即使配置两个 worker 也可能因预算而串行。
- 预检拒绝重复源/硬链接、重复目标、输出覆盖其他输入，以及一个计划产物同时成为另一产物父目录；单项坏文件、目标已存在、超预算等独立失败，不中断其余项。预检关闭全部身份句柄后才编码，最终文件仍仅由 output 层复查和提交。Windows 比较键保守折叠大小写，可能过度拒绝；其他平台不存在的目标别名可能直到最终提交才冲突，不承诺跨文件系统预检完全一致或抵御外部路径竞争。
- 单项执行异常转为失败并释放预算；`WorkerPanicked` 的文件结果未知，需先检查源/目标/备份。锁故障停止接纳但仍可读快照；`CleanupFailed` 保留原始错误和残留路径，不伪装成成功取消。
- 汇总仅成功项计节省量，其余保留源大小；未知大小/溢出返回 `None`。阶段与数量不是耗时百分比。批量模型保留 `PathBuf`、`Duration` 和错误上下文，**不是 IPC DTO**，TS 生成文件本阶段不变。

P1 新增的 19 项批量回归及 1 项编译型 doctest 在本轮 Windows 检查中继续通过；后续 Tauri DTO/订阅和正式界面见 [批量桌面计划](docs/devlog/_plan/260922/png-batch-desktop.md)。

## 纯 Rust 导入与输出规划入口

调用链为 `import::scan` → `ImportScan::plan` → `BatchService::start` → 原有 pipeline/output。用法见 [导入入口与可编译示例](crates/pixofold-core/src/import/mod.rs)，隔离目录的真实闭环见 [导入集成测试](crates/pixofold-core/tests/png_import.rs)。扫描与规划均为同步、只读 API，后续 Tauri 必须在有界后台任务中调用，不能阻塞 UI。

- 文件与目录列表共用入口，目录内排序、根列表按传入顺序；重叠目录和已接受候选的硬链接去重，以首次归属决定输出布局。不跟随叶节点或枚举节点的符号链接/Windows reparse；显式祖先别名仍沿用规范化边界，不是文件系统沙箱。
- 显式根硬上限为 1000，且不得超过条目限制；默认 1000 个候选、10000 个发现条目、32 层目录及累计读取 1 GiB，单文件沿用 64 MiB 等资源限制。候选数可设 1–100000、条目数 1–1000000、深度 0–256（0 允许根目录直接文件），累计读取预算须非零。条目与每 64 KiB 读取边界检查取消；一次只保留一张图片的压缩数据，不展开像素。句柄数量有界但仍可能受系统限制，OS 调用和单次 CRC 不能硬中断。
- 按内容识别静态 PNG，检查 chunk/CRC、头部资源上限并拒绝 APNG；JPEG/GIF/WebP 等明确反馈未支持。扫描候选不是完整解码成功的保证，坏像素数据仍可能在流水线失败；逐项错误不阻断其他条目。
- 仅完整扫描且有候选时允许规划。取消或触及全局限制返回可展示的部分结果，**不得自动启动部分批次**；空输入不建立批次。参数/目标预检失败保留冻结清单，修正后可重新规划；冻结的是路径与归属，不锁定文件内容，实际处理仍重新校验。
- `Overwrite` 保持覆盖备份契约；`CopyBeside` 输出到原目录；`CopyTo` 支持指定目录的扁平或 `PreserveRoots` 布局。副本统一为 `stem_compressed.png`，不沿用误导性扩展名；保留结构时为 `目标/导入根名/相对父目录/副本名`，单独文件直接放目标根。无名称的文件系统根使用 `_root`。不同根同名、扁平重名、输出与输入交叉均拒绝，不自动编号或覆盖。
- 选定目标根必须已存在；`OutputPolicy::CopyTree { root, relative }` 只接受普通相对组件，规划不创建目录。只有 output 暂存阶段创建必要子目录，最终仍执行 noclobber；取消/失败/无收益可能保留空结构目录，避免并发任务误删共有目录。旧 `Copy` 契约不变；Rust 下游完整匹配 `OutputPolicy` 时需处理新增变体，IPC/TS 类型未变。
- 默认排除输出层保留名：`.pixofold-output-` + 六位 ASCII 字母数字 + `.tmp`，以及 `.pixofold-backup-` + 六位 ASCII 字母数字 + `.png`；可用 `include_artifacts` 显式包含。命名匹配不是来源证明，合法用户文件恰好使用保留名也会收到排除反馈；普通隐藏图和 `_compressed` 图片不排除。目标在输入树内时，必须先扫描结束，再开始写产物。

P2 新增 19 项 Windows 导入回归及 1 项编译型 doctest；包含 Unix 符号链接用例的适用平台测试已随 `11c55fa` 的三平台 CI 通过。

## 应用级任务协调（P3a）

桌面库的 [tasks 模块](src-tauri/src/tasks/mod.rs) 在核心之上统一管理导入、参数修正、启动、取消、重试和清除；[退出桥](src-tauri/src/lifecycle.rs) 将所有者装配到 Tauri 应用。模块本身无窗口/IPC 依赖，可用 [真实文件与确定性门闩测试](src-tauri/src/tasks/tests.rs) 脱离 GUI 验收，不等于前端已经可调用压缩。

- `TaskRuntime` 是唯一线程所有者，持有一个协调线程及核心配置内的固定编码 worker；`TaskControl` 克隆不新建服务，句柄销毁不取消后台任务，窗口重载不改变所有权。默认仍为 1 个编码 worker，资源上限不提高。
- 单槽命令接纳覆盖扫描、待修正清单、规划、运行与清除。`import(roots, Some(settings))` 完整扫描后自动规划启动；`None` 等待设置，`start(selection, settings)` 只允许 Ready。参数/输出错误保留同一冻结清单，取消/超限部分结果不能启动。接纳成功不代表后台处理成功，结果通过快照读取。
- `SelectionId` 防止旧导入请求操作新清单；重试还要求最新批次 revision，防止旧请求重复增加 attempt。核心继续按行校验、保留成功项及备份；清除返回旧快照供恢复信息留存，不删除文件。Rust 快照不是 IPC DTO，也不承诺跨进程恢复/自动续跑。
- 扫描回调提供真实计数；活跃批次按 100ms 间隔采样核心快照，空闲等待通知，不忙轮询。`wait_for_change` 使用单调应用 revision，超时只停止等待；阶段/数量不是编码耗时百分比，轮询采样不是逐事件无损日志。批次全部取消后外层仍为 Finished，具体结果以核心状态和计数为准。
- 取消令牌贯穿扫描、只读规划、批次预检和执行，避免预检期间取消后仍正常入队。外部取消被观察后与核心 revision 一起更新，不暴露相同版本却不同阶段的快照；重复取消不持续唤醒空闲 worker。
- 主窗口关闭或常规退出先停止接纳，只有一个后台收尾任务等待协调/编码线程全部 join，再允许事件循环退出；不得在 UI 线程等待。不可中断的 OS/编码调用可能延长关闭，OS 强杀/断电/重启不提供恢复保证；关闭提示和 UI 状态尚待 P4。
- `TaskSettings::default()` 明确采用产品默认有损 80、原图覆盖；核心 `PngRequest` 默认无损不变。新增 `BatchError::Cancelled` 需要 Rust 下游完整 match 补分支；桌面 `run` 返回可包含任务初始化原因的错误链。P3a阶段未改任务 DTO、TS、权限和现有 `get_app_info` 契约，`compressionAvailable` 仍为 false；后续只读查询见下节。

## 任务只读快照（P3b第一步）

[桌面IPC模块](src-tauri/src/ipc/mod.rs) 与 [前端适配器](src/lib/ipc/tasks.ts) 复用唯一 TaskControl。主窗口可调用 get_task_snapshot 查询状态；该查询本身不启动导入、重试或压缩，也不创建第二个任务服务。P4工作台通过订阅复用此有界查询。

- Rust DTO 与协议常量为权威来源，生成 [tasks.generated.ts](src/lib/ipc/tasks.generated.ts)；原有核心 generated.ts 和 get_app_info 契约保持不变。协议版本当前为 1。
- `JobErrorDto` 新增 `unsupported_content_credentials` / `unsupported_metadata`，分别表示 caBX 与其他不支持安全改写的元数据；嵌套恢复原因使用同样类别，真正的产物验证错误仍为 `validation`。消息结构/协议版本未变；桌面前后端必须随同一构建更新，外部穷举消费者需增加分支，不能用旧前端混接新后端。
- 应用/批次 revision、selection/batch ID、字节数、elapsedMs 均为规范 u64 十进制字符串，前端用 BigInt 比较；不经过 Number。毫秒向下取整，异常溢出返回 invalid_snapshot；未知大小保留 null。行 ID、候选/问题索引、attempt 与数量使用检查过的整数；行身份由 selection/batch/id/attempt 共同界定，不是文件名。
- 查询指定 jobs、candidates 或 issues，limit 为 1–100。offset=0、expectedRevision=null 读取最新版本；后续页必须带该 revision。一次响应的摘要与行来自同一份 Arc 快照；版本不匹配返回 stale_snapshot/currentRevision，应丢弃旧分页并从第一页重新读取，不拼接不同版本，也不保存无限历史快照。高频变化时翻页可能反复失效；本阶段不保证活跃批次全量遍历，后续订阅需按视口更新并在终态恢复完整清单。
- 只转换被请求的最多100行，所有展示名最多240个Unicode字符，分别标明截断、非Unicode替换及控制字符清理；只返回文件名，不返回完整路径、图像数据或底层错误字符串。错误保持稳定代码，清理失败可同时保留原始取消/错误及备份/临时产物名称。原生恢复路径仍留在Rust，展示名不构成定位、读取或覆盖权限。
- TaskSnapshotReader 管理单个可见页面：仅应用最后一次查询，拒绝版本回退，卸载后丢弃迟到成功/失败；错误保留最后一份好快照。没有定时轮询或隐式重跑；浏览器返回 null，不模拟后端。
- 仅主窗口授权 allow-get-task-snapshot；未开放通用文件读写、dialog、opener 或 shell。Rust 请求反序列化拒绝额外字段/非法表示；合法结构但非法分页返回稳定错误码，传输/反序列化失败仍由 Tauri 错误通道报告。

## 有界只读订阅（P3b第二步）

[订阅服务](src-tauri/src/subscriptions/mod.rs)随应用创建唯一通知线程，复用TaskControl，不再创建任务服务。三个主窗口命令为subscribe_task_changes、acknowledge_task_changes和unsubscribe_task_changes；仅管理观察关系，不导入、启动、取消或重试压缩任务。

- subscribe替换唯一会话，返回protocolVersion/subscriptionId/revision票据；ID和版本仍是规范u64十进制字符串。首次票据未经确认不发送Channel通知，前端先查询快照再ACK，避免订阅建立期间漏变化。
- 每会话至多一条在途通知，内容只有上述三个字段；没有任务行队列或每100ms全量广播。确认后直接观察最新revision，中间阶段可以合并，终态通过权威快照恢复，不承诺逐事件日志。
- ACK只接受当前会话实际在途版本；重复最近已确认版本幂等，但不能释放后续票据；未来版本拒绝。旧会话ACK报stale_subscription，旧unsubscribe返回false，不干扰新会话。投递失败移除对应订阅，不影响任务。
- 无订阅、待确认或已Closed无新版本时Condvar休眠；其他时候有界等待任务变化，每100ms超时检查订阅控制状态，以便替换/退出收尾。发送、Channel释放和join均不持状态锁；退出同时停止订阅接纳并取消任务，在同一后台收尾路径join两个所有者。
- [TaskSnapshotSubscription](src/lib/ipc/task-subscription.ts)是应用级单页适配器：connect后查询所选集合第一页（最多100行），每次通知读取快照再确认；页面最多一个连接，最多一个查询和一个待处理通知。拒绝倒退快照、忽略旧会话/迟到结果，错误保留最后好快照。其他页仍由TaskSnapshotReader管理同revision分页，界面不应按组件反复创建观察器。
- 正常disconnect由Rust释放Channel并发送end帧，后续connect重新查询最新状态；没有自动无限重连或轮询探活，connected表示已建立观察关系，不保证传输持续可达。锁定SDK没有公开Channel.close接口：首次注册响应失败且会话归属未知时进入reload_required并保留页面槽，须重载WebView释放回调；已知会话清理失败可重试disconnect。不能用SDK私有接口强行清理，也不能把停止等待说成资源已释放。

不要按窗口或命令创建TaskRuntime或SubscriptionRuntime；原生WebView订阅/重载仍需单独验收。

## 受控原生选择与任务操作（P3b第三步）

[原生授权槽](src-tauri/src/ingress/mod.rs)、[命令适配](src-tauri/src/commands/mutations.rs)和[TaskActions](src/lib/ipc/task-actions.ts)复用当前已握手的应用订阅与TaskControl，不把旧项目的path数组IPC迁入新工程。

- select_native_import只接受files/folder与subscriptionId，通过Rust侧tauri-plugin-dialog 2.7.3打开主窗口所属原生对话框。最多一个物理对话框；等待由有界占位后的后台任务承担，不持服务锁或阻塞UI。取消/空选择返回null，不建任务；SDK也可能把系统对话框失败表示为null，不能解读为成功处理。插件未提供显式关闭对话框API，重载不释放物理占位；应用退出先撤销接纳/授权，晚到结果不启动任务，原生退出行为待GUI验收。
- 路径只留Rust，前端仅得到grantId/rootCount（不是扫描数量、展示名或路径凭据）。授权绑定订阅会话，最多1000根、原生路径编码长度总和不超过1 MiB，5分钟惰性过期；新选择替换旧授权，成功导入消费一次，忙状态拒绝不消费。根数和重试行数上限随Rust DTO生成；路径身份、内容及输出安全继续由核心复查。
- apply_task_mutation接收可辨识操作联合。import使用授权及固定settings；settings=null只扫描，非null完整扫描后自动启动。start仅用于Ready清单修正。模式复用严格PngMode，输出仅overwrite/copy_beside，不允许任意路径、输出目录或资源预算字符串/覆盖。
- cancel/clear/start携带当前selectionId；retry额外要求expectedBatchRevision和最多1000个唯一失败/取消行ID（不是数组/页码索引），沿用Rust原行输出目标，仅修改模式，成功/无收益项不重跑。clear只清记录，不删除原图、结果或备份；UI须先展示需保留的恢复信息。
- 首次查询/ACK握手完成后才能操作。后端在订阅锁内原子校验会话并短时接纳，锁序为订阅→授权槽→任务，无文件I/O/await；旧会话、旧授权、旧selection/revision不影响新任务。命令返回selectionId仅表示接纳，后台成功/失败仍经任务快照查询。
- TaskActions固定点击时的参数并限制一个在途操作；默认有损80/覆盖，浏览器明确不可用。选择期间断开/重连的迟到结果不能自动启动；操作传输失败可能已经接纳，不自动重试，应先恢复权威快照。P4工作台复用此适配器，不构造模拟进度。

## PNG 工作台（P4最小纵向闭环）

[Workspace](src/features/workspace/Workspace.tsx)沿HTML原型呈现中央列表、右侧设置与固定底部汇总。[控制器](src/features/workspace/controller.ts)是页面唯一会话所有者；StrictMode、外观切换与组件重挂载不创建重复Channel，不取消后台任务。卸载后异步释放订阅，重挂载等待旧会话清理。

- 选择文件/目录后自动扫描及处理，默认有损80/覆盖。质量仅接受完整0–100整数；空值/非法文本可导入但只扫描，修正后同一清单自动启动一次。Esc恢复最后合法值，无损不携带质量。只持久化合法设置，不保存授权、任务或临时输入文本。
- 点击导入时冻结参数；运行中调整草稿不修改批次。后台规划失败保留清单与错误，同设置不会无限重发；重载恢复的Ready清单需用户主动调整设置才继续。操作返回仅表示接纳，连接或写操作失败保留最后快照、禁止进一步写入，要求显式重连；未知会话归属要求重载页面。
- jobs/candidates/issues每页50项；状态更新时回到首屏，额外分页最多一个在途查询。过期页不拼接，按最新revision恢复。未知大小显示“—”，精确字节按bigint计算；进度为processed/total，取消不冒充100%成功。
- 重试只接纳当前同revision可见页内失败/取消行的稳定ID，使用草稿模式但保留原输出位置；清除需确认，仅清记录，不删除文件。结果展示实际回退原因、输出/备份名及失败恢复信息；展示名不是可访问路径，截断或替换明确标注。
- main页面Started生命周期在同一订阅锁域撤销会话与授权，不清除或重跑任务；尚未关闭的物理对话框仍占槽，晚到结果拒绝后才释放。非main或Finished事件不撤销新会话，不依赖unload必达。
- 浏览器仅预览，不读取图片或模拟业务。未开放拖放、自选输出目录、缩略图、高级编码参数、新格式和通用fs/dialog/event权限；这些入口及原生GUI覆盖范围按开发记录后续验收。

## 工程边界

```text
src/                        React 界面
  app/                      工作台、语言和主题装配
  features/workspace/        真实列表、设置草稿与唯一页面控制器
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

版本链路为 `getAppInfo()` → Tauri `get_app_info` → `pixofold_core::app_info()`；只读任务链路为 `getTaskSnapshot()` → get_task_snapshot → 应用TaskControl快照。主窗口授权两个查询、三个订阅管理及两个受控选择/任务操作命令。dialog仅由Rust调用，未授予dialog/fs/event/opener/shell通用前端权限；传递依赖tauri-plugin-fs不等于启用其插件或权限。生产CSP保持同源资源，开发CSP仅额外允许本机Vite/HMR所需连接和样式。

Rust DTO 通过可选 `bindings` feature 使用 ts-rs 12 生成 TypeScript，普通核心与桌面发布不启用该工具。TypeScript 7 超过当前 typescript-eslint 的声明兼容范围，因此采用 Oxlint 做 lint，并由 TypeScript 编译器执行严格类型检查。

pnpm types:generate/types:check 同时运行核心与桌面生成器，后者需具备桌面编译系统依赖，但不启动窗口或任务线程。桌面全部单元/mock测试由显式 tests/desktop.rs target 承载（库默认 test harness 关闭以免重复运行），Windows MSVC 通过 build.rs 复用 Tauri 生成的资源 manifest；这保证 mock 使用的 Common Controls v6 可加载，不修改发行行为或跳过旧测试。

生产与mock共用一次Tauri上下文宏展开和同一命令注册入口，避免macOS重复嵌入plist符号及测试路由漂移。mock IPC从WebView实际URL获取正常来源，分别覆盖配置中的开发地址与平台打包协议；其他窗口、远程/相似域名仍由真实capability拒绝，不为测试开放额外权限。这些回归不启动原生WebView，也不证明原生GUI导航/拖放已经验收。

2026-09-21 脚手架已通过冻结安装、统一检查、Windows 可执行文件构建/启动及浏览器外观交互验证。2026-09-22 无损 `3f6c617`、有损 `ab9b2bb`、包含 P1/P2 的 `11c55fa` 分别通过三平台 CI 统一检查和桌面构建；最后一轮为 run `35713030818`，已复验旧批量提交的 Ubuntu/macOS 测试导入修复。

P3a 在 Windows 通过 `pnpm check`（90 项 Rust 测试、2 项编译型 doctest、5 项前端测试及 32 份语料清单）和 `pnpm tauri build --no-bundle --ci`；包含相同业务代码的 `36a1174` 已通过三平台 CI（run35808907663）。历史 release 空闲关闭退出码为 0，但 stderr 有 `Chrome_WidgetWin_0` 注销告警（1412），仍待排查。原生导入/处理中的 GUI 关闭、完整任务订阅、视觉复验、峰值 RSS 及安装包尚未验收；P3b 新代码的本机验证单独见 [开发记录](docs/devlog/README.md)，不沿用旧 CI 或空闲窗口冒烟证据。

P3b只读查询048ecf8已于2026-09-23推送。后续38af28a的CI run35825516745仅Windows通过，Ubuntu来源权限测试和macOS重复plist符号失败；修复85d942a已推送，run35830576186三平台全部通过。订阅实现b0105b9随后推送，run35834973154的Windows/macOS/Ubuntu统一检查与桌面构建全部通过（Windows117项Rust运行测试、2项编译型doctest、31项前端测试）。这些结果不包含本轮未提交原生选择/任务操作，其本机验证见 [开发记录](docs/devlog/README.md)。只读查询、mock权限/Channel测试不等同于原生桌面压缩闭环验收。

## 项目方案

设计、图片引擎选型、架构、实施阶段和验收标准见 [项目方案](docs/架构设计文档/pixofold-proposal.md)。

- [UI 交互与质量映射设计](docs/架构设计文档/ui-interaction-design.md)
- [亮暗主题界面设计图](docs/UI界面设计/README.md)
- [HTML 交互原型](docs/UI界面设计/PixoFold.html) · [使用与修改说明](docs/UI界面设计/HTML原型说明.md)

## 开源协议

除另有声明外，本项目依据 GNU General Public License 第 3 版或其后版本（`GPL-3.0-or-later`）发布，完整条款见 [LICENSE](LICENSE)。

第三方组件保留各自的版权和许可声明，当前直接依赖见 [第三方组件说明](THIRD_PARTY_NOTICES.md)。本阶段接入 oxipng / png / imagequant，并通过 libdeflater 静态构建 libdeflate；imagequant 按 GPLv3+ 使用，尚未分发独立编码工具。
