# PNG/JPEG桌面混合工作流（J2第二段）

- 创建日期：2026-10-09（Asia/Shanghai）。
- 状态：代码与本机自动验收/正式MSI完成，用户随后要求提交推送，本段已提交a21d330并推送origin/dev（d5bf44c..a21d330）；等待手测反馈，新SHA三平台及照片/大图/RSS仍待后续验收，保持活动。P0 d5bf44c的六个CI jobs已于2026-10-09 15:07:21全部通过。
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
