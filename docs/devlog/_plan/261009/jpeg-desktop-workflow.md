# PNG/JPEG桌面混合工作流（J2第二段）

- 创建日期：2026-10-09（Asia/Shanghai）。
- 状态：阶段1预算修复5f68448与提交记录94dd86c已同步origin/dev，94dd86c的六个CI jobs全部success，代码/照片大图/工作集/MSI资源及新SHA平台门槛完成。用户确认此前功能正常及卸载，缺少WebView2的环境按其安排后置。新修复包实际安装/GUI、细分原生/视觉与无开发工具环境证据继续按实际范围记录，完整J2任务保持活动；下一主要开发为GIF无损实验。
- 分支/基准：dev/d5bf44cf35579f1b4877954b2acca3200956b48f，已推送origin/dev。接手时保留J2第一段的提交结果补记；本轮先同步第一段和索引的真实CI进展。
- 关联：[阶段计划](../260930/next-development-plan.md)、[可信工具随包及P0](jpeg-desktop-bundle.md)、[PNG原生收尾](../260922/png-batch-desktop.md)。

## 目标与验收

1. 将应用验证的同一ImageEngines注入扫描、规划、BatchService和预览，保持唯一图片任务池及安全退出；工具不可用时保留PNG能力。
2. 协议v10提供实际格式能力、候选/任务格式和格式专属报告/错误；生成TS、运行时校验、中英文展示同步演进，旧页面要求更新重载。
3. 原生过滤、受控导入/拖放、Ready启动、冻结参数/目标、普通重试、结果/目录/备份定位保持原授权契约；JPEG保护拒绝不开放PNG caBX移除许可。
4. JPEG缩略图保持单解码、输入/像素/解码/输出/缓存限额和世代失效；应用Exif方向1–8，明确ICC/CMYK边界；不通过预览开放原始路径或元数据。
5. 有效PNG回归、格式契约、错误恢复、方向与资源边界回归、真实混合后端验收、pnpm check和正式构建通过。GUI/安装/视觉/照片观感/RSS及新SHA三平台分别记录实际证据。

## 2026-10-09 代码探索与实施顺序

- P0 run37895466410已取得三平台JPEG与统一检查通过；macOS/Ubuntu整个Check job已success，Windows桌面构建仍在执行。完成平台基线后开始本段代码。用户自行完成现有MSI的P1手测，尚无反馈。
- 启动顺序：锁定Tauri 2.11.5的app.rs在setup hook前创建配置中create=true的窗口。拟关闭自动创建主窗口，由setup先加载引擎、建立任务/订阅/预览所有者，再从原窗口配置创建main，保证首次页面/IPC早于服务初始化的竞争被消除。
- 引擎贯通：TaskRuntime增加显式引擎构造，NativeImport使用scan_with_engines，BatchService使用with_engines。默认构造仍PNG-only供无工具检查；实际能力由已注入引擎产生，不读取plannedFormats。
- DTO按format区分PNG/JPEG处理报告，保留各自回退原因与质量参数；共用mode的输入形状沿用既有无损/有损和0–100范围，数值含义由各格式映射决定。JPEG工具错误/超时及嵌套文件恢复完整转换；分页仍保持同revision。
- 缩略图拟复用可信helper现有pixels操作与进程回收，不引入第二套JPEG库或修改helper配方。严格解析输入/颜色边界，受限像素在Rust缩放及应用方向后只编码小PNG；ICC/四分量或不明确颜色保留不可预览状态，压缩仍按各自策略处理。
- 验证先覆盖纯转换/资源/方向与现有PNG消费者，再扩展显式无GUI部署验收，使真实共享引擎经过应用扫描/任务/DTO/定位/预览；原生窗口投递、安装和视觉效果由用户手测补证。

## 后续入口与未完成项

- 本记录创建于P0最终CI等待期间；基线完成后开始功能代码/协议/生成类型迁移，入口为src-tauri/src/lib.rs、tasks、ipc、assets及workspace。
- 完成后同步README/AGENTS的实际边界、生成类型和本连续记录；未经新的明确授权不提交本段功能改动。

## 实现与首轮验证

