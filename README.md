# PixoFold（轻图）

基于 Tauri、React 和 Rust 的本地批量图片压缩工具，计划支持 PNG、JPEG、GIF 和 APNG，由 png-palettes 重构演进。

当前已建立 **Tauri 2 + React + TypeScript + Rust workspace 脚手架**，实现可独立调用的 **静态 PNG 无损/有损核心、纯 Rust 批量任务服务及统一导入/输出规划**。P3a 提供应用级任务协调器，桌面持有唯一服务并在常规退出时取消、等待线程收尾；P3b 首步新增任务 DTO 与有界只读快照 IPC。桌面界面仍为工程启动页，包含亮暗主题、中英文、偏好持久化和构建信息；**Channel 订阅、原生导入入口、任务变更 IPC 与正式业务界面尚未接通。** 包含 P3a 的 `36a1174` 已通过三平台 CI 检查/桌面构建；本轮新增代码的验证单独记载，不沿用前序证据。

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
- 有损模式采用 imagequant 的 min=0、target=q（映射版本 1），独立固定 speed=4、dither=1.0。实际 remapping 评分低于 q、透明端点/半透明保护不通过，或体积不优于源文件及无损候选时，明确回退到严格无损。100 不等同于通用无损；评分也不等同于 SSIM。
- 16-bit、ICC/cHRM/HDR、依赖原始表示的 sBIT/bKGD 等元数据保守回退；不隐式降位深或删除颜色信息。支持有效 gAMA 和 sRGB，保留适用元数据及其 IDAT 前后位置。纯透明 alpha=0、不透明 alpha=255 必须保持；半透明仍在 1–254 且 alpha 误差不超过 8。有损不承诺透明像素的隐藏 RGB 不变。
- `PngRequest::new` 继续默认无损，显式设置 `PngMode::Lossy` 才进行量化；`QualityValue` 默认 80。产品界面默认有损 80 的设计尚未接入。报告区分实际量化、保护性回退与无收益，并返回输入/实际输出属性。
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

[桌面IPC模块](src-tauri/src/ipc/mod.rs) 与 [前端适配器](src/lib/ipc/tasks.ts) 复用唯一 TaskControl。主窗口可调用 get_task_snapshot 查询状态；不启动导入、重试或压缩，也不创建第二个任务服务。界面尚未使用该接口，compressionAvailable 仍为 false。

- Rust DTO 与协议常量为权威来源，生成 [tasks.generated.ts](src/lib/ipc/tasks.generated.ts)；原有核心 generated.ts 和 get_app_info 契约保持不变。协议版本当前为 1。
- 应用/批次 revision、selection/batch ID、字节数、elapsedMs 均为规范 u64 十进制字符串，前端用 BigInt 比较；不经过 Number。毫秒向下取整，异常溢出返回 invalid_snapshot；未知大小保留 null。行 ID、候选/问题索引、attempt 与数量使用检查过的整数；行身份由 selection/batch/id/attempt 共同界定，不是文件名。
- 查询指定 jobs、candidates 或 issues，limit 为 1–100。offset=0、expectedRevision=null 读取最新版本；后续页必须带该 revision。一次响应的摘要与行来自同一份 Arc 快照；版本不匹配返回 stale_snapshot/currentRevision，应丢弃旧分页并从第一页重新读取，不拼接不同版本，也不保存无限历史快照。高频变化时翻页可能反复失效；本阶段不保证活跃批次全量遍历，后续订阅需按视口更新并在终态恢复完整清单。
- 只转换被请求的最多100行，所有展示名最多240个Unicode字符，分别标明截断、非Unicode替换及控制字符清理；只返回文件名，不返回完整路径、图像数据或底层错误字符串。错误保持稳定代码，清理失败可同时保留原始取消/错误及备份/临时产物名称。原生恢复路径仍留在Rust，展示名不构成定位、读取或覆盖权限。
- TaskSnapshotReader 管理单个可见页面：仅应用最后一次查询，拒绝版本回退，卸载后丢弃迟到成功/失败；错误保留最后一份好快照。没有定时轮询或隐式重跑；浏览器返回 null，不模拟后端。
- 仅主窗口授权 allow-get-task-snapshot；未开放通用文件读写、dialog、opener 或 shell。Rust 请求反序列化拒绝额外字段/非法表示；合法结构但非法分页返回稳定错误码，传输/反序列化失败仍由 Tauri 错误通道报告。

