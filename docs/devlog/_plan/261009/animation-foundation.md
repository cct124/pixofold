# 动画验证底座与GIF最小实验

- 创建日期：2026-10-09（Asia/Shanghai）。
- 状态：截至2026-10-13，GIF无损/v11基线（0830dab）与扫描日志、4A及4B第一段（395a0ea）均已入库。实验边界、222组对照和单文件有损质量/收益/回退/安全输出已通过本机验证。下一步同步接入4B批量预算与4C/v12报告消费者，再推进APNG；人工观感、新SHA三平台、其他系统安装及无开发工具/后置无WebView2分别验收，完整动画任务继续活动。
- 分支/基准：dev/0f4fcfd0825f51542d5d2658da600d21e0174350，业务a21d330，开工本地/远端一致且工作区干净。
- 关联：[阶段计划](../260930/next-development-plan.md)、[J2桌面混合工作流](jpeg-desktop-workflow.md)、[项目方案](../../../架构设计文档/pixofold-proposal.md)。

## 目标与阶段边界

用户已反馈PNG/JPEG手测功能正常，JPEG预算修复与新SHA平台检查已通过。下一主要开发进入动画阶段，首轮聚焦GIF可复现语料、独立播放验证与固定Gifsicle最小实验；实验通过后再建立GIF无损核心和桌面闭环。APNG完整底座按后续批次扩展。J2剩余安装/视觉证据同期收尾，明确故障优先修复。

GIF/APNG沿用现有输入授权、任务冻结、唯一worker池和输出提交边界。实际支持由通过验证的引擎决定；实验阶段保留现有生产格式能力。新抽象只覆盖已证明共用的语义，两个格式保留自己的循环、元数据、颜色和编码路径。

## 实施顺序与交付

| 顺序 | 范围 | 可审阅产物与完成条件 |
| --- | --- | --- |
| 0 | JPEG阶段收尾 | 94dd86c包含预算修复5f68448，三平台统一检查/构建/部署与JPEG jobs已通过；细分安装/视觉欠项单列，新增故障先修复 |
| 1 | GIF优先的动画语义、语料与验证底座 | 确定性GIF样本和manifest；画布/帧区域/延时/循环/合成模型；有界独立解码与时间轴比较；APNG完整模型后续扩展 |
| 2 | Gifsicle最小实验 | 固定官方源码版本、提交/归档哈希及构建配方，Windows先验证，记录许可证、无损参数、语义正确性、体积、耗时和资源数据 |
| 3 | GIF无损核心 | 可信工具身份、受控子进程、格式专属请求/报告/错误、实际执行限额、验证及安全输出；取消/超时/退出/NoGain/冲突回归 |
| 4 | GIF桌面闭环 | 同一引擎贯穿能力/扫描/规划/批量/预览；生成契约与消费者同步，混合批次、重试、备份、结果定位和安装手测 |
| 5 | 后续格式能力 | GIF有损先完成质量映射和动画观感验收；APNG先实现严格无损，再验收共享调色板/半透明/闪烁等有损行为 |

## 动画语义与分批语料

1. GIF覆盖GIF87a/89a、单帧/多帧、全局/局部调色板、交错、透明、偏移/局部帧、不同dispose、空透明帧、相邻重复画面、不等延时及循环结束画面。无循环扩展、有限循环和无限循环分别保留，不能直接套用APNG的num_plays整数。
2. APNG阶段覆盖SOURCE/OVER、NONE/BACKGROUND/PREVIOUS、首帧PREVIOUS特殊行为、默认静态图参与/不参与动画、帧偏移、序号与声明帧数、透明/半透明及16位样本。该阶段先建立验证范围，编码支持范围由后续实验明确。
3. 延时使用精确整数/有理数表达，显式处理零延时及APNG零分母，避免浮点舍入或浏览器播放钳制掩盖改变；无法确定等价的情况保护原图并拒绝。
4. 无损比较画布、完整合成画面及其持续时间、循环语义和最终画面；允许经证明等价的帧结构变化，不能只比较帧数、文件字节或首帧。零延时/空帧须保留其影响后续合成的语义。APNG另保留默认静态图与相应原始数据/元数据不变量。
5. 语料以自生成小样本为主，保存生成入口、输入哈希、预期合成结果和时间轴；第三方样本记录来源与许可。加入截断、错误帧区域、缺失调色板、非法索引、错误APNG序号/CRC、资源溢出及未知/保护扩展的拒绝策略。
6. 独立验证器使用与优化工具不同的解码路径，并用人工确定的微型画布核对合成算法。加入修改延时/循环/偏移/像素后的负例，证明验证器能识别动画改变。

## 资源与引擎实验约束

- 限制输入大小、画布尺寸、帧数、单帧/累计解码字节、元数据和执行期限；使用checked运算。逐帧比较，按实际算法保留必要画布/前态，避免将全部合成帧留在内存。
- 资源预约须覆盖解码缓冲、合成画布/前态、输入输出、验证及原生工作集；头部/扫描只收紧上限，真正解码和子进程执行仍须使用相同限额。先量测实验数据，再选择明确的产品默认值。
- Gifsicle实验只读取隔离样本并写临时输出，不使用原地修改模式。评估不同无损优化级别、空帧/扩展保留及兼容参数，独立检查时间轴；输出无收益计为NoGain，不能用压缩率代替正确性。
- 固定源码与构建产物身份，保留许可证及源码获取方式；运行时不从任意PATH找工具、不自动下载。生产接入时评估复用JPEG的可信随包/生命周期机制，只提取实际共用部分。
- 取消/超时/退出须回收原进程、管道线程和临时产物后才释放worker/预算；最终输出仍由现有提交层完成，错误/无收益/取消保留原图。

## 代码入口与已有边界

- [格式模型](../../../../crates/pixofold-core/src/model/mod.rs)已有规划GIF/APNG，尚不代表真实引擎；[批次格式与共享引擎](../../../../crates/pixofold-core/src/batch/image.rs)实际仅PNG/JPEG。首轮实验完成前不将规划格式加入supportedFormats。
- [PNG完整结构检查](../../../../crates/pixofold-core/src/probe/mod.rs)明确拒绝acTL/fcTL/fdAT。APNG应建立独立动画路径，避免取消该拒绝后让静态pipeline接受首帧。
- [JPEG子进程所有权](../../../../crates/pixofold-core/src/jpeg/process.rs)及tools/jpeg-lab提供现有可信实验/回收参照；先核对格式依赖，再决定共用代码范围。
- [现有语料入口](../../../../tests/fixtures/README.md)与[测试生成器](../../../../tests/fixtures/generate.mjs)可评估扩展；新实验命令和构建源码位置在实施时同步package.json/说明，不将规划命令写成已存在。

## 验证与交付门槛

- 首轮：确定性语料复生成检查、微型画布合成正负例、精确时间轴/循环测试、资源上限/损坏输入测试、固定Gifsicle真实无损输出及输入哈希保持；记录环境、配方与实际结果。
- 生产接入：按变更运行定向回归、pnpm check、真实动画核心入口和随包部署验证；桌面协议变化同步生成类型、运行时验证和消费者。新SHA平台结果、Windows安装/GUI、动画播放观感与RSS各自记录。
- 有损开放：质量映射边界及实际输出报告完整，比较体积与观感，人工检查透明边缘、背景残留和闪烁；无损路径独立保持。
- 本轮只记录计划与平台快照；现有自动检查结果归属J2，不作为未来动画代码证据。

## 2026-10-09 规划依据与下一入口