- TaskRuntime::with_engines把同一引擎实例注入NativeImport/BatchService及AssetService，快照返回supportedFormats。main禁用自动创建，setup先初始化服务后按原配置建窗；原生文件过滤按实际能力提供PNG或PNG/JPEG。
- 协议升级v10，候选/任务显式携带format；processing为按format区分的PNG/JPEG联合，JPEG独立呈现原生编码质量、四类保守无损回退与工具错误。APP11保护拒绝使用独立wire错误，避免提示PNG移除流程。JPEG进程/文件嵌套恢复信息及真实备份定位已接通。
- 缩略图复用原有可信helper的pixels操作（未改helper/构建配方）；实际输入、像素、工作集与5秒单次期限受限，Exif方向在共用小图采样时应用，不分配第二份全尺寸旋转图。AssetService的世代撤销同时取消在途JPEG，真实返回前仍占解码许可，退出等待回收。
- 前端候选/任务格式、质量含义、回退/工具/保护错误和可用格式说明已同步中英文；新增运行时能力/格式/报告一致性检查，生成类型已运行。首轮前端检查定位到一组旧mock快照漏写supportedFormats，补齐后159项前端测试全部通过，类型检查通过。
- 首轮桌面回归112项通过，包含方向1–8、预览取消/许可占用、JPEG专属报告/错误与既有PNG原生授权/输出/订阅回归。新代码尚未完成统一检查、真实部署验收或正式构建；不沿用P0绿色为本段背书。
- 扩展无GUI部署检查复用真实应用任务、DTO与AssetService：混合Ready启动、单行及全部冲突、固定目标重试、备份、NoGain、方向/颜色边界、目录分组和退出，另由djpeg/coeffdump复验产物。当前正在构建/执行，日志target/j2-desktop-bundle-261009.log，结果随后补记。

## 本机最终验证与交付

- `pnpm check`通过：159项前端、44项核心单元、全部Rust集成回归、113项桌面及2项doctest；格式、Oxlint、TypeScript、生成类型只读一致性、34份语料及全目标/全特性Clippy均通过。日志target/j2-desktop-check-261009.log。
- `pnpm jpeg:core:check`通过：20项无损（14项有收益）、92个有损/回退组合（67个实际输出）及真实混合扫描/模式/冲突/重试/备份/原名/结构/凭据隔离/关闭回收；djpeg/coeffdump/标记独立复验通过。证据target/jpeg-lab-core-QerXrz，日志target/j2-desktop-jpeg-core-261009.log。
- `pnpm jpeg:bundle:check`通过，证据target/jpeg-bundle-Wr09TB；桌面部分产生13个输出（12 JPEG、1 PNG）和11份真实预览，混合Ready、冻结目标重试、旧attempt拒绝、NoGain、全部冲突、备份原字节、目录分组和关闭均通过。另补充嵌入缩略图不能遮蔽矛盾颜色标记的预览拒绝回归，避免把有损回退的优先原因误当成全部颜色检查。
- `pnpm tauri build --bundles msi --ci`通过，前端生产包及Windows正式程序/MSI已生成，日志target/j2-desktop-msi-261009.log。helper仍为原固定配方/哈希，无依赖或许可变更。未启动应用GUI或安装/卸载应用。
- 新MSI为4,063,232 bytes（2026-10-09 15:45:51），SHA256 `99E8FC449F2FA3CD3A3F9F7E69518E9C9BE19ED934146D6253CE41829DC4E5AE`。稳定交付位置：`target/deliverables/261009-jpeg-v10/PixoFold_0.1.0_x64_en-US.msi`。构建后本机EXE为10,788,352 bytes，SHA256 `518D1C21F62A14F86D9D20EC91F2D8D0D2869ACAD4338A123EE96A2B55F5C0FA`，仅记录构建产物身份，不作为已安装启动证据。
- 新MSI已静默管理解包到target/j2-workflow-msi-extracted-261009：主程序、helper、身份清单、许可/IJG致谢及说明均在包内；无开发检查程序或实验编码器。使用该实际包内资源执行`pnpm jpeg:bundle:check --resources E:/Project/pixofold/target/j2-workflow-msi-extracted-261009/PFiles/PixoFold`再次通过，证据target/jpeg-bundle-GIi5SG，日志target/j2-desktop-msi-resources-261009.log。此项是解包/后端部署证据，不是GUI安装验收。
- 最后diff审阅将测试夹具中的不可见控制字符改成等价可读字节数组，定向颜色标记用例及rustfmt检查通过；所有改动文本的NUL与本地文档链接检查通过。生成TS只从Rust来源生成，Cargo.lock/pnpm-lock.yaml无变更。
- 未提交改动包含：核心共享引擎/预览和恢复解析，桌面启动/任务/IPC/assets/验收程序，生成TS及前端格式校验/展示/回归，README/AGENTS/native说明与本轮devlog。P0提交授权针对当时6份文件；本段功能须新授权后再提交推送并核对对应SHA的CI。
- 本机最终审阅为44份文件，git diff --check、44份文本NUL检查及135个本地文档链接通过；锁文件与核心既有generated.ts无diff。已提供工作区审阅入口并请求本轮J2提交推送授权，当前等待用户答复；尚未执行本段Git写入或推送。
- 用户曾回复“先保留本机改动，等我手测”，本段44份改动先保留在dev工作区；随后用户要求按工作区内容提交推送，改动已提交为a21d330并同步origin/dev（见下节）。下一入口为用户提供新MSI手测反馈，有问题优先修复。

