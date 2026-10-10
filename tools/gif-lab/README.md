# GIF语料、独立播放验证与Gifsicle无损实验

第一阶段实验及第二阶段核心验收入口。GIF生产验证已迁入Rust核心，固定工具只读取隔离样本并写入新的目录；桌面工作台仍开放PNG/JPEG。

## 运行

使用仓库锁定的Node、pnpm与Rust工具链：

```powershell
pnpm gif:fixtures:generate
pnpm gif:fixtures:check
cargo test -p pixofold-core --locked --test gif_lab
pnpm gif:lab:build
pnpm gif:lab:check
pnpm jpeg:lab:build
pnpm gif:core:check
```

`build` 首次下载固定源码归档，此后核对归档和源码树。Windows需要MSVC、Windows SDK、tar与CMake 3.15+；CMake在PATH中不可用时查询Visual Studio，也可使用 `pnpm gif:lab:build --cmake ABS_PATH`。Windows按上游win32cfg及完整源文件列表构建Release静态CRT（/MT），包含1.96新增的kcolor.c。macOS/Linux配方使用上游configure、make及C编译器，关闭gifview、gifdiff、threads和SIMD；本机尚无这些平台运行证据。

`check` 先核对工具身份和确定性语料，再构建独立Rust验证器。每次复制45项固定语料并生成一项压力图，分别执行default/careful × O1/O2/O3，共168次优化。全部显式使用 `--same-comments --same-extensions`，只写不存在的候选文件，逐个比较播放语义并复查输入SHA256。

## 固定身份与许可

- [engine.json](engine.json) 固定Gifsicle 1.96官方tar.gz及SHA256 `fd23d279681a6dfe3c15264e33f344045b3ba473da4d19f49e67a50994b077fb`。缓存位于 `.tools/gif-lab/`，`identity.json` 记录原生目标、源码树/配方及工具SHA256；修改配方后必须重新build。
- Gifsicle按GPL-2.0-only作为独立进程使用，实验复制上游完整COPYING/README，保留原始源码版权。若分发工具，按所选GPLv2方式提供全部对应源码和本项目构建配方，不能把报告里的来源链接当作完整源码交付。
- 独立验证器使用gif =0.14.2与weezl =0.1.12，第二阶段已转为同版本生产依赖，按MIT使用；原文在 [gif通知](licenses/gif-LICENSE-MIT) 与 [weezl通知](licenses/weezl-LICENSE-MIT)，同时复制到实验。原始源码包含MIT/Apache双许可正文，详见 [第三方说明](../../THIRD_PARTY_NOTICES.md)。

## 播放与拒绝边界

[语料说明](../../tests/fixtures/README.md) 和 [manifest](../../tests/fixtures/gif/manifest.json) 包含45项输入、SHA256、人工给定的画布/时间轴，以及8组等价正例和改变延时/循环/偏移/像素/注释的负例。循环接缝另固定第二轮人工预期，防止错误重置画布。

Rust验证器先做完整结构预检，再使用gif库读取帧及weezl全帧有界解码，检查EOI、解码像素数量与调色板索引，手动重排交错行、执行透明覆盖和dispose。跨行LZW字典码有独立回归，避免将逐行解码接口的NoProgress误报为缺结束码。

比较画布尺寸、合成RGBA画面摘要、精确整数厘秒持续时间、原始循环扩展语义和注释。连续相同画面可以合并时长；分别核对透明和逻辑背景，两种解释都必须保持。存在循环扩展时再保留画布运行第二轮，核对循环接缝。无循环扩展、有限循环值与无限循环值区分；没有把GIF循环字段当作APNG num_plays。

本阶段拒绝零延时多帧动画、用户交互、未知应用/文本扩展、非零像素宽高比、不受支持保留位及所有损坏/超限输入。此处采用保守子集，浏览器延时钳制、人工观感和完整产品格式支持另行验收。

## 实验限额

| 范围         | 默认最大值                           |
| ------------ | ------------------------------------ |
| 输入         | 16MiB                                |
| 画布 / 单帧  | 各4,194,304像素                      |
| 帧数         | 256                                  |
| 累计解码     | 64MiB，按RGBA及背景/循环代表轮次计费 |
| 元数据       | 64KiB                                |
| 结构子块     | 65,536                               |
| 独立验证     | 10秒软期限，帧间检查                 |
| 单次Gifsicle | 10秒期限，返回前回收子进程           |

本表描述首轮实验CLI。验证器只保留有界画布、单帧和摘要，直属Gifsicle工作集采样为观测数据。第二阶段已经接入生产worker/预算/取消和有界原生分配，完整执行限额及内存范围见 [GIF核心说明](../../native/gif/README.md)。

## 本机结果与复验

2026-10-10，x86_64-pc-windows-msvc首轮168项真实优化已完成，输入SHA256全部保持：

| 配方                   | 有收益 | NoGain | 验证拒绝 |
| ---------------------- | ------ | ------ | -------- |
| default O1             | 26     | 2      | 0        |
| default O2/O3（各）    | 24     | 2      | 2        |
| careful O1/O2/O3（各） | 26     | 2      | 0        |

default O2/O3在larger-pattern.gif与stress.gif中产生透明索引超出调色板的候选，被严格验证器以InvalidGif拒绝。实验仅允许这两个已解释边界，其余结构/播放改变立即失败；careful O2必须通过全组语料，否则实验失败。候选不得因工具退出成功或体积缩小而跳过独立验证。

核心采用上述careful O2配方并强制独立复验；O1也通过本语料，O3暂无额外收益依据。gif:core:check另验证真实单文件、三格式混合、备份/取消/原图安全、2MP/16帧压力与采样，要求固定JPEG工具已构建。产物在target/gif-core-目录，包含完整许可和本项目native封装源码。GIF可信随包与GUI/安装仍在后续阶段验收。

每次报告位于 `target/gif-lab-*/report.json`，包含逐项输入/输出哈希、字节数、语义结论、耗时、采样工作集、工具与验证器身份；同目录保留输入/候选/完整Gifsicle许可和两份MIT通知。三平台GIF实验job已配置，尚未推送运行。具体证据和剩余项见 [连续记录](../../docs/devlog/_plan/261009/animation-foundation.md)。
