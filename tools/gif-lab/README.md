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

核心采用上述careful O2配方并强制独立复验；O1也通过本语料，O3暂无额外收益依据。gif:core:check另验证真实单文件、三格式混合、备份/取消/原图安全、2MP/16帧压力与采样，要求固定JPEG工具已构建。产物在target/gif-core-目录，包含完整许可和本项目native封装源码。GIF可信随包使用独立gif:bundle:check，GUI/安装证据按平台与版本在连续记录中分别维护。

每次报告位于 `target/gif-lab-*/report.json`，包含逐项输入/输出哈希、字节数、语义结论、耗时、采样工作集、工具与验证器身份；同目录保留输入/候选/完整Gifsicle许可和两份MIT通知。既有无损三平台结果及本轮新代码证据见 [连续记录](../../docs/devlog/_plan/261009/animation-foundation.md)，不能混用不同SHA。

## 4A：有损最小实验

```sh
pnpm gif:lab:build
pnpm gif:lossy:check
```

后者不联网下载素材，编译显式`gif-lossy-lab`特性的`gif_lossy_lab` example；默认核心/桌面特性不包含实验接口。固定1.96有界helper、careful O2、保留评论与扩展，比较lossless/原生lossiness 0/10/20/40/80。强度越大允许的像素变化越大，不等于UI的0–100质量；没有启用额外调色板缩减或抖动。

### 检查与资源边界

- 输入仍须通过生产结构/元数据/完整LZW验证；未知或保护应用扩展、零延时多帧、超限输入分别记录为不进入实验，不删除这些保护边界。45项确定性语料及人工正负比较先完整验收。
- 有损比较保持画布、循环原始值、评论及完整合成透明覆盖。原图每个实际画面转换的精确厘秒时间点都必须保留；候选允许拆帧，原图连续相同画面可合并；两个代表循环覆盖循环接缝。改变总时长、同总时长但移动转换时间、循环/元数据/透明的负例均应拒绝。
- 新流式合成器逐背景逐帧处理，复用可取消weezl解码，与人工预期语料的所有画布摘要交叉核对。生产Playback::equivalent不改变；有损比较不会将像素变化误报为无损。
- 同时存活缓冲保守上限为`画布像素×24 + (输入字节+候选字节)×4 + 2MiB`，受默认64MiB验证缓冲限制；额外全画布比较总量限256Mi像素。结构/合成/比较共用绝对10秒期限，片段检查取消；原生分配32MiB、子进程30秒，实际回收复用生产process路径。
- 质量观测：RGB编码值RMSE（按画面持续时间加权）、区间最差RMSE、最大通道误差、变化像素比例；v2时域误差只在误差画面实际变化时计RMS分母，等价拆帧不稀释；另记录最差误差跳变（含循环接缝）。原图正常运动本身不计作误差；这些量不是SSIM/感知评分，也不能代替观看动画。
- v2筛选为双背景RMSE≤2.5、区间最差≤4、最大通道误差≤64、时域误差≤3、最差跳变≤6。硬约束拒绝优先；体积不小于输入为NoGain，完全等价则单列无损收益。单文件核心v1使用同一质量上限并增加相对无损基准至少1%且1KiB的收益门槛；实验的quality-candidate仅表示通过质量筛选。
- contracts.mjs为全部候选断言硬语义预期，两个透明循环样本仅允许精确的transparency拒绝；所有无损/0候选（除已知拒绝）须完整等价，照片/地球在10/20须实际有损且通过质量。契约负例证明错误接纳、非预期拒绝、非有限度量及局部/时域超限使检查失败。

### 样本与报告

`target/gif-lossy-*/`保留`report.json`、`index.html`、来源/许可与原件/候选。HTML展示可播放GIF，标注被拒绝的候选；多个浏览器GIF不保证同步，播放语义由独立时间轴检查负责。报告`encodeMs`包含输入验证、工具身份复查与helper执行；`elapsedMs`再包含双输入比较、独占输出写入，不能把encodeMs当纯编码器CPU耗时。采样每10ms观察example及其直属固定helper；没采到短进程时峰值为null，不能称为RSS硬上限。

37个接纳输入：27个确定性正例、4个可接纳公开GIF、4个生成动画、1个公开照片平移衍生图、1个真实录屏缩放衍生图；6组参数共222次。三像素/两帧transparent-seam缩小了透明插画的循环问题：helper将最后Background改为Keep，第二轮首帧残留末帧像素，独立人工像素回归与生产验证都拒绝。公开transparent.gif含不支持应用扩展、dispose_prev.gif含零延时多帧；原始录屏超累计解码限额，另生成320×309输入。来源/许可见[补充素材](../../tests/fixtures/gif-lossy/README.md)，收益不混入缩放效果。

本机首轮显示原生10/20是进一步评估的候选：旋转地球相对输入节省约7.59%/11.60%，照片平移约7.41%/11.71%；录屏10仅节省2 bytes，20节省28 bytes，复杂度增加未必值得。40/80使部分照片/动画超过上述筛选范围；透明插画在所有参数（包括无损）下触发透明覆盖拒绝，原件保留。不能因为配方名为无损就绕过候选复验。

4B单文件v1质量映射与安全输出已实现，见[核心说明](../../native/gif/README.md)。gif:core:check在生产路径额外生成固定暗部噪声，验证有压缩潜力但RGB/时域误差不合格时QualityBelowTarget回退；录屏2/28 bytes收益按InsufficientGain回退/NoGain。批量/桌面有损及最终观感继续后续验收。
