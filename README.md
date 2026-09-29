# PixoFold（轻图）

基于 Tauri、React 和 Rust 的本地批量图片压缩工具，计划支持 PNG、JPEG、GIF 和 APNG，由 png-palettes 重构演进。

当前已接通 **静态 PNG 真实工作台**：Tauri 2 + React 界面经受控原生选择/拖放、应用级任务协调和有界快照驱动独立 Rust 无损/有损核心，支持文件/目录导入、批量处理、可选备份覆盖、同目录或指定目录副本、重试与清除记录。指定目录可选保留输入目录结构，授权仅本次连接有效。启动后不提供主动取消操作，退出时仍保留底层安全收尾。大小、状态和进度均来自实际任务。**仅支持静态 PNG；缩略图及其他格式尚未接入。** compressionAvailable=true仅表示已有一种可用压缩格式，不表示全部plannedFormats已实现。含原生选择入口的584d1e9曾通过三平台CI；当前代码与P5验证见[开发记录](docs/devlog/_plan/260922/png-batch-desktop.md)，不沿用旧GUI/CI作为新代码证据。

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
| `cargo run -p pixofold-core --release --locked --example batch_profile -- SOURCE WORKERS COPIES [--confirm-credentials]` | 临时副本上测量整图并发；有损68，采样RSS/活动数；可显式移除测试副本凭据 |

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
- 含 C2PA 内容凭据容器（`caBX`）的 PNG 默认保留原图并拒绝压缩。桌面批次结束后汇总“需要确认的图片”，列表全量显示、限高滚动、默认全选；点击“移除内容凭据后压缩”即授权本次选择。沿用打开时的输出设置：副本不显示额外输出模块；覆盖显示“覆盖原图 / 备份原图”，初值继承主设置备份开关，弹窗调整仅对本次选中图片生效。此操作不是更新/验证/重签凭据，其他未知保护块、损坏输入、真正验证失败不开放该选择，普通RGB正常处理。
- 有损模式采用 imagequant 的 min=0、target=q（映射版本 1），独立固定 speed=4、dither=1.0。实际 remapping 评分低于 q、透明端点/半透明保护不通过，或体积不优于源文件及无损候选时，明确回退到严格无损。100 不等同于通用无损；评分也不等同于 SSIM。
- 16-bit、ICC/cHRM/HDR、依赖原始表示的 sBIT/bKGD 等元数据保守回退；不隐式降位深或删除颜色信息。支持有效 gAMA 和 sRGB，保留适用元数据及其 IDAT 前后位置。纯透明 alpha=0、不透明 alpha=255 必须保持；半透明仍在 1–254 且 alpha 误差不超过 8。有损不承诺透明像素的隐藏 RGB 不变。
- `PngRequest::new` 继续默认无损，显式设置 `PngMode::Lossy` 才进行量化；`QualityValue` 默认 80，工作台默认有损80。报告区分实际量化、保护性回退与无收益，并返回输入/实际输出属性。
- 桌面“原图覆盖”在始终展开的高级选项中提供“覆盖前备份原图”，默认不勾选；旧偏好缺失/非法值按false处理，重启恢复有效布尔值，副本模式隐藏且忽略该开关。未勾选使用`OverwriteWithoutBackup`，不生成恢复备份；勾选映射`OutputPolicy::Overwrite`，成功覆盖保留同目录`<原文件名，不含扩展名>-backup-<6位随机标识>.png`完整原图，例如`风景.v2.png`对应`风景.v2-backup-Ab12xY.png`。随机标识使用ASCII字母数字，不是内容哈希；独占创建、碰撞重试，不覆盖或重命名历史备份。路径随结果/提交失败返回，不自动删除；完整原名若使路径超文件系统限制，失败并保留原图，不截断或静默取消备份。核心`PngRequest`和`ImportOutput`默认仍为备份覆盖，不静默修改核心调用方。两种覆盖都先验证临时产物、复查源文件后单次替换，不能先删除再写；无收益不生成备份，无备份提交失败不伪造恢复路径。
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