下一步继续 P3b：先设计有界 Channel 订阅、确认/替换/销毁和断线恢复，保证终态可查询，不用每100ms广播全量数组；随后接原生选择/拖放授权与任务变更命令，再由 P4 沿既有原型接入真实交互。不要按窗口或命令创建 TaskRuntime。

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

启动页链路为 `getAppInfo()` → Tauri `get_app_info` → `pixofold_core::app_info()`；只读任务链路为 `getTaskSnapshot()` → get_task_snapshot → 应用 TaskControl 快照。主窗口只授权这两个查询命令，尚未接入文件系统、dialog、opener 或 shell 插件。生产 CSP 保持同源资源，开发 CSP 仅额外允许本机 Vite/HMR 所需连接和样式。

Rust DTO 通过可选 `bindings` feature 使用 ts-rs 12 生成 TypeScript，普通核心与桌面发布不启用该工具。TypeScript 7 超过当前 typescript-eslint 的声明兼容范围，因此采用 Oxlint 做 lint，并由 TypeScript 编译器执行严格类型检查。

pnpm types:generate/types:check 同时运行核心与桌面生成器，后者需具备桌面编译系统依赖，但不启动窗口或任务线程。桌面全部单元/mock测试由显式 tests/desktop.rs target 承载（库默认 test harness 关闭以免重复运行），Windows MSVC 通过 build.rs 复用 Tauri 生成的资源 manifest；这保证 mock 使用的 Common Controls v6 可加载，不修改发行行为或跳过旧测试。

2026-09-21 脚手架已通过冻结安装、统一检查、Windows 可执行文件构建/启动及浏览器外观交互验证。2026-09-22 无损 `3f6c617`、有损 `ab9b2bb`、包含 P1/P2 的 `11c55fa` 分别通过三平台 CI 统一检查和桌面构建；最后一轮为 run `35713030818`，已复验旧批量提交的 Ubuntu/macOS 测试导入修复。

P3a 在 Windows 通过 `pnpm check`（90 项 Rust 测试、2 项编译型 doctest、5 项前端测试及 32 份语料清单）和 `pnpm tauri build --no-bundle --ci`；包含相同业务代码的 `36a1174` 已通过三平台 CI（run35808907663）。历史 release 空闲关闭退出码为 0，但 stderr 有 `Chrome_WidgetWin_0` 注销告警（1412），仍待排查。原生导入/处理中的 GUI 关闭、完整任务订阅、视觉复验、峰值 RSS 及安装包尚未验收；P3b 新代码的本机验证单独见 [开发记录](docs/devlog/README.md)，不沿用旧 CI 或空闲窗口冒烟证据。

## 项目方案

设计、图片引擎选型、架构、实施阶段和验收标准见 [项目方案](docs/架构设计文档/pixofold-proposal.md)。

- [UI 交互与质量映射设计](docs/架构设计文档/ui-interaction-design.md)
- [亮暗主题界面设计图](docs/UI界面设计/README.md)
- [HTML 交互原型](docs/UI界面设计/PixoFold.html) · [使用与修改说明](docs/UI界面设计/HTML原型说明.md)

## 开源协议

除另有声明外，本项目依据 GNU General Public License 第 3 版或其后版本（`GPL-3.0-or-later`）发布，完整条款见 [LICENSE](LICENSE)。

第三方组件保留各自的版权和许可声明，当前直接依赖见 [第三方组件说明](THIRD_PARTY_NOTICES.md)。本阶段接入 oxipng / png / imagequant，并通过 libdeflater 静态构建 libdeflate；imagequant 按 GPLv3+ 使用，尚未分发独立编码工具。
