# JPEG 核心与混合批次（J1a / J1b）

本目录的 helper 与 `pixofold_core::jpeg` 提供无损与保守有损核心，现已接入PNG/JPEG混合批次、共用worker和资源预约。桌面v10在可信随包引擎可用时开放JPEG导入、实际报告与受限方向缩略图；实际安装/GUI/平台结果分别见[J2桌面记录](../../docs/devlog/_plan/261009/jpeg-desktop-workflow.md)。

## 构建和真实回归

```text
pnpm jpeg:lab:build
pnpm jpeg:core:check
```

沿用 [固定源码](../../tools/jpeg-lab/engine.json) 的 MozJPEG v4.1.5，不新增 Cargo 依赖。构建脚本校验上游归档、配置/源文件身份，编译静态库和本目录 helper，并记录工具 SHA256；核心加载由受信宿主提供的绝对目录及预期哈希，不搜索 PATH、不执行 shell、不自动下载。哈希来源属于应用信任配置，不得让图片输入或任意 IPC 指定工具/哈希；加载及每次运行复查哈希、普通文件和原生身份，但不是对恶意可写安装目录的执行 CAS 或 OS 沙箱。

核心回归入口为 `tools/jpeg-lab/core-check.mjs`，依次运行 `jpeg_core_check`、`jpeg_lossy_check`、`jpeg_mixed_check`及独立coeffdump/djpeg/标记复验。命令缺工具、源码身份变化或缺语料直接失败，不静默跳过。三平台JPEG job沿用此入口，应用 `pnpm check` 仍无需编译原生JPEG工具。证据写入独立 `target/jpeg-lab-core-*/`；包含自生成边界语料和固定上游testorig.jpg玫瑰照片（227×149），照片SHA256与原始许可/来源随报告保留。小图验证不代表完整摄影观感、ICC校准或性能验收；新代码的平台结果以开发记录为准。

## 混合批次入口与迁移

- 宿主先用 `JpegEngine::load` 验证固定工具，再构造 `ImageEngines::with_jpeg` 和 `BatchService::with_engines`。将 `service.engines()` 传入 `scan_with_engines`，再调用清单的 `plan` 和服务的 `start`；规划保存同一个共享引擎实例，服务拒绝换入另一实例。默认 `scan` / `BatchService::new` 仍只提供PNG，显式JPEG行缺能力时逐行失败。
- 扫描按内容识别，保留原有条目/累计字节/取消上限，不解码像素或启动helper。JPEG须为jpg/jpeg原扩展名（大小写保留）；误命名为.jpg的真实PNG继续按PNG处理。保护元数据/坏结构进入扫描问题，坏像素仍可能在执行时失败。
- 一个固定池处理两种格式及PNG凭据确认；格式请求、报告、质量映射和错误为显式分支，共用状态/统计/输出层。重试保留行ID和未选行，JPEG拒绝PNG凭据移除策略。副本保留原名，目标冲突逐行隔离，全部冲突仍返回Finished批次。
- 核心Rust字面量迁移：`BatchItem`增加 `format`，`BatchRequest`增加 `engines`，`BatchParameters`增加 `jpeg`，`ScanOptions`增加 `jpeg_limits`；默认参数可用结构更新语法。行请求改为 `ImageRequest`，报告/错误/候选属性/质量映射分别由 `ImageReport`、`ImageError`、`ImportedImage`、`QualityMapping`区分格式；共用输入模式为 `CompressionMode`（沿用既有PngMode输入契约）。单文件API不变。
- 桌面TaskRuntime::with_engines将同一实例贯穿扫描/规划/执行/预览，默认构造仍PNG-only。v10快照公开supportedFormats、行format及格式专属processing联合；JPEG工具/保护/恢复错误完整转换，不用PNG量化测量或caBX确认替代。旧v9页面须更新重载。

## 模式、质量与兼容性

- `JpegRequest::new` 保持核心默认无损/备份覆盖。使用 `JpegMode::Lossy { quality: QualityValue::default() }` 显式选择有损80；调用方仍负责输出策略与总并发准入。
- 映射版本1将质量0转为原生1，其余1–100保持原值。固定 FASTEST 配置、整数 ISLOW DCT 与优化 Huffman，不启用 trellis/SIMD；保持原颜色空间、分量ID、采样和基线/渐进类别。YCbCr逐行解码和重采样仍会改变像素，100仍属有损，不承诺跨格式相同质量数值或固定体积收益。
- `JpegReport.processing` 区分 Lossless、Lossy 与带明确原因的 LosslessFallback，并保留输入质量/映射版本/原生质量。它描述候选的处理方式；只有 outcome=Optimized 才提交，NoGain保留原图且不产生备份/副本。
- ICC、CMYK/YCCK、JFIF嵌入缩略图或不明确的三分量颜色标记进入现有系数无损验证路径。复杂Exif/XMP/未知APP/APP11仍拒绝，不能以无损回退绕开保护。压缩保留方向tag、不旋转文件像素；桌面仅在缩略图中应用方向。
- 未发布的核心请求结构新增 mode，报告新增 processing；结构字面量调用方需补 mode，使用 new 的调用方仍默认无损。桌面v10共用模式沿用PngMode的无损/有损输入形状，格式专属质量映射通过processing分别表达。

## 格式和元数据边界