- 核心显式配置默认1 worker/1000行/4 GiB；worker有效域1–32。**正式桌面按系统资源自动配置线程池**：启动时读取可用CPU并行度及RAM，预算取可用RAM的一半、总RAM的四分之一、4 GiB中的最小值；池大小同时受CPU、32上限和每槽128 MiB基础预算限制。内存查询不可用时保守回退1 worker/256 MiB；低资源设备不强制多线程。此配置不实时追踪其他进程负载，也不是OS硬内存限额。
- 每张图片作为一个完整任务交空闲worker：读取/解码、受控移除caBX、压缩、验证和输出全在该worker中执行；普通导入、普通重试和凭据确认共用唯一池，凭据确认一次提交所选行，不由前端逐张调用。编码器内部仍关闭并行，避免线程池嵌套。移除只作用于内存工作副本，成功且有收益后才提交；保留备份、文件冲突和源变化检查。
- 调度前仅依据CRC有效的33字节PNG头与文件大小收紧该图执行上限：像素数与单边按图约束，解码缓冲按RGBA16最坏大小并留1 MiB最低余量，输入/候选上限覆盖源字节+RGBA16大小+1 MiB，均不超过用户原上限。完整pipeline实际采用同一上限，排队后图片变大不能绕过预算；头部不可用时沿用原始上限，不信任损坏尺寸。完整结构、动画和像素仍由原pipeline检查。
- 工作集按上述**执行上限**计费：`9×max_input_bytes + 8×max_decoded_bytes + 有损时128×max_pixels + 16 MiB`，包含凭据工作副本。单项超过总预算失败，多个任务预算不足时排队，真实返回才释放预约；不是预分配或真实RSS承诺。大图可能仍串行，小图可并行；公共快照仍保留用户设置，不将内部收紧上限写成新的用户参数。
- 重复源/硬链接仍整批准入拒绝；单项坏文件、目标已存在、超预算及副本输出冲突只标记该行失败，其余项继续。同批同目标的全部副本、计划文件兼作另一输出父目录的相关副本均失败，不让线程先后决定胜者；副本指向其他输入时仅副本失败，不连带阻止合法覆盖行。预检关闭全部身份句柄后才编码，最终文件仍仅由 output 层复查和无覆盖提交。Windows 比较键保守折叠大小写，可能过度拒绝；其他平台不存在的目标别名可能直到最终提交才冲突，不承诺跨文件系统预检完全一致或抵御外部路径竞争。
- 单项执行异常转为失败并释放预算；`WorkerPanicked` 的文件结果未知，需先检查源/目标/备份。锁故障停止接纳但仍可读快照；`CleanupFailed` 保留原始错误和残留路径，不伪装成成功取消。
- 汇总仅成功项计节省量，其余保留源大小；未知大小/溢出返回 `None`。阶段与数量不是耗时百分比。批量模型保留 `PathBuf`、`Duration` 和错误上下文，**不是 IPC DTO**，TS 生成文件本阶段不变。

P1 新增的 19 项批量回归及 1 项编译型 doctest 在本轮 Windows 检查中继续通过；后续 Tauri DTO/订阅和正式界面见 [批量桌面计划](docs/devlog/_plan/260922/png-batch-desktop.md)。

## 纯 Rust 导入与输出规划入口

调用链为 `import::scan` → `ImportScan::plan` → `BatchService::start` → 原有 pipeline/output。用法见 [导入入口与可编译示例](crates/pixofold-core/src/import/mod.rs)，隔离目录的真实闭环见 [导入集成测试](crates/pixofold-core/tests/png_import.rs)。扫描与规划均为同步、只读 API，后续 Tauri 必须在有界后台任务中调用，不能阻塞 UI。