- 核对[GIF89a规范](https://www.w3.org/Graphics/GIF/spec-gif89a.txt)、[PNG第三版动画规范](https://www.w3.org/TR/png-3/)及[Gifsicle手册](https://www.lcdf.org/gifsicle/man.html)。Gifsicle优化可改变局部帧结构并可能增大体积，默认空帧行为也需验证；APNG的默认静态图、精确延时和dispose/blend均属于播放契约。
- 下一次实施从第1/2项开始：先准备微型GIF/APNG样本及其预期合成/时间轴，评估解码依赖与固定Gifsicle构建，再实现有界独立验证和无损实验。等待中的JPEG CI不阻止该独立准备，明确平台失败仍优先处理。
- 开工时同步本连续记录，首轮交付后按实际结果决定GIF无损生产参数与接入范围；完整验收前本任务保持活动。此次文档变更保留本机，未提交推送。
- 本轮交付检查：4份devlog文档的90个本地链接、NUL和尾随空白检查通过，git diff --check通过；改动为本记录（新增）、开发索引、阶段计划及J2第二段平台补记。未改业务代码或重跑已有功能测试/构建。

## 2026-10-10 首批交付细化（待实施）

- 本日基准dev/94dd86c，本地/远端一致，包含预算修复5f68448；六个CI jobs已经通过，具体身份与时间续记[J2第二段](jpeg-desktop-workflow.md)末节。既有创建日和前轮过程保留，后续实施仍续写本任务。
- 当前实际ImageKind/ImageEngines仅PNG/JPEG，导入将GIF识别为未支持，PNG路径仍拒绝acTL/fcTL/fdAT；Cargo与实验命令未接GIF/APNG。首轮交付聚焦GIF无损所需证据，避免同时实施两个格式的完整解码/编码和桌面迁移。APNG语义保留为后续验收要求，此轮不实现完整APNG解码/编码。
- 首批按三步交付：①自生成GIF87a/89a语料、复生成入口、哈希与期望画面/时间轴，覆盖透明/局部帧/调色板/交错/dispose/延时/循环；②评估并锁定独立GIF解码依赖，用微型画布核对合成，逐帧有界比较及错误像素/延时/循环/帧偏移负例；③固定官方Gifsicle源码/哈希/Windows构建配方和许可，比较无损优化参数的实际结果，再纳入三平台实验检查。命令拟新增gif:lab:build/gif:lab:check，当前尚不存在。
- 完成门槛：优化后完整画面时间轴/循环等价，损坏和超限输入明确拒绝，输入哈希不变，收益/NoGain与耗时/工作集有记录，验证器能识别故意破坏的负例。固定版本/参数以实际实验选择，不预先声称某版本或优化级别必然最优。
- 首个生产交付随后分为GIF无损核心（可信工具、资源预约/实际执行、候选验证和安全提交）及GIF桌面接入（真实能力/混合批次/协议与报告/预览/结果定位）。有损质量映射与APNG完整底座各自验收，不能直接将实验输出当作桌面支持。
- 用户后置的缺少WebView2环境和其它未完成原生/视觉验收保持独立记录；现有JPEG安全缺陷若出现优先修复，已完成自动验收不重复作为新开发前置。本轮仅计划细化和文档维护，未实施上述代码或提交推送。
- 本轮4份开发记录diff已审阅，git diff --check、95个本地链接及NUL/尾随空白检查通过；只保留本轮文档改动，后续代码实施从本节首批三步开始。

## 2026-10-10 首批实施

- 用户明确要求实施GIF语料、独立播放验证和Gifsicle无损实验。开工dev/94dd86c，保留上述4份规划/平台文档；沿用本连续记录，不接桌面格式能力。
- 核对官方发布入口当前提供Gifsicle 1.96源码，拟固定该归档及SHA256；独立解码评估Rust gif 0.14.2，使用精确版本、只引入实验开发依赖。需显式启用帧区域/LZW结束码检查及解码内存限制，结构、帧数、累计解码、元数据和播放语义由验证器另行约束。具体依赖/许可/配方在源码核对后补记。

### 首批本机实现与决策

- 固定Gifsicle 1.96官方归档，SHA256为fd23d279681a6dfe3c15264e33f344045b3ba473da4d19f49e67a50994b077fb，按GPL-2.0-only独立CLI使用。Windows以MSVC Release /MT、上游win32cfg及含kcolor.c的完整列表构建，关闭工具内部线程/SIMD；源码缓存、配方及二进制各自校验。详细命令与边界见 [实验说明](../../../../tools/gif-lab/README.md)，完整第三方通知和源码要求同步到根THIRD_PARTY_NOTICES。
- `tools/gif-lab/fixtures.mjs`自写微型GIF编码器与人工画布oracle；45项固定样本=27有效+18拒绝，manifest记录SHA256、结构/语义预期和8组等价正负比较。覆盖静态/动画、调色板/透明/偏移/交错、dispose、延时、循环和元数据；预算用收紧额度触发。另生成512×256×16帧压力图参与实验。全部为自生成隔离输入，不触及用户图片。
- `examples/gif_lab`为开发验证模块，`examples/gif_validate.rs`为CLI，`tests/gif_lab.rs`接单元/集成检查；只新增gif =0.14.2、weezl =0.1.12开发依赖，gif仅开启std。手工合成与独立索引解码比较完整画面摘要、精确厘秒、原始循环语义及注释，透明与逻辑背景均验证，存在循环时保留画布再执行第二轮。循环接缝另加人工第二轮画布防止错误重置。
- 资源预检与解码实际执行均使用相同限额，只允许收紧：16MiB输入、各4Mi像素画布/单帧、256帧、64MiB累计RGBA（背景/循环代表轮次计费）、64KiB元数据。验证器为帧间10秒软期限，Gifsicle单次10秒并由OwnedChild保证回收后返回。10ms目标间隔只采样直属工具工作集，既非RSS硬上限，也非生产worker/预约验收。
- 明确拒绝损坏结构/LZW/缺EOI/非法索引与预算溢出，零延时动画、用户交互、未知应用/文本扩展、非零像素宽高比及保留字段暂不支持。扩展/评论保留分别由结构和输出比较验证，不能把工具成功退出当作无损证明。

### 故障与修复

- Windows CMake首次遇到反斜杠源码路径转义问题，配方加入file(TO_CMAKE_PATH)后构建通过；不修改上游源码。
- 原先依赖gif逐行解码时，跨行LZW字典码被误判缺EOI。改为gif skip_frame_decoding读取帧，再用weezl全帧有界解码、超额像素/EOI检查并手动交错重排。新增interlaced-dictionary.gif固定字典码样本，与人工七行画布比对；缺EOI仍必须拒绝。早期诊断报告target/gif-lab-rMAPyQ/与target/gif-lab-8y90pT/保留，不作为最终全组证据。
- 收尾核对发现累计解码原只计一次帧序列，未把双背景和循环第二轮的实际重复工作计入。结构结束前按2/4倍checked计费，64MiB限额覆盖全部代表轮次；新增单帧48字节/循环192字节的正好可用与少1字节拒绝回归。此修正后再运行最终完整检查和实验，避免以文档额度替代实际约束。

### 首轮实验结果

首轮全组报告 `target/gif-lab-JNFp3k/report.json` 为passed：45项语料+8组比较、28个可优化输入×6配方=168项，所有源哈希不变。工具SHA256为59d744699a09f7c0635433e5f42e26c59c500441a63997e602017ffa14ed14a2，目标x86_64-pc-windows-msvc；完整源码/配方身份见同目录engine-identity.json。收尾修正后的最终交付证据见下节kboUBd。

| 配方 | 有收益 | NoGain | 验证拒绝 | 合计耗时ms | 采样峰值bytes |
| --- | --- | --- | --- | --- | --- |
| default O1 | 26 | 2 | 0 | 1212 | 6901760 |
| default O2 | 24 | 2 | 2 | 1212 | 6938624 |
| default O3 | 24 | 2 | 2 | 1264 | 6549504 |
| careful O1 | 26 | 2 | 0 | 1226 | 6492160 |
| careful O2 | 26 | 2 | 0 | 1309 | 6610944 |
| careful O3 | 26 | 2 | 0 | 1214 | 6426624 |

- default O2/O3的larger-pattern.gif与stress.gif候选透明索引超出调色板，严格结构验证以InvalidGif拒绝；这不等同已证明所有播放器显示错误。实验断言只允许这两项已解释拒绝，其余语义或结构变化失败；careful O2必须进入全组安全配方。
- 下一阶段候选参数确定为 `--careful -O2 --same-comments --same-extensions` 并强制独立候选验证。O1也通过本语料，O3尚无额外收益依据；不从单轮耗时选择最优等级。两项NoGain仍保留输入，不因无损结构合并或体积减少跳过验证。
- 语料生成/核对、实验构建/运行新增package.json入口，统一fixtures:check包含GIF；新增三平台gif-lab job构建/实验并保留target证据。Windows本机已构建，Linux/macOS configure配方及新SHA三平台CI均未运行。现有94dd86c的JPEG CI不覆盖这些未提交改动。
- 本轮业务支持仍为PNG/JPEG，未改DTO/界面，也未构建新MSI。文档/许可、人工循环接缝预期及累计解码计费修正后的最终验证如下。

### 最终验证与证据

- Windows本机采用仓库便携Node 26.9.0及Rust 1.98.1。`pnpm check`完整通过，日志 `target/gif-stage1-check-261010.log`：Prettier/rustfmt、Oxlint、TypeScript、159项前端、34项PNG/45项GIF语料一致性、生成类型只读检查、全目标/全特性Clippy、44项核心单元、全部核心集成（含新增3项GIF回归）、113项桌面及2项doctest均通过。
- 最终 `node tools/gif-lab.mjs check`（对应pnpm gif:lab:check）通过，日志 `target/gif-stage1-experiment-261010.log`，报告 `target/gif-lab-kboUBd/report.json`：45项语料/8组比较、168项优化结果与首轮一致，全部源SHA256保持；本次包含第二轮人工预期、完整重复解码计费及两份MIT通知。careful O2合计977ms，工具采样峰值6,557,696 bytes；这些数据只覆盖合成语料、当前环境和直属子进程。
- 最终证据目录保留输入/候选、engine-identity.json、完整Gifsicle COPYING/README及gif/weezl MIT原文。两份MIT通知已与锁定上游源码核对（只忽略行尾/末尾空行）。固定工具构建日志 `target/gif-lab-build-261010.log`保留。
- 8份涉及文档的154个本地链接、NUL及尾随空白检查通过，脚本 `target/gif-stage1-documents-261010.mjs`；git diff --check通过，diff及新文件已审阅。第三方通知同时将JPEG第一段PNG-only注明为历史状态，并链接现有混合工作流边界。工具/target缓存继续由Git排除，新增测试素材为本项目生成。
- 首批Windows本机实施完成，Linux/macOS配方和新SHA三平台GIF实验尚无运行证据；现有JPEG平台结果不覆盖本轮。完整动画任务继续活动，后续范围从生产核心开始。

### 下一入口与交接

- 首批本机交付范围：GIF开发example/测试/语料/构建实验工具、Cargo锁定开发依赖、package命令、CI、根README/THIRD_PARTY_NOTICES与相关文档；前轮阶段计划/J2平台补记一并保留。改动随后按用户指令提交为0c0693f（见末节），首批本机门槛已完成。
- 提交推送后核对新SHA三平台gif-lab与统一检查。后续从GIF无损生产核心开始：可信工具随包、格式专属请求/报告/错误、预检/执行/预约一致限额、独立候选验证、安全提交与取消/超时/关闭回收。核心完成后再接混合批次/桌面，APNG完整解码/编码和GIF有损各自验收。

## 2026-10-10 第二阶段计划：GIF无损生产核心

- 本次根据用户询问细化下一阶段计划；第二阶段代码尚未实施，当前仍为第一阶段未提交工作区。沿用本连续记录，保留创建日及首轮全部证据。第二阶段交付为可独立调用且安全提交的GIF无损核心，再接核心混合批次；桌面开放GIF安排在第三阶段。
- 第一阶段新SHA三平台实验仍待本轮明确提交推送授权，授权后优先核对Windows/macOS/Linux的固定构建、语料与真实优化结果。平台失败先定位修复；独立的本机核心准备可以继续，平台能力声明依据实际新代码结果。

### A：单文件无损核心

| 顺序 | 任务 | 交付与验收 |
| --- | --- | --- |
| 1 | 将实验验证迁入核心 | 新建格式私有结构/播放验证模块及GifRequest、GifLimits、GifInfo、GifReport、GifError；gif/weezl从开发依赖转为锁定生产依赖；example作为调用核心的验收入口，人工画布/语义负例继续验证 |
| 2 | 可信工具与受控执行 | 固定1.96及careful O2配方，核心显式注入经验证的引擎；核对目标/工具身份，使用受控输入、输出与有界管道，稳定区分缺失、替换、异常退出、超时、取消和清理失败 |
| 3 | 实际执行限额 | 输入、画布、单帧、帧数、累计解码、元数据与候选大小均在实际读取/执行时约束；生产验证器传入取消和期限，检查长帧内的有界工作片段；独立列出累计解码工作量、常驻缓冲与原生工作集 |
| 4 | 验证后安全提交 | 接现有Source/Destination/output边界，候选落盘后复查内容/身份并比较完整播放/元数据；提交前复查源与取消，覆盖/备份/副本冲突使用已有策略；GIF备份保留原始gif扩展名与大小写 |
| 5 | 真实核心回归 | 拟新增gif:core:check：真实单文件无损、NoGain、候选结构/画面/延时/循环/元数据变化、取消/超时/工具替换、源变化、冲突/权限/清理失败与超限回归，失败时验证原图哈希保持 |

- 初版采用第一阶段已验证的保守输入子集和 `--careful -O2 --same-comments --same-extensions`；零延时动画、交互帧和未知/保护扩展返回格式专属的明确错误。GIF有损请求明确不支持，质量映射在后续阶段单独验收。元数据正文和私人路径只留内部，不进入报告日志白名单。
- 16MiB输入、4Mi画布/帧像素、256帧、64MiB累计解码及64KiB元数据仅作为初版保守范围依据；64MiB是双背景/循环代表轮次的解码工作量，不能直接当作峰值内存或批次预约。需核对Gifsicle原生分配和失败路径，选择可实际执行并验证的资源约束；未解决前不能用当前约6MiB采样峰值证明生产上限。
- 取消、超时和退出必须完成原进程wait、管道线程join与临时产物清理后才释放所有权。以实际子进程/线程回收证据验收，清理失败保留可识别恢复上下文；同步锁不覆盖解码和进程等待。评估最小可复用进程所有者，命令、格式验证和错误仍属于各自引擎。
- A完成条件：真实GIF输出经独立播放验证，所有拒绝/无收益/取消路径保留原图；明确的执行上限、异常回收、定向核心回归及pnpm check通过。工具与源码许可资料同步，Windows先验；三平台按新SHA实际结果收口。

### B：核心批量与资源调度

- 扩展ImageKind/ImageEngines、格式请求/报告/错误、scan_with_engines与规划，把GIF接入现有唯一worker池；扫描冻结格式与头信息，执行重新核对，不凭旧扫描尺寸放宽额度。新增核心枚举时同步Rust消费者的显式处理，桌面能力/协议开放在下一阶段完整迁移。
- 预约覆盖父进程输入/候选/验证缓冲、当前/恢复画布、索引/压缩缓冲及原生工作集；使用与实际执行相同的收紧限额，验证取消/超时后真实回收才释放worker/预算。继续使用现有全局预算及整图并发策略。
- 运行PNG/JPEG/GIF真实混合批次，覆盖资源不足排队、超限逐行失败、源增长/格式替换、普通重试、失败/NoGain保留原图，以及副本同名/父子/输出输入交叉冲突隔离；正常行继续，输出层noclobber保持。
- B完成条件：混合批次进度/终态/汇总来自实际核心任务；失败、取消、清理与重试语义正确，PNG/JPEG已有回归保持；gif:core:check、pnpm check及新SHA平台检查通过，并记录大画布/多帧与并发工作集证据及其限制。

### 第三阶段入口

A/B稳定后接GIF桌面闭环：可信资源prepare/打包与加载、supportedFormats、生成协议和运行时验证、格式专属状态/报告/错误、受限预览、混合导入/重试/备份/结果定位。界面需明确GIF仅无损时的设置行为；真实动画观感、Windows MSI和原生/视觉矩阵分别验收。GIF有损和APNG完整底座按后续批次安排。

- 本次只细化计划及索引，未新增核心实现、未运行功能测试或构建，也未提交推送。文档校验脚本的8份文档/154个本地链接、NUL/尾随空白及git diff --check通过。计划实施仍先更新本记录，再定位定义/全部引用并分批修改；开发过程中按实际失败和证据调整范围。

## 2026-10-10 第二阶段实施

- 用户明确授权实施上述计划，先A验收，再B集成；开工HEAD/origin/dev仍94dd86c，保留第一阶段和前轮所有未提交文件。当前没有本轮提交推送授权。
- 已定位现有JPEG工具身份/子进程回收、Source/Destination/output安全提交及ImageEngines/scan/批量资源边界。GIF验证仍在开发example，需迁入核心并加入取消/长帧期限。
- 原生预算方案开始核对上游固定源码中的所有malloc/realloc/free路径：拟增加本项目独立helper入口与有界分配封装，源码缓存保持不变；原生分配上限和Rust输入/验证缓冲分别计费，RSS仅作观测。实现和验收结果随后追加。

### A/B本机实现进展

- 验证模块已从example迁入核心gif/structure、playback；开发入口改为调用生产API，人工manifest继续独立核对。新增GifLimits/GifRequest/GifInfo/GifReport/GifError及可信GifEngine；gif/weezl转为同版本生产依赖。源码读取/落盘复验/提交复用Source/Destination/output，GIF备份保持原始扩展名大小写，扫描排除严格保留的GIF备份名。
- 验证保留双背景/循环接缝、精确厘秒及元数据不变量，公开播放摘要仅含评论SHA256；加入长帧LZW分片、逐段像素/哈希、dispose与帧间取消/期限检查。验证缓冲按双画布、索引、最多两份压缩输入容量及1MiB余量计费；64MiB累计解码与缓冲额度分别执行。
- 独立native/gif入口和分配封装在固定源码编译时重定向所有malloc/calloc/realloc/free；含对齐头和realloc新旧同时存活峰值，超限固定86退出。默认原生请求分配32MiB，可配置至256MiB；初版保守全图预约为8份输入容量+验证缓冲+原生分配+32MiB系统/线程余量，默认256MiB，头探测可进一步收紧同一执行额度。该约束不称RSS硬限额。
- 新增封装按GPL-2.0-only OR GPL-3.0-or-later提供，独立helper按GPLv2组合；未重许可既有代码。固定上游源码不改，构建身份新增helper及封装配方；后续随包阶段须携带对应源码/配方和完整许可。
- 子进程所有者回收child、三个管道线程及工作目录后返回；BrokenPipe不抢先kill以覆盖原生86退出码， stdout/stderr超限仍主动终结。实际spawn/wait与阻塞输入取消/超时回归通过，日志target/gif-core-process-261010.log。MSVC C头缺少max_align_t，封装使用64位16字节对齐分支并由自检验证，修复后原生构建通过，日志target/gif-core-native-fixed-261010.log。
- B已接ImageKind/ImageEngines、扫描/规划、格式专属错误/报告和共用worker。GIF头仅收紧画布/维度/验证缓冲，原生分配额度保持冻结；普通重试使用同一引擎及有效无损参数，拒绝GIF有损/PNG凭据许可。新增排队源变大/格式替换的真实pipeline门闩回归，保留源并释放资源，日志target/gif-batch-gates-fixed-261010.log。
- 桌面保持v10的PNG/JPEG实际能力：ImageKindDto改为显式TryFrom，GIF行/报告/能力快照返回InvalidSnapshot；新增拒绝契约回归，未将GIF映射成PNG/JPEG或开放GUI。Rust消费者和冻结重试参数已同步，全工作区all-targets/all-features编译通过，日志target/gif-batch-workspace-fixed-261010.log。
- gif:core:check已提供独立单文件和PNG/JPEG/GIF混合入口，真实检查通过：45项输入的接纳/拒绝及25收益/2无收益、备份/无备份、取消阶段、原生/验证额度、工具替换、只读/路径I/O失败、源变化、混合坏像素逐行失败、NoGain、稳定ID重试、总预算不足、GIF有损拒绝和副本冲突隔离。证据target/gif-core-LrxAQl/。
- 加入2048×1024单帧大图与512×256×16帧压力图后真实核心/混合复验通过，证据target/gif-core-6L6KTE/，日志target/gif-core-stress-261010.log；metrics.json记录当前example与直属固定helper的采样工作集，未采到helper时明确为null。候选落盘替换负例和完整pnpm check在下一步收口。

### A/B最终验证与证据

- Windows本机沿用便携Node 26.9.0、Rust 1.98.1及固定MSVC工具。最终 `pnpm check` 退出码0，日志 `target/gif-stage2-check-confirmed-261010.log`：Prettier/rustfmt、Oxlint、TypeScript、159项前端、PNG/GIF确定性语料、生成类型一致性、全目标/全特性Clippy、48项核心单元、全部核心集成（含3项GIF验证和进程回收/源变化门闩）、114项桌面及2项doctest均通过。最终业务代码检查后仅补充注释和文档。
- 最终 `node tools/gif-lab.mjs core-check`（对应 `pnpm gif:core:check`）退出码0，日志 `target/gif-stage2-core-final-261010.log`，证据 `target/gif-core-RoNjgr/`。有界分配自检、27项有效输入的25收益/2NoGain及18项拒绝输入、真实备份/无备份、四个取消阶段、资源/冲突/I/O/源变化/工具替换、三格式混合和稳定ID重试均通过；混合JPEG输出另经独立系数工具核对。
- 候选落盘后分别替换为改变延时、像素、循环与注释的四项有效GIF，均返回ValidationFailed；原图SHA256保持且暂存候选清理。独立验证器继续通过人工画布/循环接缝和8组语义正负比较；不能以候选字节匹配替代播放等价验证。
- 本轮2MP单帧图1,579,080→92,633 bytes、301ms；512×256×16帧压力图1,579,421→80,337 bytes、530ms。单文件验收采样父进程峰值25,620,480 bytes、helper峰值9,625,600 bytes、同时合计峰值25,620,480 bytes；混合验收同时合计峰值13,742,080 bytes。metrics.json保留10ms目标间隔、样本数及范围；此为合成语料/example及直属helper的采样结果，不作为真实用户压缩率或RSS硬限额承诺。
- 最终 `node tools/gif-lab.mjs check`（对应 `pnpm gif:lab:check`）退出码0，日志 `target/gif-stage2-lab-final-261010.log`，报告 `target/gif-lab-S1QM4M/report.json`。45项语料、8组比较和168项优化复验通过；careful O2为26收益/2NoGain/0拒绝，合计824ms，工具采样峰值6,946,816 bytes。default O2/O3保持前述各2项严格拒绝边界；全部输入SHA256保持。
- 核心证据保留engine-identity.json、Gifsicle完整COPYING/README、本项目native/gif封装源码和gif/weezl许可原文；真实混合另保留MozJPEG许可。固定helper SHA256为4352dd36bcb7b9298740da788c9c8fd9bd67728d8e828b1d239c5c5e8a932051；JPEG helper为8fc117b4851ae02d405995361eddccee646b3307a7584ef15fc62fe1d1acfd60。工具与验证数据继续由Git排除。
- 最终diff已审阅，git diff --check通过。文档校验脚本 `target/gif-stage1-documents-261010.mjs` 对本轮10份文档的167个本地链接、NUL及尾随空白检查通过；README命令表、核心说明、许可/迁移边界和devlog索引已同步。

### A/B交接与剩余门槛

- 本机实施已完成。HEAD/origin/dev仍为94dd86c，全部第一阶段与第二阶段代码、实验语料/命令、CI、原生封装、许可与开发文档尚未提交推送；保留前轮阶段计划/J2平台补记。本连续记录保持活动，不将本机通过写成跨平台或桌面GIF支持。
- 下一入口：得到本轮明确提交推送授权后提交到dev并推送origin/dev，核对新SHA的三平台统一检查/桌面构建、JPEG jobs和新增GIF构建/168实验/真实核心混合jobs；首次Linux/macOS configure+CMake有界helper配方尚无运行证据，有明确失败先定位修复。提交/同步/CI结果随后追加，不沿用94dd86c为本轮背书。
- 三平台门槛收口后进入第三阶段GIF可信资源prepare/随包与加载、桌面协议/能力/报告/错误、受限预览及GUI/安装。桌面v10当前仍PNG/JPEG；GIF有损、APNG和用户后置的无WebView2安装环境各自验收。本轮未运行新MSI或GUI。

## 2026-10-10 编写提交信息、提交与推送

- 用户要求根据工作区内容编写提交信息、提交并推送，无需执行其它任务。复核范围：101份文件（新增核心gif模块5份、native/gif封装4份、examples/tests、45份GIF语料与manifest、tools/gif-lab工具与许可；其余为ImageKind/引擎/扫描/规划/预算接入、CI、依赖、README/AGENTS、第三方说明与devlog），无未暂存改动与未跟踪残留（构建缓存与target证据按设计排除），git diff --check通过。
- 提交0c0693f「feat: 实现GIF无损核心与三格式混合批次」（101 files changed、5416 insertions、49 deletions）。沙箱内.git只读，add/commit/push按规则提升同一条命令执行，未绕开沙箱约束；94dd86c..0c0693f  dev -> dev已同步origin/dev，本地与远端一致。
- 沿用本轮已完成验证（pnpm check：159项前端、48项核心单元、全部核心集成、114项桌面与2项doctest、PNG/GIF语料与类型一致性、全目标Clippy；gif:core:check真实核心/混合含2MP与512×256×16帧压力样本；gif:lab:check 168项复验与careful O2 26收益/2NoGain）；本次未改业务代码，不重复构建或测试。桌面GIF、APNG、新MSI与新SHA三平台结果仍未取得，不据此声明通过。

## 2026-10-12 仓库与三平台CI复核、第三阶段入口

- 用户要求深入分析当前仓库与最近提交并制定下一步开发计划，本轮只读复核与记录维护。开工dev/6209a23、origin/dev同SHA，工作区干净；0c0693f为第一阶段及第二阶段A/B业务提交，6209a23仅追加交接文档。
- GitHub只读查询run38038904722：head_sha=6209a23fd22902b8ffe0974503746ad045887f42，push/dev、run59，completed/success；2026-10-10 16:44:02创建、17:02:22最后更新（Asia/Shanghai）。9个jobs全部success，平台及关键步骤如下。

| 平台 | Check job | JPEG engine lab job | GIF lossless lab job |
| --- | --- | --- | --- |
| Windows | 114175106115 | 114175105976 | 114175106175 |
| Ubuntu 24.04 | 114175106103 | 114175106094 | 114175106205 |
| macOS | 114175106247 | 114175106185 | 114175106212 |

- 每个平台的Check均通过完整格式/类型/lint/测试/Rust检查、正式无安装包桌面构建及开发环境外JPEG资源验证；JPEG job通过固定工具构建、系数/元数据/资源实验及真实JPEG核心；GIF job通过固定单线程Gifsicle构建、语料/独立播放/真实无损优化、固定JPEG helper构建及真实GIF核心/PNG/JPEG/GIF混合回归。首次Linux/macOS configure+CMake有界helper配方的平台门槛已取得，第一阶段与第二阶段A/B收口；未把核心实验结果写成GIF随包/安装或GUI已通过。
- 下一入口按[阶段计划本日复核](../260930/next-development-plan.md)推进第三阶段：3A可信GIF资源prepare/部署检查与生产加载器；3B同批完成v11/实际能力/启动注入/工作台与受限首帧预览；3C新SHA三平台部署/桌面混合CI及新MSI用户手测。完整动画任务保持活动，续用本记录。
- 当前v10扫描与DTO不能分批开放：3A仅由独立部署验收入口注入GIF，避免生产扫描接纳后被v10快照拒绝。建议GIF首版仅无损，在默认有损设置下明确模式提示和逐行失败/改为无损后的稳定ID重试；按行自动无损属于另一模式契约，若用户选择需同步请求/报告/重试。预览建议输出第一幅完整合成画布的静态PNG，执行既有预算/缓存/撤销规则。
- 本轮仅维护计划与当前证据，未改业务代码或运行功能/GUI/构建，也未提交推送；历史待验状态保留在各日期原节，本日以实际新SHA结果更新。GIF有损、APNG、Windows新包安装/细分视觉及用户后置的无WebView2环境继续分别验收。

## 2026-10-12 第三阶段实施（本机完成，平台与手测待验）

- 用户确认按计划实施。基准dev/6209a23，与origin/dev一致；保留开发索引、阶段计划、本记录和native/gif说明的四份既有文档修改。
- 顺序：3A固定GIF可信资源prepare、构建身份及独立部署检查；3B同批迁移v11、启动实际能力、混合工作台、仅无损提示及受限静态首帧；3C接入CI、完成本机正式构建与新Windows安装包，准备原生与视觉验收。
- 验收覆盖真实GIF处理/独立播放、资源身份拒绝、三格式混合及稳定ID重试、输出/备份/取消/退出、旧协议拒绝及预览撤销；已有PNG/JPEG回归按改动面复验。远端新SHA CI需后续提交推送后取得，GUI/安装结果按实际执行记录。
- 实施复用现有JPEG随包模式与GIF固定构建配方，不增加新编码依赖；资源部署先以独立入口验收，生产能力与协议一起开放。

### 3A：可信随包与独立部署（本机完成）

- 新增gif:bundle:prepare/check，复用固定Gifsicle 1.96有界helper配方；runtime携带完整上游归档、native/gif封装、构建配方及许可，build.rs校验目标/运行库/配方/源码/产物后内嵌helper SHA256。生产加载器不从旁置清单或PATH取得信任，JPEG/GIF启动能力独立降级。
- Windows prepare通过，helper SHA256为4352dd36bcb7b9298740da788c9c8fd9bd67728d8e828b1d239c5c5e8a932051。初次部署证据target/gif-bundle-tUt7Ar，缺失helper、替换helper并伪造清单均拒绝，源文件不变；日志target/gif-stage3-deploy-261012.log。桌面gif_过滤回归4项通过，日志target/gif-stage3-bundle-tests-261012.log。

### 3B：v11与三格式桌面工作流（本机完成）

- 启动在创建main窗口前分别校验JPEG/GIF并注入唯一任务/资产服务；v11能力、候选/行格式、GIF无损报告与错误、Rust生成类型和TS运行时校验同批迁移。GIF有损行返回unsupported_gif_mode，页面提示切无损后按稳定ID与revision重试，其余格式继续；GIF不继承PNG凭据许可。
- 工作台按实际能力显示格式、GIF帧数与原始loopCount；GIF首帧透明画布由Rust解码为静态缩略图，不执行helper。16MiB输入、4Mi画布及单帧像素、32MiB缓冲、5秒协作期限，复用应用单解码/缓存/撤销机制。核心预览2项测试覆盖独立语料首帧哈希、损坏输入、预算、取消与超时。
- pnpm check全部通过：15个前端文件/161项测试、50项核心单元与全部核心集成、117项桌面Rust测试、2项doctest、34份PNG和45份GIF语料、生成类型只读一致性、格式/lint/typecheck及全目标Clippy -D warnings。日志target/gif-stage3-check-261012.log；修正App旧范围文案断言及DTO版本注释后通过。
- pnpm gif:bundle:check通过，证据target/gif-bundle-bVJRh4/report.json，日志target/gif-stage3-desktop-261012.log。真实protocolVersion=11、15次预览、PNG/JPEG/GIF混合及稳定ID重试、NoGain、全部目标冲突、GIF/JPEG备份、退出回收、独立播放与源不变均通过。独立能力降级覆盖三格式、缺JPEG仅PNG/GIF、缺GIF仅PNG/JPEG、双缺仅PNG。
- README、AGENTS、GIF/JPEG说明、第三方通知与交互说明同步到v11实际范围；本机代码尚未提交。3C已配置三平台GIF部署/桌面混合CI与证据上传，JPEG回归、GIF真实核心及新Windows安装包证据见下节；新SHA远端结果和实际安装/GUI继续分别验收。

### 3C：CI与Windows正式包（本机完成）

- 三平台Check增加pnpm gif:bundle:check及gif-bundle-*证据上传；尚未提交推送，没有本轮新SHA的远端结果。6209a23的9个绿色jobs仍只覆盖前序代码。
- pnpm jpeg:bundle:check通过，日志target/gif-stage3-jpeg-regression-261012.log，证据target/jpeg-bundle-1ufy73/；沿用v11，PNG/JPEG混合、11次预览及JPEG方向1–8保持通过。pnpm gif:core:check通过，日志target/gif-stage3-core-261012.log，证据target/gif-core-df0G2H/；25项收益、2项NoGain、2项压力样本及PNG/JPEG/GIF真实混合通过。
- pnpm tauri build --bundles msi --ci完成Windows正式构建，日志target/gif-stage3-msi-261012.log。交付副本为target/deliverables/261012-gif-v11/PixoFold_0.1.0_x64_en-US.msi，4,964,352 bytes，SHA256为138d6e15ab334e662effc4d287c92ccf0f0dd06474c03d8b15251f24ff5981e6。
- 使用WiX dark解包至target/gif-stage3-msi-261012/，target/gif-stage3-msi-check-261012.ps1核对24个安装文件及GIF清单14个文件；report.json结果passed，installed=false。资源与本次构建逐文件哈希相符，包含GIF helper、完整Gifsicle源码归档、native/gif封装/构建配方与许可，以及JPEG资源；未包含bundle-check开发验收程序。
- 首次主程序逐字节比对发现差异，定位为Tauri打包时将唯一__TAURI_BUNDLE_TYPE_VAR_UNK标记改为MSI，只有3个字节变化。校验脚本确认长度、唯一标记位置与替换后的整文件SHA256，其余字节严格一致；报告保留打包前后哈希，不放宽其他文件校验。
- 业务改动与新增文件已完成diff复核；最终补齐README快照节的v11、三格式报告和旧v9/v10页面重载说明。target/gif-stage3-documents-261012.mjs检查10份文档的186个本地链接、NUL及尾随空白通过，git diff --check通过；构建/证据继续由Git排除。未运行新安装包或GUI，不将解包、后端部署或模拟IPC结果写成安装与视觉已通过。

### 第三阶段交接与手测清单

- 开工基准dev/6209a23；用户随后要求提交推送，本轮源码、协议生成类型、前端、构建/CI和文档已提交3bcfe6b并同步origin/dev（6209a23..3bcfe6b）。待检查新SHA的三平台结果，失败先定位修复，不能沿用前序CI。
- Windows新包：安装、启动、关闭与卸载；确认PNG/JPEG/GIF实际可用，启动/任务/退出日志正常。无WebView2环境沿用用户的后置安排；无开发工具的隔离系统及macOS/Linux安装继续分别取得证据。
- 导入与模式：文件/目录选择、图片区域拖放；静态与动画GIF、三格式混合；默认有损模式中GIF逐行失败，PNG/JPEG继续，切无损后重试GIF，成功项保持不动且后台任务不因页面重载重跑。
- 输出与查看：指定目录副本、保留结构、同目录冲突、覆盖前备份及原始GIF扩展名大小写；失败/NoGain原图保留；结果/备份定位、完成批次输出目录入口按实际输出工作，结合A1剩余原生场景验收。
- 预览与视觉：GIF缩略图仅显示第一幅完整画布，含透明/偏移样本；JPEG方向预览保持正确。外部打开真实GIF复核播放速度、循环及观感，应用内不提供动画播放。检查亮暗、中英文、小窗口和不同DPI下的提示、详情与按钮；未测项保持待验。
- 本连续任务继续活动，下一入口为新SHA CI及上述手测反馈；GIF有损质量/闪烁实验与APNG仍按后续阶段推进。

## 2026-10-12 编写提交信息、提交与推送

- 用户要求根据工作区内容编写提交信息、提交并推送，无需执行其它任务。复核范围：47份文件（新增核心gif/preview.rs、assets/gif.rs、engine_bundle/gif_bundle、build-support/gif.rs、gif-bundle程序与两个脚本、resources/gif说明；其余为DTO v11与转换、启动注入、工作台设置/行/消息/预览、CI、依赖与文档），无未暂存改动与未跟踪残留（gif runtime资源按设计由.gitignore排除），git diff --check通过。
- 提交3bcfe6b「feat: 接通GIF随包资源与v11三格式桌面工作流」（47 files changed、1696 insertions、175 deletions）。沙箱内.git只读，add/commit/push按规则提升同一条命令执行，未绕开沙箱约束；6209a23..3bcfe6b  dev -> dev已同步origin/dev，本地与远端一致。
- 沿用本轮已完成验证（pnpm check：161项前端、50项核心单元、117项桌面、2项doctest、PNG/GIF语料与全目标Clippy；gif:bundle:check真实protocolVersion=11与15次预览及三格式混合/稳定ID重试；jpeg:bundle:check与gif:core:check回归；Windows正式MSI与24个安装文件校验）；本次未改业务代码，不重复构建或测试。新SHA三平台结果、安装/GUI与视觉手测仍未取得，不据此声明通过。

## 2026-10-13 用户完成Windows手测后的日志复核

- 用户反馈前述5组检查均已完成：安装/启动/关闭/卸载与三格式可用性；原生选择/拖放/混合及GIF模式重试；输出/备份/冲突/定位；GIF/JPEG预览与外部播放；亮暗/中英/窗口/DPI。未报告具体异常，要求进一步从日志检查问题；本轮按用户反馈记录已完成，日志证据单独核对。
- 开工dev/0830dab，与本地origin/dev一致，工作区干净；前序GIF业务3bcfe6b与公开素材737f622已入库。
- 仓库logs最新为10月9日；当前正式版用户temp_dir下找到10月12日17:25:17启动、10月13日10:48:35最后修改的20,063-byte会话。沙箱临时目录隔离，提升仅用于只读访问正式版日志并在Git排除的target目录保留分析快照；不运行/终止/卸载应用。
- 下一步：解析会话/任务终态、错误类别、输出与退出顺序及日志健康；发现可复现业务问题再定位代码，验收记录按日志实际覆盖范围更新。

### 日志结果（Asia/Shanghai，本轮复核完成）

- 已保存原始快照与只读分析脚本至target/gif-manual-review-261013/，均由Git排除。原文件run-1791797117836-vEK9HmE7aDwS.jsonl，20,063 bytes，SHA256 df27575c9da499bd56d00f2540b94607ac79286b153f159bc68c74622cfd897c；76行JSON均可解析，带sequence的1–75连续且无重复，末条为独立健康汇总。版本release/0.1.0、Windows x86_64，会话自2026-10-12 17:25:17至2026-10-13 10:48:35。
- 启动含jpeg_engine_verified和gif_engine_verified；worker池30个、CPU线程32、预算4,111,122,432 bytes。日志没有构建SHA，不能单凭0.1.0头部验证确切安装包身份；本轮不读取或启动安装程序。
- 4个单图批次均完成，按batch_id/job_id/attempt分别核对，每项恰好一次job_started/job_finished，无重复终态或未闭合运行项；batch_finished计数与行结果一致。成功3项、NoGain 1项、失败0、取消0。成功路径均Reading→Optimizing→Validating→BeforeCommit且commit开始/成功成对，NoGain无提交事件。

| 批次 | 本地结束时间 | 结果 | 输入→输出bytes | 单项耗时 | 输出策略 |
| --- | --- | --- | --- | --- | --- |
| 1 | 10-13 10:44:36 | succeeded | 15,305→15,297 | 83ms | copy |
| 2 | 10-13 10:45:05 | succeeded | 15,305→15,297 | 25ms | copy |
| 3 | 10-13 10:45:47 | no_gain | 1,001,718→1,001,718 | 403ms | 未提交 |
| 4 | 10-13 10:48:11 | succeeded | 146,690→134,756 | 119ms | overwrite_without_backup |

- 4次thumbnail均ok，耗时2/2/2/4ms；同时检查INFO级result/error_code，没有被日志级别掩盖的展示失败。WARN/ERROR均0，未出现WorkerPanicked、ServiceFault、ToolIdentity/ToolIo/ToolExit/TimedOut、CommitFailed/CleanupFailed或shutdown_failed的已记录故障。
- 10:48:35出现application_stopping(exit_code=0)，其后session_finished，dropped_events=0、write_failures=0。源码lifecycle在订阅/任务shutdown及资产wait_idle后记录application_stopping，和这次正常收尾一致；本轮未检查OS残留进程或对所有临时文件作文件系统审计，不扩展为任意强杀场景保证。elapsedMs约6,260万为应用累计运行时间，任务耗时用elapsed_ms，不能误读成单张处理十几小时。
- 11次扫描均Complete，候选数依次1、1、1、0、0、0、0、0、0、0、1。selection 4–10发生在10:46:01–10:47:05，共7次0候选，均没有进入批次。tasks/worker.rs仅记录scan.files().len()与status，扫描单项失败/不支持/重复/排除项留在ImportScan.issues()及progress，没有写入日志。用户随后说明当时导入“apng和应该是受保护的jpg文件？”。源码probe遇acTL/fcTL/fdAT返回UnsupportedAnimation，JPEG结构检查对APP11/JUMBF保护及不支持元数据拒绝，故结合操作说明与当前边界相符，没有新增扫描缺陷证据。JPEG“受保护”仍是用户推测，缺少逐项日志，未将其具体类别或7次输入逐一认定。
- 本会话未出现多行混合批次、batch_retry_started、attempt>1、备份创建/保留、reveal或输出目录展示事件。格式/输入方式/源路径也未写入日志，无法仅凭相同字节数认定某个具体样本。用户明确反馈5组手测已完成，作为用户验收记录保留；本日志独立证据只覆盖上述实际事件，亮暗/中英/DPI、动画观感、安装/卸载不能从JSONL推导。
- 结论：在可观察运行范围内未发现明确业务故障；空候选已收到APNG/可能受保护JPEG的操作说明，与当前拒绝范围相符，尚无改动业务代码的证据。python target/gif-manual-review-261013/audit.py通过，report.json记录上述配对、终态、提交顺序与健康核对；两份开发文档的34个本地链接、NUL/尾随空白及git diff --check通过。仅维护开发记录和索引；未运行GUI/压缩/安装、未修改业务代码、未重复构建/测试、未查询或宣称新SHA三平台CI，也未提交推送。后续入口为核对新SHA平台门槛；无WebView2/无开发工具环境沿原计划分别验收，未来若加强日志可补充不含路径的扫描拒绝类别计数。

## 2026-10-13 平台门槛收口与第四阶段计划

- 用户询问下一步合理开发计划。本轮只读核对代码、远端分支/CI并更新连续计划；开工dev/0830dab，保留此前日志复核的两份未提交文档修改。GitHub分支dev与本地HEAD均为0830dab88a55e3c8338a0438bb2346a56f20fdcf。
- [CI run38234086653](https://github.com/cct124/pixofold/actions/runs/38234086653)，push/dev、attempt1，2026-10-12 18:05:14创建、18:20:18最后更新（Asia/Shanghai），completed/success。核对最新attempt全部9个jobs及关键步骤，详情保存target/gif-phase4-plan-261013/ci-0830dab.json。

| 平台 | Check job | JPEG engine lab job | GIF lossless lab job |
| --- | --- | --- | --- |
| Windows | 114760798744 | 114760798759 | 114760798379 |
| Ubuntu 24.04 | 114760798831 | 114760798679 | 114760798706 |
| macOS | 114760798883 | 114760798735 | 114760798675 |

- 三平台Check的统一检查、桌面正式构建、JPEG隔离部署、GIF隔离部署及PNG/JPEG/GIF混合步骤均success；三平台JPEG/GIF实验和真实核心/混合步骤均success。统一检查在该SHA包含samples:check，确认32张公开原件/4份许可的跨平台checkout后哈希门槛通过；这不扩展为所有系统的GUI/安装或prepare命令手测。
- 因此上节“新SHA三平台待核”在本次查询后收口，已完成的Windows手测不再重复列为普遍阻塞；完整动画任务尚有有损/APNG，继续活动。没有当前故障证据要求重做输出层、线程池、原生授权或性能专项。

### 小批补充：扫描诊断（可与4A并行，尚未实施）

- 根据本次7次空候选调查，给import_scan_finished增加accepted/rejected/excluded/duplicates及有界错误类别计数，区分unsupported_animation、JPEG凭据/元数据、资源、I/O、重复与产物排除。来源复用ImportScan.issues/progress，日志保持固定字段白名单，不输出路径、文件名、请求Debug或元数据正文。
- 如为关联混合任务补充format/mode，只记录冻结请求中的受控枚举。保持现有UI/任务行为及协议不变，不改变保护文件拒绝策略。
- 验收：APNG、保护/不支持JPEG、重复/备份排除分别得到正确类别计数；日志隐私过滤和正常退出健康回归通过。此项是诊断能力改进，不能据此追溯补造已保存会话的拒绝原因。

### 4A：GIF有损最小实验（下一主要代码交付）

1. 固定现有Gifsicle 1.96与有界helper、careful O2基线，实验范围先聚焦原生lossy参数。先核对固定源码内参数语义，以少量原生强度点比较，不直接将UI的0–100质量作为lossiness，也不先升级编码器。调色板缩减/抖动仅在结果需要时另列对照，避免首轮叠加多个变量。
2. 复用45项确定性语料与公开手测GIF，再补少量来源/许可明确的文字/UI、插画、透明边缘、照片动画和录屏运动样本。当前6张公开GIF不足以独立覆盖全部观感边界，新增样本限制总规模并保存哈希。
3. 现有Playback::equivalent依赖完整画面哈希，仅适用于无损；建立独立有损判定。结构合法、画布、循环、精确时间轴及透明覆盖、保护/评论元数据为硬门槛；按时间段对齐比较完整合成画面，允许已证明合理的帧分块/合并，不能依赖帧数相同。双背景继续覆盖；像素误差与帧间误差变化另作有界质量评估，保留无损判定路径。
4. 对每个样本记录原图/无损/有损体积、耗时、采样工作集、空间误差与时域误差变化；不能把正常运动量直接当作闪烁。输出可查看的原图/候选动画和对比报告，人工观察文字、透明边缘、背景残留和循环接缝；阈值及默认质量方案由实际实验决定。
5. 执行仍在隔离目录，保留源SHA256；取消/超时/资源超限/结构或时间轴改变必须识别并收尾。实验命令名称与报告模式在实现时固定；4A不先开放生产有损能力，也不为使实验通过而放宽现有安全拒绝。

完成条件：既有无损回归保持；硬语义负例能被拒绝；得到可复现的收益/质量/资源对照及质量映射候选，明确适用与拒绝范围。若没有稳定收益或可接受观感，保留GIF无损并转向APNG无损，避免无限扩展参数搜索。

### 4B/4C：实验通过后接入核心、桌面与产品验收

- 4B增加GIF专属质量映射与版本、请求/processing报告，明确实际有损、无损回退及NoGain；有效质量参数按批次冻结，普通重试保持稳定ID、原输出/备份策略与现有授权。安全验证/工具错误保留失败语义，质量/收益门槛下的回退策略在报告中明确。不得复用PNG凭据移除或质量测量语义。
- 资源预约覆盖新的双输入/候选画面比较缓冲和原生工作集，实际执行同一收紧限额；取消/超时/关闭仍等子进程、管道和临时产物回收后释放许可。复验源变化、候选替换、逐行/全部目标冲突、备份/NoGain及混合批次。
- 4C同批迁移Rust DTO/生成TS/运行时验证/能力与详情，按报告契约升级协议（预计v12）；移除“GIF仅无损”提示必须以真实生产能力为前提。核心、统一检查、JPEG/PNG回归、GIF bundle、实际新SHA三平台、新MSI及有损动画观感分层验收。

### 第五阶段：APNG无损

- GIF有损阶段收口后，先建立APNG独立结构/CRC/序号/声明帧数、精确有理数延时、默认图参与/不参与动画、SOURCE/OVER及NONE/BACKGROUND/PREVIOUS验证与拒绝语料。
- 通过独立编码方案实验后，再接有界无损核心、混合调度、协议/预览/桌面和部署。不能仅删除静态PNG的acTL/fcTL/fdAT拒绝，将首帧压缩误作动画支持；APNG有损继续单独推进。
- JPEG凭据移除、零延时GIF语义扩展、无WebView2/隔离系统、其他平台安装及没有当前回归依据的性能专项各自保留边界，按明确需求或证据安排。

## 2026-10-13 实施扫描诊断与GIF有损实验4A

- 用户确认继续下一步开发，授权范围为扫描拒绝分类日志与GIF有损最小实验4A。开工dev/cf16ea8（前轮计划已提交），本地origin/dev同SHA，工作区干净；本轮不提交或推送。
- 先完成有界扫描分类与隐私回归，再实现独立有损语义/质量对照、固定helper参数实验及可查看报告；逐步补充实际结果。GIF生产模式、v11协议及APNG范围沿既有契约，4B/4C在4A实验结论后另行推进。
- 验收：日志明确空候选的类别而不泄露路径/文件名；无损回归保持、动画时间轴/循环/透明负例被识别，真实候选收益/画质/时域误差/资源可复现，取消/超时/源不变有证据。

## 2026-10-13 编写提交信息、提交与推送

- 用户要求根据工作区内容编写提交信息、提交并推送，无需执行其它任务。范围为本轮4份文档（索引、阶段计划、本记录及公开素材任务归档至_fin/261013），无业务代码、依赖或构建配置改动，git diff --check通过。
- 提交cf16ea8「docs: 复核GIF桌面平台门槛并规划GIF有损实验」（4 files changed、121 insertions、15 deletions；含素材记录从_plan重命名至_fin）。沙箱内.git只读，add/commit/push按规则提升同一条命令执行，未绕开沙箱约束；0830dab..cf16ea8  dev -> dev已同步origin/dev，本地与远端一致。
- 沿用本轮已完成的只读核对（CI run38234086653三平台9个jobs、76条release日志复核与文档链接检查）；本次未运行GUI/压缩/构建或重新查询平台，不将历史CI结果写成新代码通过。GIF有损4A、扫描拒绝分类日志、APNG与各平台安装仍按计划待实施。

## 2026-10-13 扫描诊断与GIF有损4A本机交付

### 实施基准与实际改动

- 本次实际实施基准经Git复核为dev/2a23aa5，本地与origin/dev一致；cf16ea8为前序计划提交，2a23aa5为其交接补记。用户授权范围为扫描拒绝分类日志与GIF有损最小实验；本次代码、实验语料与文档均保留在工作区，未提交或推送。
- `tasks/scan_diagnostics.rs`从ImportScan的进度与问题列表生成日志：import_scan_finished含accepted/rejected/excluded/duplicates，import_scan_issue_summary按固定error_code汇总count，由selection_id关联。类别穷举，最多每类一条；区分动画、JPEG APP11凭据、其他元数据、资源、I/O、重复与产物排除，不记录路径、文件名、元数据正文或原始错误。
- 真实扫描回归组合APNG、APP11 JPEG、未知APP1 JPEG、缺失文件、重复输入与保留备份，断言1接纳/4拒绝/1重复/1排除及逐类计数；另一个selection验证resource_limit。日志不含PRIVATE_标记，正常退出dropped_events=0/write_failures=0。新字段用于后续诊断，不能追溯补造10:46–10:47旧会话的具体拒绝原因。
- 新增显式`gif-lossy-lab`特性、`gif::lab`和gif_lossy_lab example。内存候选共用可信Gifsicle 1.96、有界helper及子进程回收路径；None继续使用原有无损配方，实验强度为lossless/0/10/20/40/80。默认核心和桌面未启用实验能力，GifRequest、v11与Playback::equivalent保持原有无损契约。
- 独立流式比较保留画布、原始循环、评论、完整合成透明覆盖及原图实际画面转换时间点，双背景和两个代表循环按时间对齐。允许原图相同画面合并、候选拆帧；改变延时、同总长移动转换、循环、评论或透明的负例均拒绝。合成器与人工语料所有预期画布交叉核对；RGB误差与相邻误差差分另作观测。
- 同时存活缓冲按`画布像素×24 + (输入+候选字节)×4 + 2MiB`在完整解码前预检，受max_decoded_bytes约束；比较总工作量最多256Mi画布像素，共用绝对10秒期限。helper继续32MiB原生分配/30秒调用期限，取消与超时复用生产回收。新增5项实验测试覆盖语义、人工画布、误差、取消/期限及双输入预算。
- `gif:lossy:check`输出全部候选、report.json、index.html、来源/衍生说明与许可，接入三平台GIF job及artifact上传。新增243,942 bytes的Pintoch OpenRefine/Wikidata公开录屏和完整CC BY-SA 4.0许可；fixtures:check校验原件与许可SHA256，保留独立许可。照片平移复用Pillow hopper.gif，UI/渐变/透明插画由确定性生成器产生。

### 216组真实对照与阶段结论

- 证据：`target/gif-lossy-8ha7ol/`，执行日志`target/gif-lossy-4a-confirmed-261013.log`。36个接纳输入包括27个确定性正例、4个公开GIF、3个合成动画、1个照片平移衍生动画和1个真实录屏缩放衍生动画；每项6组，共216组，全部输入SHA256在运行前后保持。固定helper SHA256为4352dd36bcb7b9298740da788c9c8fd9bd67728d8e828b1d239c5c5e8a932051。
- 45项既有结构语料及8组播放比较先完整验收。公开transparent.gif的未知应用扩展、dispose_prev.gif的零延时多帧、OpenRefine原始806×779录屏的累计解码超限均如实保留拒绝；录屏另按固定helper生成320×309实验输入，收益只相对衍生输入计算。

| 原生强度 | 无损收益 | 有损质量候选 | NoGain | 超实验质量阈值 | 硬约束拒绝 |
| --- | --- | --- | --- | --- | --- |
| lossless | 28 | 0 | 7 | 0 | 1 |
| 0 | 28 | 0 | 7 | 0 | 1 |
| 10 | 26 | 4 | 5 | 0 | 1 |
| 20 | 26 | 4 | 5 | 0 | 1 |
| 40 | 26 | 2 | 5 | 2 | 1 |
| 80 | 26 | 2 | 4 | 3 | 1 |

- 初轮筛选假设为双背景RGB RMSE≤2.5、最差区间≤4、最大通道误差≤64、时域误差≤3；这些是实验筛选值，尚不是感知质量承诺或产品接受规则。无损收益依据原有完整播放等价，不因使用--lossy参数就将实际无损结果误记为有损。

| 代表输入 | 输入bytes | 无损候选bytes | 原生10候选bytes/节省 | 原生20候选bytes/节省 |
| --- | --- | --- | --- | --- |
| 旋转地球 | 1,001,718 | 1,002,028（NoGain） | 925,720 / 7.59% | 885,488 / 11.60% |
| 照片平移 | 106,036 | 105,939 | 98,179 / 7.41% | 93,623 / 11.71% |
| OpenRefine 320×309 | 88,976 | 88,976（NoGain） | 88,974 / 2 bytes | 88,948 / 28 bytes |

- 透明插画全部强度（含lossless）触发transparency硬拒绝，实验保留候选用于研究，原图不变；这说明无损配方名称不能代替独立复验。不是放宽透明约束的依据，也未据此宣称所有播放器都会显示错误。
- 216次encodeMs合计7,474ms、单次16–415ms，elapsedMs合计15,653ms、单次18–1,009ms；encodeMs含输入验证、工具身份及helper，elapsedMs另含比较和独占落盘，不代表纯编码时间。比较模型上限本轮2,097,540–13,952,136 bytes；每10ms采样example与直属helper，未采到短进程时峰值为null，不将采样当RSS硬上限或性能保证。
- 本轮结果支持继续4B受控实现准备。候选锚点为q100严格无损、q80评估原生10、q60评估20；原生40/80已有部分样本超实验阈值，更低质量与连续映射尚无结论。录屏微小收益要求4B定义最小收益门槛，并同时比较无损候选；不能仅因有2 bytes收益就接受画面改变。

### 验证结果与未验收范围

- `cargo test -p pixofold-core --features gif-lossy-lab --lib gif::lab`通过5项；真实扫描隐私/分类回归通过。
- 首次统一检查在Clippy的chunks_exact_to_as_chunks失败，改为as_chunks后定向rust:check/rust:test通过；最终`pnpm check`退出0，日志`target/gif-lossy-4a-check-final-261013.log`：格式、Oxlint、TypeScript、161项前端、全部语料/公开来源哈希、生成类型一致性、全目标全特性Clippy、55项核心单元、全部核心集成、118项桌面和2项doctest通过。未用豁免绕过失败。
- `pnpm gif:lossy:check`完成上述216组对照，硬拒绝为明确实验结果，未记为安全候选；`pnpm gif:core:check`通过25收益/2 NoGain、2项压力图及PNG/JPEG/GIF混合，证据`target/gif-core-N5JboT/`，日志`target/gif-lossy-4a-core-261013.log`。
- `pnpm gif:bundle:check`退出0，证据`target/gif-bundle-HMnbKn/`，日志`target/gif-lossy-4a-bundle-261013.log`；开发PATH外GIF可信身份、工具独立降级、三格式桌面后端/稳定ID重试、备份/冲突/NoGain及15次预览通过。现有资源身份检查通过，无须重新prepare。
- 收尾`git diff --check`通过；target/gif-phase4-plan-261013/check-documents.mjs核对9份变更文档的197个本地链接、NUL及尾随空白通过，实验README定向Prettier检查通过。完整统一检查后仅补充交接文档，未重复扩大测试。
- 本轮未运行实际GUI、安装/卸载、新MSI构建或有损动画人工观感验收；新GIF实验CI步骤尚无本轮提交SHA的三平台结果。旧0830dab绿色CI与用户对无损版的五组反馈均不代替这些证据。完整动画任务保持活动。

### 交接入口与未提交范围

1. 对照页为`target/gif-lossy-8ha7ol/index.html`，完整数值见同目录report.json；重点核对地球/照片在10、20下的运动纹理、文字/UI、透明边缘及循环接缝。浏览器并排GIF不保证同步，时间轴硬约束以独立验证结果为准。
2. 4B从质量映射版本、实际路径报告、相对原图及无损候选的收益门槛、预算与取消回收契约开始；质量/收益不足可走明确报告的无损回退，结构/透明/时间轴和工具错误保留失败语义。4C随后同步DTO/生成TS/运行时验证/能力/详情与混合重试。
3. 改动已提交395a0ea并推送origin/dev，待以实际新SHA核对三平台统一检查/GIF有损实验/核心/部署；生产有损开放仍需新MSI与观感验收。APNG及JPEG凭据移除不在本轮实施范围。
- 未提交代码：核心Cargo特性、gif/engine.rs与mod.rs、新gif/lab/*及两个example文件；桌面tasks/scan_diagnostics.rs、tasks/mod.rs/worker.rs与diagnostics/layer.rs/tests.rs。
- 未提交工具/语料：tools/gif-lab.mjs、tools/gif-lab/lossy-check.mjs、package.json、CI配置、.gitattributes及tests/fixtures/gif-lossy/*。
- 未提交文档：README、AGENTS、THIRD_PARTY_NOTICES、native/gif/README、tools/gif-lab/README及本记录/索引/阶段计划。target实验产物和.tools构建资源继续由Git排除；本轮未修改生产前端或生成DTO。

## 2026-10-13 4A后续复核与4B分段计划

### 当前依据与优先级调整

- 用户询问下一步合理开发计划。本轮复核HEAD仍为dev/2a23aa5，与本地origin/dev引用一致；保留上述全部4A/扫描诊断未提交改动。没有查询远端最新CI，也没有重新执行压缩或功能测试；本节为下一批计划，尚未实施。
- 216组报告支持继续GIF有损开发，原生10/20有候选收益；仍需先把实验观测转为生产接受规则。固定1.96源码手册及官方Gifsicle手册均说明lossiness控制允许的画面扰动，不能直接当作本项目0–100质量。质量映射和编码器升级分开处理，本批沿用已固定工具。
- `tools/gif-lab/lossy-check.mjs`目前只对结构语料的lossless/0强制无损等价，其他有损候选按screening分类后报告measured。因此216组执行完成不代表有损生产质量门槛已经落地；下一批应补充可失败的语义/拒绝契约。
- `gif::lab::Accumulator`的时域RMS目前按对齐区间之间的通道数平均。等价拆帧可能增加零变化区间并改变分母；这是代码复核得到的风险，尚未用新负例复现。生产接受规则使用该指标前，应验证拆帧/合帧不改变结论及循环接缝/瞬态误差边界。
- 批量`planning.rs`与`ImageRequest::gif()`仍拒绝有损，`QualityMapping`仅有PNG/JPEG，桌面`ipc::convert::report()`将全部GifReport固定转为GifProcessingDto::Lossless。单文件核心可先独立交付；批量有损入口开放必须和真实processing DTO、生成类型及前端消费者迁移同批完成，避免输出有损却报告无损。

### 下一批推荐范围：4B第一段（实验边界收尾与单文件核心）

1. **收尾三个具体实验缺口。** 为现有语料建立预期通过/明确拒绝契约，使非预期硬拒绝、错误接纳和无损基准变化导致实验检查失败；不以耗时/RSS或逐字节压缩大小作为跨平台固定断言。将透明插画拒绝缩成最小样本，定位到背景/帧处置/循环或合成/工具的具体差异，用独立像素预期锁定；根因不明时继续安全拒绝。补充等价拆帧/合帧、不同帧时长、局部瞬态误差和循环接缝的质量回归，确认或调整时域指标的归一化。
2. **明确版本化质量与收益策略。** 新增GIF专属mode、quality mapping、processing及fallback原因；q100严格无损、q80对应原生10、q60对应20作为待核对候选。完整0–100边界/单调性须有定义；首版可将最大强度保守限制在已评估的20，更低质量的更激进参数须有新增实验和观感依据，不从40/80自动外推默认值。
3. **比较三种结果。** 以原图和通过验证的无损候选中较小者为无损基准，再判断有损候选的额外收益。初始收益门槛可试用“相对基准至少1%且不少于1KiB”，用现有小图/录屏/照片数据验算后冻结；此数值是拟议产品规则，尚未验证或实现。质量/收益不足时采用有效无损结果，无损也无收益时NoGain；时间轴/透明/元数据/结构失败、工具异常、取消和超时保留明确失败或取消语义。
4. **接单文件真实输出。** GifRequest/GifReport表达请求与实际处理路径，报告区分实际有损、实际无损和有原因的无损回退，NoGain仍作为独立outcome。落盘后复验最终选中的候选，沿用Source/Destination/output的源变化、目标冲突、备份和取消保护；实验CLI保留拒绝候选的行为不能进入产品提交路径。
5. **按实际生命周期计费。** 重新核对源/无损候选/有损候选/验证缓冲的同时存活峰值，优先顺序执行并及时释放；明确整个任务及各阶段期限。取消/超时等真实回收后再结束，准备后续批量预约所需的单图预算接口。现有批量GIF缓冲收紧使用旧无损公式，不能在下一段直接套给双输入比较。

完成标准：真实单文件能覆盖有损成功、实际无损、质量/收益回退、NoGain与安全拒绝；原图/备份/副本行为正确，结果报告与真实像素变化一致；新语义和质量负例能使检查失败；pnpm check、gif:lossy:check及扩展后的gif:core:check通过。代表性动画观感可同期核对，但生产桌面开放须有实际观感证据。第一段不通过移除批量拒绝提前开放桌面。

### 随后交付：4B第二段与4C同步接入

- 同批迁移ImageRequest转换、批量规划/QualityMapping、资源预约与实际执行收紧，以及Rust DTO/生成TS/运行时验证/前端详情。报告结构变化按v12迁移，旧协议按既有规则拒绝；移除“GIF仅无损”提示以真实能力和完整消费者为前提。若核心API变化需要先改桌面编译适配，仍保留有损入口门槛及如实报告。
- 参数按已接纳任务冻结；普通重试使用当前合法模式/质量，同时保持稳定job ID、递增attempt、原输出与备份策略。复验PNG/JPEG/GIF混合、有损失败切无损重试、重载/关闭、候选替换、逐行/全部目标冲突和指定目录身份，不能绕过既有输入授权或引入第二套worker池。
- 预算和限额必须覆盖真实生产双候选路径，扫描只收紧；同一worker顺序处理候选，子进程/管道/临时产物回收前不释放预约。面向用户的详情显示实际有损/无损回退及原因，原生强度和实验误差只在有明确诊断用途时展示。
- 验收按受影响范围运行统一检查、GIF实验/核心/随包及JPEG回归，再构建Windows MSI，重点手测新GIF有损/回退/重试/详情与动画观感；用户已经完成的旧版五组检查保留原证据，不无条件要求重复。推送经明确授权后按新SHA验证三平台，不用0830dab旧CI为新代码背书。

### 分支基线与后续格式

- 当前4A代码及扫描日志适合作为独立基线提交，经用户明确授权后提交推送并验新SHA。新平台失败优先定位；等待提交或平台结果不妨碍独立的本机4B分析和单文件实现准备，不将等待写成所有开发的前置阻塞。
- GIF有损的质量/回退/报告和桌面闭环收口后，推进APNG独立结构/CRC/序号/精确延时/默认图/合成底座与无损编码实验，再接生产。若GIF候选在定向观感或可靠质量门槛上无法成立，则保留GIF无损并转向APNG，无限扩大参数搜索不作为默认计划。JPEG凭据移除、无WebView2、其他平台实际安装和性能专项沿原边界安排。
- 本轮只更新本连续记录、索引和阶段计划，未修改业务代码、运行新功能检查、提交或推送。git diff --check通过；既有文档脚本核对9份变更文档的198个本地链接、NUL与尾随空白均通过，前轮全部代码改动保留。

## 2026-10-13 实施4B第一段

- 用户明确授权先补实验验证边界，再完成GIF有损单文件核心。基准仍为dev/2a23aa5，保留前轮全部未提交4A与扫描诊断改动；不提交推送。
- 顺序：定位透明差异/补预期契约与时域指标边界→质量/收益/回退模型→有界单文件候选选择及落盘复验→真实核心与既有桌面无损回归。批量有损入口及v11保持既有门槛，协议迁移属于后续4B第二段/4C。

### 实验边界完成

- 通过固定helper的帧信息定位透明插画：最后帧的Background被改为未指定/Keep，前11帧仍恢复背景。新增3×1、两帧透明红点最小输入；第一轮两个输出相同，第二轮首帧原图alpha为[255,0,0]，修改末帧处置后为[255,0,255]。独立手工RGBA断言、完整Playback及有损比较都识别差异。真实helper同样触发透明拒绝，保留源和拒绝策略，没有改写无损等价规则。
- 比较器从实验lab迁入常规comparison模块，lab只保留显式特性的未经复验候选接口。时域RMS改为只对实际误差变化边界累计分母，加入worst_temporal_rmse；等价拆帧、不同帧时长、短暂高误差和循环接缝回归通过。单比较仍共用绝对期限及256Mi画布像素上限。
- contracts.mjs对所有实验候选执行预期检查：两个透明样本必须精确transparency拒绝，其余必须通过硬语义；无损/0基准完整等价，地球/照片低强度有损有收益且质量合格。Node契约负例验证错误接纳、非预期拒绝、NaN与像素/时域超限会使检查失败。报告模式升级pixofold.gif-lossy-lab.v2以说明时域归一化变化。
- 首次边界复验222组通过（37输入×6强度），证据target/gif-lossy-gvOPSJ/和target/gif-4b-boundaries-261013.log；原4A的216组报告保留为历史证据。每强度新增一项透明拒绝，其余收益/质量分组未变；最新schema与最终检查在收尾补记。

### 单文件核心完成

- GifMode、GifQualityMapping v1、GifProcessing/GifFallbackReason和GifReport requested_mode/processing/quality落地；全0–100映射为min(20,ceil((100-q)/2))，q100只走严格无损，q80为10，q≤60封顶20。有效范围、单调性与0/60/80/99/100锚点测试通过；不把JPEG/PNG的q100语义改为GIF规则。
- 有损通过双背景RMSE≤2.5/最差区间≤4/最大通道≤64/误差变化RMS≤3/最差跳变≤6后，还需比min(原图,有效无损候选)至少再省1%且1024 bytes。纯整数比较覆盖临界及u64范围。质量/收益不足各有回退原因，实际无损候选按Lossless报告；无损也无收益则NoGain，硬语义/工具/取消/超时不被成功回退掩盖。
- pipeline顺序执行两候选，比较预算另计存活的无损候选/复查候选容量；选定后释放未选候选，暂存、重新读取、独立复验再进共享输出层。默认准备总期限90秒，单次编码/比较仍30/10秒并受剩余期限收紧；文件I/O是协作检查，进入共享输出事务后遵循原取消/临界区契约，不声称可抢占I/O或提交后回报超时。
- 保留批量planning和ImageRequest::gif()有损拒绝。适配新核心字段的同时，v11转换对意外有损请求/处理报告返回InvalidSnapshot，契约测试覆盖，避免将有损结果标成无损。未修改DTO/v11协议、前端或生成类型。资源池及旧无损收紧公式维持，下一段开放批量前须同步新预算路径。

### 当前验证证据

- 定向比较器8项及GIF核心19项均通过，含进程回收、预览、质量选择/取消/绝对期限；默认核心现在运行比较器测试，无需实验特性。
- 最终真实核心验收target/gif-core-Uz2p0g/，日志target/gif-4b-core-final-261013.log：旧无损25收益/2NoGain、2项压力图、三格式混合通过；生产单文件q0/60/80/99/100、实际无损、有损、收益回退、质量回退、覆盖/副本/备份、各阶段取消、候选替换、源变化、目标冲突和比较资源限制通过。
- 暗部32阶固定噪声原图38,844 bytes，原生20候选RMSE约4.519、时域RMS约6.342、最差跳变约6.721；生产返回QualityBelowTarget回退与NoGain，原图保持。录屏28 bytes收益返回InsufficientGain/NoGain；照片q80/q60得到实际有损输出，q100无损。该噪声由项目生成，不引入新网络素材。
- 初次核心构建发现仅测试使用的validate_gif导入警告，已移到测试模块；未抑制lint。统一检查、最终v2实验与随包兼容验证继续收尾，GUI/MSI/有损观感和新SHA三平台不在本轮已验范围。

### 收尾复核

- 最终复核将纯无损/q100的无损等价检查保留在落盘后，避免新增一次重复全动画解码；只有需要比较有损收益时，提前验证无损基准。旧无损路径仍先验证源与最终文件，安全门槛未降低。因这处执行顺序变化，重新执行最终统一/真实核心/随包验证。
- 最终pnpm check退出0，日志target/gif-4b-check-final-261013.log：161项前端、63项核心单元、全部核心集成、118项桌面、2项doctest通过；格式、Oxlint、TypeScript、生成类型只读一致性、全部语料/来源哈希和全目标全特性Clippy通过。之后仅补充API注释与交接文档，不改变接口/执行逻辑。
- 最终222组v2实验通过，证据target/gif-lossy-dQthj8/、日志target/gif-4b-lossy-final-261013.log，包含2项Node契约测试。各强度2项明确透明拒绝；10/20各26无损收益、4质量候选、5NoGain，无新增非预期拒绝。HTML并排播放仅供观感核对，时间正确性仍以独立比较为准。
- 随包首轮target/gif-bundle-paQb7V/通过，包含v11、15次预览、三格式混合重试/备份/冲突/NoGain及能力独立降级。候选验证顺序调整后的最终核心/随包结果在下一条补记，旧结果不冒充最后代码结果。
- 最终真实核心与随包顺序复验均退出0：target/gif-core-3ejglx/、target/gif-bundle-QBEbnw/，日志分别为target/gif-4b-core-confirmed-261013.log和target/gif-4b-bundle-confirmed-261013.log。照片106,036 bytes在q80输出98,179、q60/q0输出93,623；q99因额外收益不足回退105,939、q100严格无损105,939。原图/候选/备份安全及三格式混合、v11/预览兼容均保持。
- 最终diff及9份变更文档的201个本地链接、NUL/尾随空白校验通过；新增Rust/脚本已经统一格式、lint与测试覆盖。未新增依赖或改锁文件；新素材仅为确定性生成器输出，已有公开原件未改。target报告、候选与工具构建继续由Git排除。
- 4B第一段本机交付完成并随本轮提交推送（见下节），未执行有损GUI/新MSI/人工观感/新SHA三平台。完整动画任务继续活动，下一入口为4B第二段批量/资源预算与4C/v12 DTO、生成TS和前端真实报告同批迁移，遵循前节验收。
- 本次相对前轮新增/调整范围：核心gif/comparison/*、gif/lab.rs、gif/quality.rs、gif/pipeline.rs及pipeline/tests.rs，GifRequest/Limits/Report和批量仅无损适配；桌面ipc转换及防误报契约；实验contracts与最小样本/真实有损验收模块、core-check入口；README/AGENTS/native与实验说明、索引/阶段计划/本连续记录。既有前端与生成DTO无修改，旧png-palettes仓库未修改。

## 2026-10-13 编写提交信息、提交与推送

- 用户要求根据工作区内容编写提交信息、提交并推送，无需执行其它任务。复核范围：43份文件（新增核心gif/comparison、gif/lab、gif/quality、gif/pipeline与两个example；桌面scan_diagnostics与诊断回归；实验contracts/lossy-check与gif-lossy语料；README/AGENTS/第三方说明/实验与native说明及devlog），无未暂存改动与未跟踪残留（target报告与.tools构建继续排除），git diff --check通过。
- 提交395a0ea「feat: 增加扫描拒绝分类日志与GIF保守有损单文件核心」（43 files changed、3243 insertions、114 deletions）。沙箱内.git只读，add/commit/push按规则提升同一条命令执行，未绕开沙箱约束；2a23aa5..395a0ea  dev -> dev已同步origin/dev，本地与远端一致。
- 沿用本轮已完成验证（pnpm check：161项前端、63项核心单元、118项桌面、2项doctest与全目标Clippy；222组v2实验与2项Node契约测试；gif:core:check与gif:bundle:check在候选验证顺序调整后复验；真实质量/收益回退、透明拒绝、取消/冲突/源变化与写后复验）；本次未改业务代码，不重复构建或测试。批量有损入口、4C/v12、有损GUI/新MSI观感与新SHA三平台仍未取得，不据此声明通过。
