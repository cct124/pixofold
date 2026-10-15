# APNG无损核心与桌面（A3/A4）

Rust入口为 `pixofold_core::apng`。核心支持受限APNG独立验证、固定O1无损压缩和安全输出；A4接入扫描、批量、协议v13、受限预览和四格式桌面。默认核心服务仍PNG-only，桌面通过 `ImageEngines::add_apng` 显式启用内建能力；静态PNG入口仍拒绝动画。APNG不依赖外部helper，JPEG/GIF独立降级不影响其能力。

## 接口与使用

开发特性apng-lossy-lab另提供[共享调色板有损实验](../../../../tools/apng-lossy-lab/README.md)：只返回内存候选及独立质量报告，没有生产输出、批量或v13协议接入；以下生产接口继续严格无损。

```rust
use pixofold_core::{
    apng::{ApngRequest, optimize_apng},
    model::{CancellationToken, OutputPolicy},
};

let mut request = ApngRequest::new("animation.apng");
request.output = OutputPolicy::Copy { destination: "smaller.apng".into() };
let result = optimize_apng(&request, &CancellationToken::default(), |_| {});
```

- `ApngRequest`只包含源路径、输出策略和额度，没有有损模式或凭据移除许可；默认覆盖并保留备份。无需外部工具或平台helper，沿用锁定的oxipng 10.2.1（关闭默认特性/并行）、png 0.18.1、fdeflate 0.3.7。fdeflate从开发引用提升为同版核心依赖，Cargo.lock没有版本变更。
- `validate_apng`接收字节、`ApngValidationLimits`与取消令牌，返回默认图/子帧/控制/合成摘要；`compare_apng`比较两份字节，源/候选共用累计工作额度。摘要不授予输出、凭据或IPC权限，不写入诊断日志。
- `ApngReport`包含输入属性（帧数、原始num_plays、默认图是否参与）、实际Lossless、真实字节数、耗时和`ProcessingOutcome`。NoGain没有最终副本或新备份，输出字节数等于输入字节数。
- `ApngError`区分无效额度、资源超限、结构/解码拒绝、编码失败、候选验证失败、取消、超时、共享文件错误与清理错误。原始I/O和编码错误保留为source，不把含私人路径的Debug放入日志。提交失败保留可恢复备份；清理失败保留原错误和残留临时路径。

## 支持与严格无损

沿用[实验支持矩阵](../../../../tools/apng-lab/README.md)：8/16位灰度/RGB/灰度透明/RGBA、8位索引透明、Adam7；默认静态图参与/不参与、SOURCE/OVER、三种dispose、首帧PREVIOUS、精确时间分数及每轮透明画布。保留原始子帧、隐藏RGB、调色板、位深、交错、元数据内容/次序/动画锚点和num_plays。1/2/4位、ICC、Exif、caBX及未知/未列入支持矩阵的元数据明确拒绝。扩展名不作为内容证据，误命名输入不会绕过验证。

O1禁用alpha优化、位深/色型/灰度/调色板缩减、16位缩放、交错转换、错误修复和元数据剥离。读取源后完整验证，编码后先校验候选字节上限，再暂存、同步并重新读取实际文件；同时核对候选SHA256和独立语义，源文件复查后才判断收益。候选结构/元数据变化即失败，不把它当作NoGain或退化为静态PNG。

实验模块现为核心验证器的薄适配层，人工生成的83项语料/11组比较保持独立；没有复制另一份验证实现。

## 输出与恢复

- 共用唯一output层：独占临时文件、noclobber副本、指定目录结构/原生身份复查、临界区前取消/源内容和身份复查、恢复备份、原子替换；不宣称文件系统CAS或断电事务。
- APNG备份为原stem加 `-backup-`、6位ASCII字母数字，再加原始 `.png/.apng` 扩展名（保持大小写）；其他误扩展名用 `.png`。原始OS名称不截断、不有损替换；路径无法创建时失败。保留名扫描新增APNG及PNG大小写识别，普通静态PNG原有备份规则不变。
- 无收益仍完成落盘验证和源复查，随后清理临时文件。错误/取消同样关闭并清理暂存；Windows清理失败返回具体残留路径与原始原因，提交失败后的备份供调用方恢复。

## 资源与取消