- 文件与目录列表共用入口，目录内排序、根列表按传入顺序；重叠目录和已接受候选的硬链接去重，以首次归属决定输出布局。不跟随叶节点或枚举节点的符号链接/Windows reparse；显式祖先别名仍沿用规范化边界，不是文件系统沙箱。
- 显式根硬上限为 1000，且不得超过条目限制；默认 1000 个候选、10000 个发现条目、32 层目录及累计读取 1 GiB，单文件沿用 64 MiB 等资源限制。候选数可设 1–100000、条目数 1–1000000、深度 0–256（0 允许根目录直接文件），累计读取预算须非零。条目与每 64 KiB 读取边界检查取消；一次只保留一张图片的压缩数据，不展开像素。句柄数量有界但仍可能受系统限制，OS 调用和单次 CRC 不能硬中断。
- 按内容识别静态 PNG，检查 chunk/CRC、头部资源上限并拒绝 APNG；JPEG/GIF/WebP 等明确反馈未支持。扫描候选不是完整解码成功的保证，坏像素数据仍可能在流水线失败；逐项错误不阻断其他条目。
- 仅完整扫描且有候选时允许规划。取消或触及全局限制返回可展示的部分结果，**不得自动启动部分批次**；空输入不建立批次。无效参数等整批准入错误保留冻结清单，修正后可重新规划；单文件/目标错误保留全部任务，由start复查并标记该行失败。冻结的是路径与归属，不锁定文件内容，实际处理仍重新校验。
- `Overwrite` 保持覆盖备份契约；`CopyTo` 支持指定目录的扁平或 `PreserveRoots` 布局。副本保留完整原文件名（包含大小写和扩展名、原始OS字符），不增加后缀、不自动编号或覆盖。保留结构时为 `目标/导入根名/相对父目录/原文件名`，单独文件直接放目标根；无名称的文件系统根用 `_root`。不同根同名允许合并布局，仅实际冲突的文件失败。`CopyBeside` 同目录同名会与原图冲突并逐项失败，应另选目录，不能回退成覆盖。识别仍看文件内容，不是扩展名转换功能。
- 选定目标根必须已存在；`OutputPolicy::CopyTree { root, relative }` 只接受普通相对组件，规划不创建目录。只有 output 暂存阶段创建必要子目录，最终仍执行 noclobber；取消/失败/无收益可能保留空结构目录，避免并发任务误删共有目录。P5增加`OutputDirectory`及`CopyToAuthorized`/`CopyTreeAuthorized`，共享身份句柄并在规划、暂存和提交前复查，目录被替换或删除后拒绝写入；这是路径级检查，不是文件系统CAS。旧`CopyTo`/`CopyTree`/`Copy`契约保留，Rust下游完整匹配需处理新变体；桌面DTO随协议v7更新。
- 默认排除输出层保留名：`.pixofold-output-` + 六位ASCII字母数字 + `.tmp`，以及非空原名 + `-backup-` + 六位ASCII字母数字 + `.png`；旧`.pixofold-backup-XXXXXX.png`仍被识别，不自动重命名。Windows对该ASCII形状忽略大小写；非Unicode原名按OS原始名称匹配，不做有损转换。可用`include_artifacts`显式包含。命名匹配不是来源证明，合法用户文件恰好使用保留名也会收到排除反馈；普通隐藏图、`_compressed`及近似但不符合完整形状的名称不排除。目标在输入树内时，必须先扫描结束，再开始写产物。

P2 新增 19 项 Windows 导入回归及 1 项编译型 doctest；包含 Unix 符号链接用例的适用平台测试已随 `11c55fa` 的三平台 CI 通过。

## 应用级任务协调（P3a）

桌面库的 [tasks 模块](src-tauri/src/tasks/mod.rs) 在核心之上统一管理导入、参数修正、启动、取消、重试和清除；[退出桥](src-tauri/src/lifecycle.rs) 将所有者装配到 Tauri 应用。模块本身无窗口/IPC 依赖，可用 [真实文件与确定性门闩测试](src-tauri/src/tasks/tests.rs) 脱离 GUI 验收，不等于前端已经可调用压缩。

