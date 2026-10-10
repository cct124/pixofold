# GIF无损核心与有界helper

第二阶段提供独立Rust单文件无损与PNG/JPEG/GIF混合核心。桌面v10当前开放PNG/JPEG，GIF随包、协议、预览和GUI在第三阶段验收。

## 开发入口

~~~powershell
pnpm gif:lab:build
pnpm jpeg:lab:build
pnpm gif:core:check
pnpm gif:lab:check
pnpm check
~~~

gif:core:check要求两种固定工具均已构建，缺失/过期即失败。输入只复制到新target/gif-core-目录，包含单文件、三格式混合、2MP单帧和16帧压力图；report.json和metrics.json分别保留行为与采样数据。

## 核心行为

[核心模块](../../crates/pixofold-core/src/gif/mod.rs) 提供GifRequest/GifLimits/GifInfo/GifReport/GifError、GifEngine::load、validate_gif和optimize_gif。绝对工具目录与期望SHA256来自可信宿主构建配置，每次调用复查普通文件、原生身份及SHA256；安装目录可恶意写入时不承诺执行CAS。

请求默认备份覆盖、仅无损。候选实际落盘复验内容/身份，比较双背景画面、精确厘秒、原始循环语义与评论SHA256，提交前复查源和取消。等价重复画面允许合并时长。NoGain保留源且不建立备份；副本noclobber、布局复用导入规划，GIF备份保留原始扩展名和大小写。

初版采用45项语料验证的严格子集。零延时多帧动画、交互帧、未知/保护扩展、非零像素宽高比、保留字段及损坏/超限输入明确拒绝。GIF有损请求返回UnsupportedMode；PNG凭据许可不用于GIF。公开播放摘要仅保留评论哈希，诊断不记录正文、原始stderr、私人路径或请求Debug。

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

Rust下游处理ImageKind/FormatOptions/ImportedImage/ImageReport/ImageError新增Gif变体、BatchParameters.gif与ScanOptions.gif_limits。桌面v10的ImageKindDto仍只有两种格式，转换改为TryFrom；GIF行/报告/能力快照返回InvalidSnapshot，第三阶段完成协议迁移后再开放。

## 许可与证据

Gifsicle按GPL-2.0-only使用。新budget.h/budget.c/entry.c另提供GPL-2.0-only OR GPL-3.0-or-later，独立helper按GPLv2组合；既有Rust主体仍GPL-3.0-or-later。分发helper需提供完整上游对应源码、本封装/配方及COPYING/README等通知；来源链接和本说明不能替代。gif/weezl按MIT使用，原文见 [第三方说明](../../THIRD_PARTY_NOTICES.md)。

Windows本机真实核心/混合/原图安全与采样证据见 [连续记录](../../docs/devlog/_plan/261009/animation-foundation.md)。新SHA三平台CI及GIF随包/安装/GUI分别待验。
