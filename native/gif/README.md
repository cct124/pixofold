# GIF单文件核心与有界helper

第二阶段提供独立Rust单文件无损与PNG/JPEG/GIF混合核心。第三阶段接通可信随包与桌面v11工作流：GIF仅无损，首帧静态预览、报告及普通重试沿用任务所有权；GUI、安装和新SHA平台按连续记录分别验收。

4B第一段增加单文件保守有损核心。批量ImageRequest/桌面v11继续拒绝GIF有损；v11转换还会拒绝意外传入的有损核心报告，不能把它标成无损。批量与v12迁移在下一阶段。

## 开发入口

~~~powershell
pnpm gif:lab:build
pnpm jpeg:lab:build
pnpm gif:core:check
pnpm gif:lab:check
pnpm jpeg:bundle:prepare
pnpm gif:bundle:prepare
pnpm gif:bundle:check
pnpm check
~~~

gif:core:check要求两种固定工具均已构建，缺失/过期即失败。输入只复制到新target/gif-core-目录，包含单文件、三格式混合、2MP单帧和16帧压力图；report.json和metrics.json分别保留行为与采样数据。

## 核心行为

[核心模块](../../crates/pixofold-core/src/gif/mod.rs) 提供GifRequest/GifLimits/GifInfo/GifReport/GifError、GifEngine::load、validate_gif和optimize_gif。绝对工具目录与期望SHA256来自可信宿主构建配置，每次调用复查普通文件、原生身份及SHA256；安装目录可恶意写入时不承诺执行CAS。

请求默认备份覆盖和无损，可显式设置GifMode::Lossy。无损候选比较双背景画面、精确厘秒、原始循环语义与评论SHA256；有损另外使用下述质量/收益规则。选定候选实际落盘复验内容/身份，提交前复查源和取消。NoGain保留源且不建立备份；副本noclobber、布局复用导入规划，GIF备份保留原始扩展名和大小写。

初版采用45项语料验证的严格子集。零延时多帧动画、交互帧、未知/保护扩展、非零像素宽高比、保留字段及损坏/超限输入明确拒绝。批量GIF有损请求返回UnsupportedMode；PNG凭据许可不用于GIF。公开播放摘要仅保留评论哈希，诊断不记录正文、原始stderr、私人路径或请求Debug。

## 单文件有损v1与API迁移

- GifRequest新增mode（new默认Lossless），GifLimits新增preparation_timeout（默认90秒，合法1ms–300秒），GifReport新增requested_mode/processing/quality；结构体字面量调用方须同步。quality保存尝试的有损候选观测，回退后不代表最终文件的误差，NoGain也不代表提交了候选。
- GifQualityMapping v1为min(20, ceil((100-q)/2))：q100只走严格无损，q80原生10，q60及更低值原生20；全0–100可用，低端保守封顶。最高质量的语义仅适用于本GIF映射，不改变PNG/JPEG。
- 顺序执行/验证无损候选与有损候选。语义硬约束保持画布、循环、时间轴、透明和评论；双背景RMSE≤2.5、最差区间≤4、最大通道误差≤64、误差变化RMS≤3、最差误差跳变RMS≤6。RGB编码值门槛是保守工程策略，不等同感知质量保证。
- 时域RMS只在误差画面实际变化的边界计分母，相同误差拆帧不能稀释分数；最差跳变包含循环接缝。原图运动不直接作为误差。透明三像素回归证明末帧Background变Keep会使第二轮残留，保留拒绝，不放宽原有无损判定。
- 有损需比min(原图,有效无损候选)至少额外节省1%且不少于1024 bytes。质量不足/收益不足分别返回LosslessFallback原因，无损也无收益则NoGain；原生有损候选若实际完全等价，按Lossless报告并选更小的无损候选，不要求有损收益门槛。结构、透明、时间轴、工具错误与取消/超时不转换为成功回退。
- 比较器迁至常规核心comparison，公开compare_gif用于独立复验。缓冲模型为像素×24+(源+候选字节)×4+2MiB，另计仍存活的无损候选或待核对候选容量；总额受max_decoded_bytes限制，工作量仍限256Mi画布像素。单文件working_set的八份输入容量、验证缓冲、原生配额与系统余量保守覆盖各阶段；现有批量无损收紧公式须在后续有损批量接入时同步修订。
- preparation_timeout覆盖源读取及候选准备/验证的总协作期限，各次helper/验证还分别受30/10秒与剩余期限较小者限制。文件I/O不是可抢占硬期限；进入共享输出事务后按原取消/不可逆提交契约安全完成，不在文件已提交后改报超时。取消/超时先回收真实子进程、管道和临时目录。
- gif:core:check包含真实q0/60/80/99/100、实际无损、暗部噪声质量回退、录屏收益回退/NoGain、透明拒绝、备份、取消、候选/源/目标竞争及验证资源限制；既有无损和三格式混合继续验证。人工观感、桌面有损及新SHA平台证据分别验收。

## 默认资源与回收

| 范围 | 额度 |
| --- | --- |
| 输入/候选 | 16MiB |
| 画布/单帧 | 各4Mi像素 |
| 帧数 | 256 |
| 累计RGBA解码工作量 | 64MiB，双背景与循环代表轮次计费 |
| 元数据 | 64KiB |
| 验证缓冲 | 64MiB，双画布、索引、压缩Vec容量与1MiB余量 |
| 原生请求分配 | 32MiB，包含分配头和realloc新旧同时存活 |
| 单次编码/单次验证 | 30秒 / 10秒协作期限 |