- `TaskRuntime` 是唯一线程所有者，持有一个协调线程及核心配置内的固定图片worker池；`TaskControl` 克隆不新建服务，句柄销毁不取消后台任务，窗口重载不改变所有权。桌面启动通过resources按CPU/RAM构造配置，扫描/规划仍由协调线程执行；不因每次确认或重试再创建池。
- 单槽命令接纳覆盖扫描、待修正清单、规划、运行与清除。`import(roots, Some(settings))` 完整扫描后自动规划启动；`None` 等待设置，`start(selection, settings)` 只允许 Ready。整批参数/准入错误保留同一冻结清单；文件目标冲突进入批次逐项失败，不退回Ready，解决占用后重试对应行。取消/超限部分结果不能启动。接纳成功不代表后台处理成功，结果通过快照读取。
- `SelectionId` 防止旧导入请求操作新清单；重试还要求最新批次 revision，防止旧请求重复增加 attempt。核心继续按行校验、保留成功项及备份；清除返回旧快照供恢复信息留存，不删除文件。Rust 快照不是 IPC DTO，也不承诺跨进程恢复/自动续跑。
- 扫描回调提供真实计数；活跃批次按 100ms 间隔采样核心快照，空闲等待通知，不忙轮询。`wait_for_change` 使用单调应用 revision，超时只停止等待；阶段/数量不是编码耗时百分比，轮询采样不是逐事件无损日志。批次全部取消后外层仍为 Finished，具体结果以核心状态和计数为准。
- 取消令牌贯穿扫描、只读规划、批次预检和执行，避免预检期间取消后仍正常入队。外部取消被观察后与核心 revision 一起更新，不暴露相同版本却不同阶段的快照；重复取消不持续唤醒空闲 worker。
- 主窗口关闭或常规退出先停止接纳，只有一个后台收尾任务等待协调/编码线程全部 join，再允许事件循环退出；不得在 UI 线程等待。不可中断的 OS/编码调用可能延长关闭，OS 强杀/断电/重启不提供恢复保证；关闭提示和 UI 状态尚待 P4。
- `TaskSettings::default()` 明确采用产品默认有损 80、原图覆盖；核心 `PngRequest` 默认无损不变。新增 `BatchError::Cancelled` 需要 Rust 下游完整 match 补分支；桌面 `run` 返回可包含任务初始化原因的错误链。P3a阶段未改任务 DTO、TS、权限和现有 `get_app_info` 契约，`compressionAvailable` 仍为 false；后续只读查询见下节。

## 任务只读快照（P3b第一步）

[桌面IPC模块](src-tauri/src/ipc/mod.rs) 与 [前端适配器](src/lib/ipc/tasks.ts) 复用唯一 TaskControl。主窗口可调用 get_task_snapshot 查询状态；该查询本身不启动导入、重试或压缩，也不创建第二个任务服务。P4工作台通过订阅复用此有界查询。