## 用户MSI手测入口与剩余工作

- 用户已选择自行安装/启动/卸载。新包可合并完成P1的PNG安装烟测及J2混合验收；旧PNG/v9包保存在target/deliverables/261009-png-v9/，原哈希不变。两者均为开发版0.1.0，若旧版已安装，先卸载再测试新包，记录实际安装包哈希/系统/WebView2。
- 手测样本目录`target/deliverables/261009-jpeg-v10/samples`包含自生成a.png、b.JPEG、exif-6.jpg、icc.jpg及protected.jpg。先用前三项测试原生文件选择/拖入、混合有损/无损、另存到空目录、格式/大小/真实结果查看及方向预览；所有样本均来自隔离自动验收，无私人图片。
- icc.jpg在有损模式应显示无损回退，缩略图不可用是颜色管理边界；protected.jpg应保留原图并显示保护拒绝，不出现JPEG凭据移除入口。再按需要验证备份/冲突重试、批次目录、页面重载不重跑、正常退出与卸载；检查诊断日志jpeg_engine_verified。
- 亮暗主题、中英文、DPI/窗口适配及原生OS投递尚未做GUI复测；测试中的DOM/mock IPC和真实无GUI后端不代表这些项目通过。照片/大图观感及RSS抽测仍待独立记录；新SHA三平台CI按本轮推送后的HEAD取得。

## 2026-10-09 编写提交信息、提交与推送

- 用户要求根据工作区内容编写提交信息、提交并推送，无需执行其它任务（此前“保留本机改动”的选择被本轮指令替代）。复核范围：44份文件（新增image-contract.ts、assets/jpeg.rs、bundle_check.rs与本段计划；其余为核心共享引擎与预览、桌面启动/任务/IPC/assets/验收程序、生成TS与前端展示/回归、README/AGENTS/native说明），无未暂存改动与未跟踪残留，git diff --check通过。
- 提交a21d330「feat: 桌面接通PNG/JPEG混合工作流与方向预览（协议v10）」（44 files changed、1617 insertions、182 deletions）。沙箱内.git只读，add/commit/push按规则提升同一条命令执行，未绕开沙箱约束；d5bf44c..a21d330  dev -> dev已同步origin/dev，本地与远端一致。
- 沿用本轮已完成验证（pnpm check：159项前端、44项核心单元、113项桌面、2项doctest、34份语料与全目标Clippy；jpeg:core:check无损20项/有损92组合67输出/混合入口；jpeg:bundle:check真实应用混合任务、13个输出与11份预览及MSI解包资源复验；15:45:51新MSI）；本次未改业务代码，不重复构建或测试。真实GUI安装/启动/卸载、照片大图观感、RSS与新SHA三平台结果仍待验收，不把本机与历史结果写成新代码通过。

## 2026-10-09 用户手测反馈与运行日志检查

- 用户反馈“手动验证后功能正常”，并授权检查相关运行日志。开工HEAD为dev/3757d4b（业务a21d330），工作区干净；当前任务为只读日志核对和本连续记录维护，不提交推送。用户未逐项说明安装/卸载、主题/DPI及照片观感覆盖范围，不自动替其补全验收矩阵。
- 已核对diagnostics实际配置：开发日志位于仓库logs/，release日志位于用户系统临时目录的pixofold-logs/。沙箱进程的临时目录与用户安装程序不同，已定位到真实用户的release目录；最近两份日志在16:03–16:11，早前开发/旧release会话另行识别。接下来解析JSONL、按会话检查告警/失败及启动、任务终态和退出健康统计。