验证额度默认：输入/候选各16MiB、画布/帧各4Mi像素、256帧、4096块、1MiB元数据、128MiB验证缓冲估算、64Mi累计像素及512MiB累计解码工作。源和候选共享累计额度，包含独立默认图、严格zlib/双解码及第二轮播放。核心在编码前确认源已使用的累计额度能容纳等结构候选；实际候选仍按剩余额度执行。

`ApngLimits`默认30秒总准备期限、512MiB工作集估算上限，均只可收紧。验证单阶段至多10秒，编码软期限取剩余总期限与10秒的较小值。所有额度共同生效；4Mi像素不是任何深度/输入大小都能处理的承诺。

工作集估算取以下较大值：

- 编码：实际画布像素×96 bytes + 最大输入/候选限额×8 + 元数据限额×4 + 帧限额×16384 bytes + 32MiB原生余量。
- 验证/输出：验证缓冲限额 + 上述压缩缓冲、帧摘要与32MiB余量。

这是保守准入估算，覆盖源/候选、复读/备份、过滤缓冲和原生分配余量；不是分配器或RSS硬限制。先按最小估算拒绝，再经结构解析按实际画布复查，之后才解码/编码。候选落盘后释放编码向量，源与读回候选的存活量继续计费。批量与PNG/JPEG/GIF共用固定worker池/全局预算；有界IHDR探测只收紧输入、像素、最大边和验证缓冲。预约与实际执行使用同一冻结额度，排队后变大的源必须拒绝，超预算只使该行失败。

## v13迁移与预览

- Rust新增 `ImageKind/FormatOptions/ImportedImage/ImageReport/ImageError::Apng`、`BatchParameters.apng`、`ScanOptions.apng_limits`。扫描按PNG块边界识别动画声明并完整结构预检，不解码；`.png`、`.apng`和误扩展名均不能覆盖实际内容。
- v13在 `supportedFormats` 中提供apng，`processing.format=apng`、`details.kind=lossless`，含 `frames`、`numPlays`、`defaultParticipates`。numPlays是总播放次数，0为无限，不能套用GIF额外循环次数。全局有损请求的mode保留，但APNG实际始终无损；没有质量映射、回退或PNG凭据移除权限。旧v12页面须更新重载。
- 低位深为 `unsupported_apng`，未支持或保护元数据为 `unsupported_apng_metadata`，损坏/资源/输出失败使用对应稳定错误。扫描诊断按这些受控类别聚合，无路径、元数据或逐项文件名。
- 首帧预览只解码第一动画子帧并在透明黑画布合成；独立默认图不参与，后续帧像素不解码。它不是整幅动画有效性证明；压缩仍完整验证。16MiB输入、4Mi画布/帧像素、单边16384、32MiB缓冲估算、5秒协作期限共同生效（大图可能先受缓冲限制），输出最多128×96/64KiB PNG。沿用应用单解码许可、撤销/迟到丢弃、缓存和结果身份契约。

oxipng同步调用不可抢占。取消/期限在各阶段及验证检查点响应；编码中收到取消也必须等待编码返回、丢弃结果再返回。核心不创建脱离任务的编码线程。进入共享提交层后按其临界区契约完成，不因提交后的取消/超时改报失败。需要进程级强制内存/时间隔离时另做受限子进程。

## 可复现验收

```powershell
pnpm apng:fixtures:check
cargo test -p pixofold-core --locked --test apng_lab
cargo test -p pixofold-core --locked --test apng_core
cargo test -p pixofold-core --locked --test apng_batch --test apng_preview
pnpm apng:core:check
pnpm apng:bundle:check
pnpm apng:lab:check
pnpm check
```

核心命令覆盖38个有效生成样本、6公开原件与压力图，共45项；全部83项（含45拒绝/额度项）另经核心集成测试。报告、源码/二进制/Cargo.lock哈希、来源/许可及隔离输入/输出位于 `target/apng-core-*/`。提交/备份/源变化/候选篡改/冲突/权限/同步取消/期限由集成和确定性注入测试验证；实验继续保留135组对照。`apng:bundle:check`复用 `gif:bundle:check` 同一次四格式验收，包含无损/有损80混批、实际无损报告、稳定ID重试、NoGain、冲突、备份及预览撤销，证据为 `target/gif-bundle-*/` 的apng报告和快照。Windows本机与新SHA三平台、GUI/实际安装证据见[动画连续记录](../../../../docs/devlog/_plan/261009/animation-foundation.md)，各自独立验收。