- Rust DTO 与协议常量为权威来源，生成 [tasks.generated.ts](src/lib/ipc/tasks.generated.ts)；原有核心 generated.ts 和 get_app_info 契约保持不变。协议版本当前为 5，前后端须使用同一构建。
- `JobErrorDto` 的 `unsupported_content_credentials` / `unsupported_metadata` 分别表示 caBX 与其他不支持安全改写的元数据；嵌套恢复原因使用同样类别，真正的产物验证错误仍为 `validation`。不能仅凭错误代码授予移除权限：v3的confirmations集合仅包含完整解码/元数据检查通过且唯一不支持类别是caBX的直接失败行，清理失败或其他未知块不入选。
- 应用/批次 revision、selection/batch ID、字节数、elapsedMs 均为规范 u64 十进制字符串，前端用 BigInt 比较；不经过 Number。毫秒向下取整，异常溢出返回 invalid_snapshot；未知大小保留 null。行 ID、候选/问题索引、attempt 与数量使用检查过的整数；行身份由 selection/batch/id/attempt 共同界定，不是文件名。
- 查询指定 jobs、candidates、issues或confirmations，limit 为 1–100。offset=0、expectedRevision=null 读取最新版本；后续页必须带该 revision。一次响应的摘要与行来自同一份 Arc 快照；版本不匹配返回 stale_snapshot/currentRevision，应丢弃旧分页并从第一页重新读取，不拼接不同版本，也不保存无限历史快照。确认集合带安全父目录标签和稳定行ID区分同名图片，批次摘要带全量confirmationCount，不以当前页数量冒充总数。
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
- apply_task_mutation接收可辨识操作联合。import使用授权及固定settings；settings=null只扫描，非null完整扫描后自动启动。start仅用于Ready清单修正。模式复用严格PngMode；协议v7保留overwrite（备份覆盖）/overwrite_without_backup/copy_beside字符串，并增加`{ copy_to: { directoryId, preserveStructure } }`，只接受同会话的原生输出目录授权。桌面默认发送overwrite_without_backup；不接受任意路径或资源预算覆盖。旧前后端握手版本不一致时须更新重载。
- clear/start携带当前selectionId；retry额外要求expectedBatchRevision和最多1000个唯一失败/取消行ID（不是数组/页码索引），沿用Rust原行输出目标，仅修改模式，成功/无收益项不重跑。clear只清非活动记录，不删除原图、结果或备份；UI须先展示需保留的恢复信息。
- 自协议v4引入的confirm_content_credentials携带selectionId、expectedBatchRevision、显式行ID、模式/质量、remove_content_credentials同意和确认专用输出枚举copy_beside/overwrite_with_backup/overwrite_without_backup；v7增加上述copy_to对象。确认按钮即同意，不另设勾选框；拒绝旧v3的overwriteConfirmed字段和含糊的overwrite值。来源路径/属性/SHA256仍由Rust保管并在执行前复查。普通retry沿用原行输出位置与备份策略，但不继承移除许可。成功报告contentCredentialsRemoved明确标记实际移除；无收益保留源文件且标记false。旧版前后端混用须更新并重载。
- 确认UI不分页：通过唯一分页查询器顺序获取每段最多100项的同revision数据，完整加载后一次呈现全部列表（受桌面当前批次1000项上限约束），默认全选且可取消个别选择；不拼接不同revision。关闭、重载或版本变动废弃在途旧数据，分段失败不允许提交部分清单，重试只读加载。
- UI不提供扫描/准备/压缩期间的主动取消入口。内部取消状态/计数和核心取消API保留，用于安全退出与异常收尾，不等于重新开放用户取消功能。
- 首次查询/ACK握手完成后才能操作。后端在订阅锁内原子校验会话并短时接纳，锁序为订阅→授权槽→任务，无文件I/O/await；旧会话、旧授权、旧selection/revision不影响新任务。命令返回selectionId仅表示接纳，后台成功/失败仍经任务快照查询。
- TaskActions固定点击时的参数并限制一个在途操作；默认有损80/覆盖，浏览器明确不可用。选择期间断开/重连的迟到结果不能自动启动；操作传输失败可能已经接纳，不自动重试，应先恢复权威快照。P4工作台复用此适配器，不构造模拟进度。

## 受控原生拖放（协议v5）

- 文件/文件夹拖到左侧图片区域可导入；当前main是单WebviewWindow，Rust从WindowEvent::DragDrop接收原生Enter/Drop/Leave，不监听子WebView专用的WebviewEvent拖放分支，也不双路转发。同一次手势绑定当时已ACK的订阅会话。路径不交给JS，DOM File、展示名或伪造浏览器drop不构成授权；未握手、扫描/准备/运行/待修正或原生对话框占槽时不排队接纳下一批。
- NativeImports复用唯一输入槽：一个对话框、未完成手势或待决票据。Drop检查最多1000根和1 MiB路径编码长度后才复制路径；同会话待决票据未释放前不接受新手势。无效原生输入只发拒绝票据，不保留路径；重复Drop、旧会话和重载后的迟到Drop不能产生授权。
- 唯一订阅Channel的TaskStreamMessage是普通TaskChangeNotice或带kind=native_drop的NativeDropNotice；后者只携带会话、offerId、可选grantId/rootCount和客户区物理坐标。不发送高频Over或另建监听；任务变化的revision ACK/背压不变。前端最多缓存一份握手间隙票据，并按规范u64 ID忽略重复/旧票据。
- 前端按当前devicePixelRatio转换Drop终点，以elementFromPoint命中实际图片区域；隐藏页面、区域外、任何打开的弹窗或操作未完成时不导入，释放票据。接受时冻结当前参数，经原有TaskActions/import扫描、去重和处理；质量非法只扫描，修正后启动。不会按拖入名称自行拼路径，也不添加模拟图片或进度。
- release_native_drop仅由main本地页在已ACK会话下调用，幂等释放匹配票据，不停止任务、不影响下一票据或物理选择框；处理后即使写响应不确定也释放残余授权，绝不自动重发导入。释放失败进入显式恢复；通知发送失败由Rust回收该票据。
- 当前不实现原生悬停高亮动画，仅在Drop终点判断区域；浏览器预览不能处理图片，DOM drop全局阻止文件导航。真实OS拖放、不同DPI/缩放、亮暗/中英及窗口适配须独立原生验收，结果见开发记录。