### 日志结果（时间均为Asia/Shanghai）

- 本次主要检查两份release/0.1.0日志：`run-1791532992388-uws1AHfhPpyq.jsonl`（16:03:12–16:11:21，58行，SHA256 D37964A90E722CBA7A27C94F5DD3ECB49F9D5B88B0F3B4C3747ED11DA2579997）及`run-1791533484893-KG5K7VdPJ1It.jsonl`（16:11:24–16:11:29，6行，SHA256 5637395495EB413A38939AC38E0606CD923FD83C73B6CBD7631426D0BB067E71）。64行JSON均有效，带sequence的事件无缺号/重复；WARN/ERROR均为0，但不据此忽略INFO内的失败结果。
- 安装目录中的主程序SHA256为45808AEB423D2B29CD652A81A693EBB7F90E20F09B297650588C1D2D1CD91F2E，与此前新MSI实际解包的主程序一致；安装目录helper为8FC117B4851AE02D405995361EDDCCEE646B3307A7584EF15FC62FE1D1ACFD60，与固定可信资源一致。主程序包内哈希与构建后还原bundle标记的EXE哈希不同，分别比较相同产物，不混用。
- 两次启动均记录jpeg_engine_verified。第一会话两个批次分别3/3和1/1成功，四个任务各恰好有一次job_started/job_finished、一次output_commit_started/output_commit_succeeded及Reading→Optimizing→Validating→BeforeCommit阶段；无失败、取消或未闭合任务。未出现ToolIdentity/ToolIo/ToolExit/TimedOut、WorkerPanicked/ServiceFault、CommitFailed/CleanupFailed、application_start_failed或shutdown_failed。
- 4次thumbnail请求中3次ok，1次在16:10:16.854返回`Some(Unavailable)`（selection=2、job=1、attempt=1）；对应压缩已成功，输入14,124 bytes、输出13,245 bytes，与交付自生成icc.jpg的原图/真实输出大小完全相符。源码对ICC/CMYK/不明确颜色、保护元数据或工具身份失败均可返回Unavailable，因此日志不能单独证明具体分支；结合已验证引擎、成功任务及手测样本，判断与当前ICC预览边界吻合，未发现压缩故障，保留该项而不宣称“所有请求均成功”。
- 4次扫描均Complete，可处理数量依次3、1、0、0；后两次在16:10:52和16:10:59未启动批次。现有日志仅记录候选数量和完成状态，未记录逐项扫描拒绝原因，不能仅凭日志断言是protected.jpg或其他格式拒绝。若用户当时使用了保护/不支持样本则属预期；用户本次整体反馈功能正常，当前没有新增缺陷证据。
- 两次退出均有application_stopping(exit_code=0)及session_finished，dropped_events=0、write_failures=0；检查时无pixofold或pixofold-jpeg-helper残留进程。该证据确认这两次实际运行安全收尾，不扩展为所有崩溃/OS强杀场景保证。
- 辅助检查全部6份保留的release日志，均可解析并有正常结束/健康统计；较早14:43–14:52会话有6条UnsupportedContentCredentials警告，属于PNG凭据保护拒绝。其中首批确认3张后最终6/6成功；第二批仍保留3张待确认行，退出正常。这些旧会话不计为本次v10运行失败。
- 结论：用户手测反馈与运行证据一致，未发现需要修改业务代码的明确故障。只更新本记录、阶段索引与J2第一段反馈状态；未执行GUI、安装/卸载、业务修改、测试重跑或提交推送。卸载、无开发工具系统及主题/DPI等矩阵不能由上述日志代替逐项确认；本次未查询远端CI，不改变其待核对状态。
- 文档交付检查：git diff --check通过，3份变更文档的31个本地链接及NUL检查通过。HEAD和本地origin/dev均为3757d4b，本轮仅3份开发记录处于未提交状态。

## 2026-10-09 手测记录入库后的平台核对与下一阶段规划