- 无损支持真实8-bit Huffman基线/渐进JPEG，1/3/4分量；只改熵编码，不量化、旋转或转换颜色。普通有损支持灰度/RGB/YCbCr，其他颜色按上述规则回退；两种路径保持基线/渐进类别，可调整渐进扫描组织。
- 同时比较尺寸、分量 ID/采样、颜色解释、各分量实际量化表、有符号 DCT 系数，以及 APP/COM 的原字节和顺序。系数流 SHA256 在父进程增量计算，固定头/精确长度亦须符合 Rust 预检；不是把编码文件更小或像素看起来相似当作无损证明。
- 有损先完整解码源，再生成候选；候选落盘回读须与刚生成的内存字节相同，并再次完整解码，检查PFJP1头/精确像素长度、尺寸、分量/采样、实际颜色解释及元数据原字节/顺序。父进程流式校验像素，不持有全图像素副本；不以源候选像素相等作为有损条件。
- 接受标准 JFIF、Adobe、完整且无重复序号的单段/分段 ICC、COM，以及只有单 IFD/单 Orientation 项的 Exif（大小端均检查）。不解释/重写 ICC，Exif 方向原样保留、不旋转像素；首段 ICC 须容纳完整 128-byte 头。
- APP11/JUMBF 一律保护；XMP、MakerNote、Exif 缩略图/额外标签、未知 APP、扫描后元数据和扫描中重定义的量化表暂拒绝。保留字节不代表签名在压缩后仍有效，未实现 JPEG 凭据验证/移除确认，不能复用 PNG 的 caBX 授权。合成 APP11 只验证拒绝分支，不是真实签名验收。
- 拒绝 12-bit、算术编码、损坏/截断、尾随拼接内容、多帧/其他 SOF；原生任何警告都视为失败。结构检查只是预检，原生系数解码及候选回读验证均不可省略。

## 资源、生命周期和文件安全

- 默认输入/候选各最多 64 MiB、16 Mi 像素、单边 16384、系数预算 128 MiB；绝对配置上限为输入 64 MiB、16 Mi 像素、单边 65535、工作集 512 MiB。最多 64 次扫描、4096 个标记和 1 MiB 元数据（含每标记计费）；系数按 MCU 对齐预检并计入源/编码工作集及元数据副本。
- native的source/destination各设置一半工作集参数，系数分配前再次检查实际头和对齐预算；关闭backing store的上游构建超预算时失败，不回退磁盘。批次按执行上限预约8份输入/候选缓冲、2份原生工作集及32MiB固定余量，覆盖同时存活的source/candidate/stored、源/备份复查、管道增长、helper和I/O线程开销。最多2MiB标记前缀只用于收紧尺寸和预算；坏头保留原上限，执行时真实复查。限额不是RSS硬配额。
- 有损/像素验证额外保守计入两份全尺寸像素工作区；Rust预检及helper实际头都复查此上限。native从总工作集扣除该余量，再将剩余预算分给解码/编码虚拟数组；仍不代表对所有库分配的RSS硬限制。
- 单图片顺序调用三次原生工具：无损为源系数/优化/候选系数，有损为源像素/重编码/候选像素；默认每次30秒，可设1ms–120s。单次一个单线程原生进程，父进程限制输入、stdout字节/精确系数或像素长度和16KiB stderr；stderr只计数、不展示/记录。三个I/O线程只搬运管道，不是嵌套编码线程池。
- 每次创建独占临时工作目录，清空继承环境，仅保留 Windows 系统目录和固定临时目录/locale。工具只收发字节，没有用户路径或最终文件写权限接口；可信 helper 不派生子进程。取消、超时、管道错误先终结并 wait 自有进程，再 join I/O、清理目录；清理失败有结构化恢复上下文。该超时不包含整个文件 I/O/工具身份哈希，也不是 OS 级实时保证。
- 复用现有输出层：候选落盘回读验证、源身份/内容复查、目标 noclobber、可选完整备份及单次替换；未验证/无收益不提交、不留备份。清理失败/替换失败保留恢复信息，不先删除原图，也不承诺断电事务或文件系统 CAS。
- JPEG 入口要求源扩展名为 jpg/jpeg（不区分大小写），备份使用精确原始 OS stem + `-backup-` + 6 位 ASCII 随机标识 + 原扩展名/大小写。PNG 入口继续固定 `.png` 备份，即使源文件扩展名误写为 jpg 也不受影响。新 JPEG 保留名纳入扫描排除，副本策略仍不覆盖已有目标。

## 待验收与许可

`decode_preview`借用同一已校验引擎执行现有pixels操作，返回无元数据的灰度/RGB像素和已验证方向；不修改helper配方。宿主将预览限制在16 MiB输入、8 Mi像素、32 MiB工作集与5秒单次进程期限，单解码在途，最终仅发送128×96以内的小PNG。ICC、四分量/不明确颜色和保护元数据拒绝预览；Exif仅支持上述单Orientation结构，1–8方向由有界采样应用。图片任务预约和预览预算独立，均不是RSS硬限制。取消/失效复用进程回收，真实返回前不释放许可。

`pnpm jpeg:bundle:check`在仓库外的受限开发环境复用生产加载器和桌面后端，验收混合扫描/任务/报告/冻结重试/NoGain/冲突/备份/目录与11张实际缩略图，再用djpeg/coeffdump独立核对JPEG输出。它不启动GUI、不分发到默认安装包，也不代替真实选择/拖放/亮暗中英/正常退出及安装验收。

平台执行结果分别见 [J1a记录](../../docs/devlog/_fin/261009/jpeg-lossless-core.md)、[J1b有损记录](../../docs/devlog/_fin/261009/jpeg-lossy-core.md)与[J2随包记录](../../docs/devlog/_plan/261009/jpeg-desktop-bundle.md)；新代码不能沿用旧SHA的三平台结果。helper使用相同静态MozJPEG，版权/许可来源见[第三方说明](../../THIRD_PARTY_NOTICES.md)。J2生成资源包含许可正文/IJG致谢/来源，正式发行仍需完整对应源码资料、安装运行及签名后身份验证。