## PNG 工作台（P4最小纵向闭环）

[Workspace](src/features/workspace/Workspace.tsx)沿HTML原型呈现中央列表、右侧设置与固定底部汇总。[控制器](src/features/workspace/controller.ts)是页面唯一会话所有者；StrictMode、外观切换与组件重挂载不创建重复Channel，不取消后台任务。卸载后异步释放订阅，重挂载等待旧会话清理。

- 选择文件/目录后自动扫描及处理，默认有损80/覆盖。质量仅接受完整0–100整数；空值/非法文本可导入但只扫描，修正后同一清单自动启动一次。Esc恢复最后合法值，无损不携带质量。只持久化合法设置，不保存授权、任务或临时输入文本。
- 点击导入时冻结参数；运行中调整草稿不修改批次。后台规划失败保留清单与错误，同设置不会无限重发；重载恢复的Ready清单需用户主动调整设置才继续。操作返回仅表示接纳，连接或写操作失败保留最后快照、禁止进一步写入，要求显式重连；未知会话归属要求重载页面。
- 启动后只展示进度/结果，不提供主动取消；运行中不能清除或导入下一批。原生文件选择框仍可取消，Ready清单仍可清除，完成后可重试未完成项。取消计数仅非零时显示，不隐藏底层安全收尾产生的真实状态；关闭窗口仍协作停止并等待编码/文件清理，而非强杀或放弃原图保护。
- jobs/candidates/issues每页50项；状态更新时回到首屏，额外分页最多一个在途查询。过期页不拼接，按最新revision恢复。未知大小显示“—”，精确字节按bigint计算；进度为processed/total，取消不冒充100%成功。
- 重试只接纳当前同revision可见页内失败/取消行的稳定ID，使用草稿模式但保留原输出位置；清除需确认，仅清记录，不删除文件。结果展示实际回退原因、输出/备份名及失败恢复信息；展示名不是可访问路径，截断或替换明确标注。
- main页面Started生命周期在同一订阅锁域撤销会话与授权，不清除或重跑任务；尚未关闭的物理对话框仍占槽，晚到结果拒绝后才释放。非main或Finished事件不撤销新会话，不依赖unload必达。
- 浏览器仅预览，不读取图片或模拟业务。桌面拖放接入上述受控入口；未开放缩略图、高级编码参数、新格式和通用fs/dialog/event权限。自选输出目录使用下节的窄命令，原生GUI覆盖范围按开发记录独立验收。

## 指定输出目录（P5，协议v7）

- “另存副本”下可选择输出目录、恢复原文件夹；指定目录时才展示“保留输入目录结构”，默认关闭。取消选择不改变现有设置、不导入或写文件。覆盖模式隐藏这些控件并忽略副本设置，在当前连接切回副本仍保留选择。
- `select_output_directory`仅接收已握手的subscriptionId，使用与输入选择相同的物理对话框互斥。Rust在后台打开既存非链接目录并固定身份；页面只得到directoryId和安全名称，不得到路径。Ready阶段允许修正目录，运行中选择只改草稿；明确的目录不可用响应提示重选，未知响应仍要求恢复。
- 单个输出草稿限当前会话复用，不占单次输入授权槽；输入/输出ID不可混用。替换、精确ID释放、页面重载或断开撤销草稿，旧响应不能撤销新目录。已接纳任务持有独立共享句柄，不随草稿撤销而改目标；句柄在最后一个任务/草稿所有者释放时关闭。
- 持久化仅包含指定目录意图与布局布尔值，不保存路径、句柄或授权ID。重启/重连后必须重新选择，缺少授权时新导入只扫描，修正后同一Ready清单启动，不静默退回覆盖/原文件夹。普通重试仅要求合法模式/质量，继续使用原任务目标，无须重新授权新草稿目录。
- caBX弹窗固定打开时的目录/布局，只给选中行授权移除；使用Rust保存的导入来源和同一个`ImportOutput::outputs_for`规划目标，不把所有普通成功项搬到新目录。指定目录副本同样不显示备份模块，目标存在则拒绝覆盖。
- 默认扁平输出，保留原文件名；保留结构时映射为`目标/导入根名/相对父目录/原文件名`，单独选择的文件放目标根。目标已存在或冲突只使相关图片失败，其他任务继续；普通重试保留原目标、caBX确认同样逐项检查，均不自动编号或覆盖已有副本。无收益不生成副本，失败/无收益可能留下空结构目录。输出位于输入树内时只在完整扫描后写入；历史_compressed文件仍是普通候选，不一律排除。此命名/错误语义更新不改变协议v7形状；显式核心Copy目标路径不被改名。