- 用户询问下一步合理开发计划。开工dev/0f4fcfd0825f51542d5d2658da600d21e0174350，本地origin/dev及GitHub分支查询一致，工作区干净；0f4fcfd仅将前轮3份手测/日志开发记录入库，实际业务仍为a21d330。旧交接中的3757d4b和未提交状态已成为历史。
- 当前SHA的[CI run37905198950](https://github.com/cct124/pixofold/actions/runs/37905198950)于2026-10-09 16:28:50（Asia/Shanghai）触发。16:32核对：Ubuntu JPEG job113736882266及macOS JPEG job113736882345已success；Windows JPEG job113736882052在真实核心回归步骤，三平台Check jobs113736881799/113736881999/113736882083在统一检查步骤，均in_progress。已完成步骤无failure；不能据此宣称六个jobs全绿。本轮仅只读查询，没有重跑或Git写入。
- 16:36补查：Windows JPEG job也已success，JPEG三平台全部通过；macOS/Ubuntu Check统一检查通过、桌面构建进行中，Windows Check仍在统一检查。六个jobs已完成步骤均无failure，整体仍未结束。
- 下一入口：继续按该SHA核对三平台统一检查、桌面构建和随包部署；明确失败优先处理。收到的功能正常反馈与运行日志已记入本段，卸载、无开发工具系统、主题/DPI、照片观感及RSS仍按实际范围补证。完整门槛满足前本段保持活动。
- 下一主要开发任务为[动画验证底座与GIF最小实验](animation-foundation.md)：先独立建立有界合成/时间轴验证与确定性语料，再试固定源码Gifsicle；首个产品交付聚焦GIF无损核心及桌面闭环。动画实验可在平台等待期间开展，生产格式开放和阶段收口须使用自己的验证结果。

## 2026-10-09 实施阶段1：JPEG收尾与照片/大图资源验收

- 用户要求实施上一轮计划的阶段1（JPEG收尾）。开工dev/0f4fcfd，保留前轮4份本机规划/平台文档改动，不进入动画实现。本连续记录承接平台、手测与照片/资源证据。
- 本轮GitHub只读复核确认run37905198950对应0f4fcfd0825f51542d5d2658da600d21e0174350，2026-10-09 16:46:37（Asia/Shanghai）completed/success；三平台Check jobs113736881799/113736881999/113736882083均success，统一检查、桌面构建及随包部署关键步骤通过；三平台JPEG jobs113736882052/113736882266/113736882345均success，真实核心步骤通过。未重跑或用旧SHA替代。
- 用户补充“已卸载，未安装webview2的测试以后再测，先完善功能”。真实卸载已按用户反馈记为通过；缺少WebView2的目标环境按用户安排后置，无开发工具隔离系统仍无独立证据。继续本轮可自动完成的照片/大图/内存验收，已有功能正常反馈不重复索取。
- 已定位现有真实核心、随包后端和独立djpeg/coeffdump入口；上游227×149照片仅提供最小真实内容观察，尚不足覆盖大照片。下一步准备来源/许可明确的隔离照片和大图，使用实际核心/桌面调度验证输出、原图安全、资源预算及采样工作集，观感与性能结论按样本范围记录。
- 新增显式jpeg:acceptance:check及bundle-check的--profile路径，复用resources::task_config和TaskRuntime；50ms目标间隔采样父进程/直接helper工作集、活动数和预约，报告实际最大采样间隔；不含WebView/GUI。调用方须提供隔离语料，输出全新目录；独立djpeg/coeffdump复验，原图哈希、NoGain/失败无副本和完整反馈检查，预期拒绝由显式JSON清单限定。
- 隔离语料来自许可明确的Fronalpstock照片（Daniel Schwen，CC BY-SA 3.0）及Times Square照片（Jeffrey Zeldman，CC BY-SA 4.0），保留原图哈希、来源、许可与准备脚本；另含固定上游rose及PNG。原始相机元数据与独立归一化派生图分别测试；原图不接受不能写成“原照片直接支持”。数据及许可证据位于target/jpeg-stage1-261009，未提交第三方图片。
- 首轮证据target/jpeg-acceptance-cWKx69：无损2成功/4NoGain、2扫描拒绝；有损80中4项失败，其中两张约2MP为ToolExit、两张12MP为ResourceLimit。直接调用固定helper复现：city-small在24/32MiB时pixels成功而lossy退出2，64/128MiB时lossy成功；不是无收益或坏图。诊断结果target/jpeg-stage1-261009/helper-diagnose.json。
- 根因已核对固定MozJPEG 6c9f0897的jccoefct：Huffman完整缓冲无条件申请whole_image和whole_image_uq两套编码系数，helper将扣除像素余量后的预算各半分给解码/编码；旧头探测/执行检查只覆盖两套总系数，收紧限额后编码半份不足。修正格式预检的有损工作集检查与头探测同源计费，保守覆盖四套系数及元数据/像素余量；在调用原生编码前返回ResourceLimit。helper/配方/哈希和默认128MiB上限保持原策略。
- 增加1600×1200真实自生成JPEG混合批次回归：默认预算必须成功提交真实有损结果，32MiB必须明确ResourceLimit且无输出/原图不变；独立JPEG输出验证接入现有jpeg:core:check。照片验收另加入6.6/7.1MP可处理大图，12MP的有损资源拒绝作为显式边界保留，无损另验证。当前修复待真实回归和完整检查，新CI绿色仅覆盖开工SHA。
- 修复后照片证据target/jpeg-acceptance-DZVdNS：10项输入每种模式均有完整任务/扫描反馈。无损2成功/6NoGain；有损40/80各6成功，100为1成功/5NoGain；两张12MP在有损下为显式ResourceLimit、无损正常终态；两个原始相机样本因复杂元数据扫描拒绝。默认80的4张归一化照片减少47.8%–49.8%，独立RGB解码PSNR36.48–40.03dB；不作为通用压缩率或画质承诺。全部源哈希不变，NoGain/拒绝无副本，未见ToolExit/Validation/清理故障。
- 并发抽测为16张6.6/7.1MP JPEG副本及1张PNG，17/17成功，处理2625ms；实际桌面配置17 workers、约2.13GiB预算，采样峰值3个活动worker/3个helper，宿主+helper同时采样峰值133.20MiB（各自峰值19.57/117.74MiB不能相加代替同时峰值）。目标间隔50ms、最大实际间隔109ms，机器AMD Ryzen 9 7945HX/32逻辑CPU/约31.7GiB总RAM；这是无GUI后端抽测，不含WebView且不是硬RSS上限。
- 已生成并查看city-small-comparison.png和landscape-small-comparison.png，包含输入、80、40的全图缩放与100%同位置裁切。抽样未见明显整体色偏、几何变化或破损，40档可见纹理/文字边缘损失；视觉结论限于这两张归一化场景，真实ICC校准/广泛照片观感仍不作扩展。对照图与来源/许可/脚本保存在target/jpeg-stage1-261009。
- 新2MP回归初次遇到新增调用签名错误，修正后又被测试辅助服务512MiB总预算提前拒绝；已按目标将该回归固定为1worker/1GiB总预算，保持每图128MiB与32MiB两种检查，并要求32MiB错误原因来自JPEG工作集而非批次预算。完整jpeg:core:check现已通过（20无损/14收益、92有损组合/67输出、新2MP成功与超限拒绝及完整混合入口），证据target/jpeg-lab-core-noLvxz、日志target/jpeg-stage1-261009/core-fixed.log。当前进行pnpm check，随后构建包含修复的新MSI。
- 完整检查已收口：pnpm check的格式/Oxlint/TypeScript、159项前端、34语料、生成类型通过；Clippy首次发现显式desktop测试装配未重导出resources，补齐测试根引用后续跑pnpm rust:check/rust:test及rustfmt，全目标/全特性Clippy、44核心单元、全部核心集成、113桌面和2 doctest全部通过。未重跑未受影响的前端；日志check-all.log、clippy-fixed.log及rust-fixed.log均在本轮证据目录。
- 新MSI于17:53:33构建完成，保存target/deliverables/261009-jpeg-budget/PixoFold_0.1.0_x64_en-US.msi（4,063,232 bytes，SHA256 B3BEACF31D536841A3D406C539E94FFB447D9AC25D0EB3898C8A83FADE9915EF）。仍为开发版0.1.0，旧v10包保留原路径/哈希，不能混用安装证据。新MSI管理解包退出0，包内主程序SHA256 878E7857FCD09704F967C972E37652A9D87F879D678A9A488DBAF87F0F6BD615，helper仍为8FC117B4851AE02D405995361EDDCCEE646B3307A7584EF15FC62FE1D1ACFD60；含许可/来源及身份清单，未包含实验编码器、验收程序或照片。
- 对该实际新包内资源执行jpeg:bundle:check通过，证据target/jpeg-bundle-ICwqrH及bundle-msi-fixed.log，真实混合后端13输出/11预览、工具缺失/伪造身份拒绝和独立输出检查通过。此项为解包/无GUI部署，新包安装/启动及完整OS视觉矩阵仍按用户实际手测反馈另记；未自行安装、启动或卸载程序。
- 最终交接：开工基准为0f4fcfd；本轮保留前轮4份规划改动并纳入18份完整审阅范围，无依赖/锁文件/生成类型或helper配方变更。git diff --check、148个本地链接、8份Markdown及18份变更文本NUL/尾随空白检查通过。改动随后按用户指令提交为5f68448并推送origin/dev（见下节）。新SHA三平台结果尚未取得，不能用0f4fcfd绿色覆盖本轮修复。

## 2026-10-09 编写提交信息、提交与推送

- 用户要求根据工作区内容编写提交信息、提交并推送，无需执行其它任务。复核范围：18份文件（新增jpeg-acceptance-check.mjs、bundle_check/profile.rs与animation-foundation计划；其余为jpeg/preflight与mixed回归、bundle-check程序、package.json/README、native与实验说明、devlog索引与J2记录），无未暂存改动与未跟踪残留（照片等隔离语料留在target不入库），git diff --check通过。
- 提交5f68448「fix: 收紧有损系数预算并补照片大图验收」（18 files changed、698 insertions、15 deletions）。沙箱内.git只读，add/commit/push按规则提升同一条命令执行，未绕开沙箱约束；0f4fcfd..5f68448  dev -> dev已同步origin/dev，本地与远端一致。
- 沿用本轮已完成验证（pnpm check：159项前端、44项核心单元、全部核心集成、113项桌面与2项doctest、34份语料及全目标Clippy；jpeg:core:check含新2MP成功与超限拒绝；jpeg:acceptance:check照片/大图与17张并发抽测；17:53:33新MSI及包内资源jpeg:bundle:check复验）；本次未改业务代码，不重复构建或测试。真实安装/视觉矩阵、无开发工具环境与新SHA三平台结果仍待验收，不据此声明通过。

## 2026-10-10 新SHA平台收口与下一开发入口

- 用户询问下一步合理开发计划。开工dev/94dd86c28bd98796f89a0c721e8a7577a67113c3，工作区干净；本地origin/dev和GitHub分支只读查询一致。git祖先核对确认包含业务5f684484c877afd84b8ad4a27ef3eccf10721dec，94dd86c仅补前轮提交记录；旧18份未提交/等待平台状态已成为历史。
- 本日约10:45（Asia/Shanghai）只读核对[CI run37917962797](https://github.com/cct124/pixofold/actions/runs/37917962797)，head_sha为94dd86c，2026-10-09 18:29:57创建、18:48:47 completed/success，六个jobs全部success。Check macOS/Ubuntu/Windows jobs113778790283/113778790691/113778790909于18:42:24/18:39:23/18:48:46完成，统一检查、桌面构建及随包部署关键步骤全绿；JPEG Windows/macOS/Ubuntu jobs113778790556/113778790597/113778791143于18:33:01/18:32:02/18:31:18完成，固定工具、实验和真实核心步骤全绿。本次没有重跑或用0f4fcfd结果替代。
- 阶段1代码和平台门槛已收口；新MSI包内资源部署与照片/资源抽测沿用前节证据，不重复测试/构建。用户确认的原版功能正常/卸载保持原范围；新修复包实际GUI安装、无开发工具系统、主题/DPI等欠项单列，未安装WebView2的环境按用户安排后置，不能由CI推断已通过。
- 下一主要开发按[动画任务本日细化](animation-foundation.md)首批实施：GIF可复现语料→有界独立合成/时间轴/循环验证→固定Gifsicle源码构建及无损实验；APNG完整实现及有损另分批。JPEG新增原图安全/授权/退出等明确故障优先处理，其他已后置验收不阻塞独立GIF实验。
- 本轮仅平台复核和连续计划/索引维护，未实现新格式、安装依赖、运行GUI/测试/构建或提交推送。