原生分配可配置1 byte至256MiB；资源字段分别限制，所有最大值的组合仍可能超过工作集额度。默认预约256MiB：8份最大输入容量+验证缓冲+原生分配+32MiB系统/线程余量。头只收紧画布、单边和验证缓冲，实际pipeline使用同一额度；原生和累计解码额度独立冻结。预约及观测不称RSS硬限额。

独立helper使用固定Gifsicle 1.96及careful O2。构建时把所有上游malloc/calloc/realloc/free重定向到 [分配封装](budget.c)，分配前检查活跃字节，超限固定86退出。源码缓存不改，关闭线程/SIMD；MSVC 64位采用16字节对齐头，其他平台采用max_align_t，自检覆盖对齐、calloc、realloc数据保持、释放归零与超限。

Rust拥有child、三个有界管道线程和工作目录；取消/超时先kill/wait并join，再清理目录，回收后才释放worker/预算。BrokenPipe等待真实退出码，stdout/stderr超限主动终结；清理失败保留恢复上下文。开发采样仅观察example及直属固定helper，10ms为目标间隔，未采到helper记null。

## 混合能力与迁移

ImageEngines::with_gif或with_jpeg(...).add_gif(...)显式注入；scan_with_engines、规划和BatchService共用同一实例，默认仍PNG-only。GIF接入唯一有界池，重试保留稳定行ID、原目标与备份策略；坏像素、超限和冲突逐行处理。

Rust下游处理ImageKind/FormatOptions/ImportedImage/ImageReport/ImageError新增Gif变体、BatchParameters.gif与ScanOptions.gif_limits。桌面v11的ImageKindDto、supportedFormats及格式专属processing包含Gif，无损报告提供frames与loopCount，后者保留原始扩展值（null无扩展、0无限循环）。旧页面须重载；Rust生成类型与TypeScript运行时验证同步。

有损模式中的GIF逐行返回unsupported_gif_mode，其他格式继续；设置栏明确提示仅无损，切换无损后普通重试保留稳定ID、原输出与备份策略，不继承PNG凭据许可。原生文件过滤器和格式标志取自启动时验证通过的能力，JPEG/GIF独立降级，默认服务仍PNG-only。

## 可信资源与首帧预览

gif:bundle:prepare共用固定开发配方，在src-tauri/resources/gif/runtime准备helper、完整上游源码归档、封装/配方及许可原文。build.rs验证目标/源码/产物并内嵌预期SHA256，运行时只从宿主资源根加载，不信旁置清单。Windows使用静态CRT；release缺资源或配方过期即失败，普通无资源debug可运行PNG。

gif:bundle:check将资源复制至独立中文空格目录，在无开发PATH的进程中执行真实处理、独立播放验证、三格式混合/模式失败/稳定ID重试/NoGain/冲突/备份/首帧预览/退出，并验证单引擎失效与伪造清单拒绝。支持--resources后接已解包的绝对资源根目录，验收二进制不随默认安装包分发。

decode_preview只在Rust合成首幅完整透明画布，不执行helper或解码后续帧；先检查整个输入的结构与支持范围，成功不保证后续帧像素可解码。应用限制16MiB输入、4Mi画布/单帧像素、256帧结构、32MiB预览缓冲与5秒协作期限；输出至多128×96/64KiB静态PNG。预览沿用AssetService的单解码、任务绑定、缓存、取消和迟到失效，不改变真实压缩结果或播放时间轴。

## GIF有损4A开发实验

4A开发实验通过显式Cargo特性`gif-lossy-lab`访问`gif::lab`。其内存候选复用可信helper及子进程所有权，比较器在双背景/代表循环按时间区间对齐，保留画布/循环/评论/透明覆盖与原图画面切换时间点；仅允许原图连续相同画面合并。RGB误差、区间最差误差和相邻误差差分属于实验观测，不冒充感知评分或生产许可。单比较共用绝对期限，输入仍受生产严格子集限制，额外同时存活缓冲受max_decoded_bytes及256Mi全画布比较像素上限约束。

默认特性及桌面依赖不启用未经验证的实验候选接口；单文件通过optimize_gif开放有损，v11与普通重试保持仅无损，现有Playback::equivalent没有放宽。运行方式、原生参数与观感报告见[实验说明](../../tools/gif-lab/README.md)。

## 许可与证据

Gifsicle按GPL-2.0-only使用。新budget.h/budget.c/entry.c另提供GPL-2.0-only OR GPL-3.0-or-later，独立helper按GPLv2组合；既有Rust主体仍GPL-3.0-or-later。分发helper需提供完整上游对应源码、本封装/配方及COPYING/README等通知；来源链接和本说明不能替代。gif/weezl按MIT使用，原文见 [第三方说明](../../THIRD_PARTY_NOTICES.md)。

Windows本机真实核心/混合/原图安全与采样证据见 [连续记录](../../docs/devlog/_plan/261009/animation-foundation.md)。2026-10-12复核确认6209a23的三平台CI run38038904722全部9个jobs通过，包含GIF固定构建/独立实验/真实核心与混合回归；此结果不覆盖本轮第三阶段。GIF随包/桌面检查、安装和GUI分别记录，新SHA三平台须在本轮提交后验收。