## 本地诊断日志

- 开发构建写仓库根目录的 `logs/`（已由 Git 排除）；release 构建写系统 `std::env::temp_dir()/pixofold-logs/`。目录不按软件版本或用户名拆分，版本、操作系统及会话标识写入每份文件的头部。
- 每份为 `run-<Unix毫秒时间>-<12位随机标识>.jsonl`；含当前活动文件在内最多 **10份**，每份最多 **5,000,000 bytes（5 MB）**，写入前轮转。只清理符合本应用命名及日志头的已关闭旧文件，不清除其他文件；`.pixofold-log.lock`及每份日志对应的零字节`.lock`为内部协调文件，不计入JSONL数量。活动锁与日志正文分离，运行中仍可读取JSONL。
- 多实例通过目录协调锁与活动文件锁保护写入；全部槽位被活动实例占用时停止新增文件并报告丢失事件，不删除活动日志。安全目录/写入检查失败、磁盘满或队列满不会中断压缩；查看“关于 → 诊断日志”的状态、丢失计数与写入失败计数，可刷新及打开固定目录。
- 核心经 `tracing` 发出受控事件，桌面唯一后台写入器使用1024项有界队列；业务线程不写日志磁盘。字段包含时间、会话、批次/图片ID、尝试号、冻结输出策略、处理阶段、耗时及稳定错误类别。重点事件为等待凭据确认、确认校验/接纳、内存移除、备份创建/保留和覆盖提交，不改变既有业务行为。
- 不记录图片内容、原图文件名、备份文件名、完整私人路径、凭据正文或原始错误。新备份名包含原名，`backup_retained`不再记录`backup_name`，日志白名单也拒绝该字段；实际备份名只经现有脱敏结果DTO在界面展示，不构成任意路径授权。日志只供诊断，不是授权记录、完整事务审计或自动恢复依据；多线程事件可交错，应以 `sessionId + batch_id + job_id + attempt` 关联，不用界面顺序判断线程数。
- 正常退出先等待任务安全收尾，再排空日志并同步磁盘，日志等待上限2秒；强杀、崩溃、磁盘失败可能丢失尾部记录。正式日志属于临时文件，可能被系统清理，复现后应及时保留相关日志。
- 固定目录拒绝符号链接/Windows reparse point及身份替换；Unix新目录0700、文件0600，拒绝不属于当前用户或权限过宽的既有目录，不更改其权限。若多个系统用户实际共享同一个临时根目录，其他用户创建的目录可能导致安全降级，**不承诺跨用户混写**；Windows继承系统临时目录ACL。本机Windows验证与未执行的跨平台验证见开发记录。
- IPC只新增 `get_log_status` 与无参数 `open_log_directory`，限定可信main窗口；不返回路径，也不开放通用opener/shell/fs权限。浏览器预览明确禁用。

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

生产与mock共用一次Tauri上下文宏展开及命令/事件装配入口，避免macOS重复嵌入plist符号及测试路由漂移。拖放回归将WindowEvent交给生产注册的窗口处理函数，再运行授权/真实扫描压缩；mock不模拟OS事件投递。mock IPC从WebView实际URL获取正常来源，分别覆盖配置中的开发地址与平台打包协议；其他窗口、远程/相似域名仍由真实capability拒绝，不为测试开放额外权限。这些回归不启动原生WebView，也不证明原生GUI导航/拖放已经验收。

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
