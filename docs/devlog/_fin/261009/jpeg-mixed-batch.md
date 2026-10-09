# PNG/JPEG 混合批次核心与资源准入（J1b 第二段）

- 创建日期：2026-10-09（Asia/Shanghai）。
- 状态：完成，2026-10-09归档。业务681f050与交接7863652已推送，当前HEAD三平台应用检查/构建及JPEG完整回归全部通过；桌面JPEG/J2另行实施。
- 分支/基准：dev/c287f4e；本轮在保留阶段计划、索引、A0/J1a/J1b第一段归档与关联链接变更的基础上完成实现，已提交681f050（见末节）。
- 关联：[阶段计划](../../_plan/260930/next-development-plan.md)、[JPEG单文件核心](jpeg-lossy-core.md)、[PNG桌面连续记录](../../_plan/260922/png-batch-desktop.md)。

## 目标与验收

1. 同一BatchService、worker池和输出层处理真实PNG/JPEG；请求/报告/错误保留格式专属含义，可信宿主显式注入JPEG引擎，默认PNG-only保持。
2. 内容探测、扫描规划、执行能力一致；扫描有限读取且不解码像素/派生工具。JPEG扩展名规则沿用jpg/jpeg，PNG误命名.jpg契约保留。
3. JPEG按实际执行上限预约父进程缓冲、原生系数/像素、管道与固定进程开销；尺寸变大/换格式不能沿用旧预算，单图超总预算逐行失败。
4. 混合成功/失败/NoGain、逐行冲突/全部冲突Finished、原名/备份/结构、重试冻结/稳定ID和凭据隔离通过；取消/超时/关闭先真实回收再释放worker及预算。
5. 保持现有PNG桌面/生成wire契约；若必须改变wire，则同次同步协议/生成类型及消费者。JPEG桌面能力与随包工具仍属于J2。
6. 必要定向回归、真实混合核心入口、原JPEG/PNG回归、统一检查及正式构建；新SHA三平台在授权提交后验收，不冒充本机或历史CI结果。

## 2026-10-09 开工

- 已复核工作区及连续记录；上一轮归档/计划改动完整保留。开始定位BatchService、导入、快照/错误调用方以及JPEG实际内存与进程生命周期。
- 仅使用隔离生成样本/固定上游夹具，不启动GUI或处理用户原图，不修改png-palettes。

## 2026-10-09 授权实施

- 用户明确要求实施开发。复核dev/c287f4e及既有未提交文档，代码仍为PNG批次和独立JPEG核心；本轮开始实现完整混合批次。
- 共用请求保留路径/输出/模式/资源参数，格式专属选项、报告与错误使用显式枚举；可信引擎配置随扫描/请求传递并由服务核对。保留PNG单文件API和桌面v9契约，JPEG桌面部署仍留待J2。
- 先完成模型与调用方迁移，再落实JPEG预算和真实混合回归；验证结果随实施追加。

### 实现与定向验证

- 增加ImageRequest/FormatOptions、ImageReport/ImageError、ImportedImage/QualityMapping，批次行显式冻结格式；BatchParameters共用模式/资源并保留JPEG扫描数/超时，默认PNG-only。ImageEngines持有受信共享引擎，扫描清单与请求保留同一实例，服务拒绝换入另一实例。桌面内部转换同步迁移，v9专属PNG DTO保持，JPEG行/错误明确拒绝转换。
- JPEG导入在已有读量/条目/深度/取消约束下检查压缩结构，不派生工具；jpg/jpeg扩展名与PNG误命名.jpg契约保持。共用规划/输出与重试，JPEG拒绝PNG凭据许可。资源头探测最多2MiB，JPEG按8份输入、2份原生预算及32MiB固定余量预约；真实执行使用同一收紧上限。
- 全目标编译通过；核心原有回归与新增调度回归通过。新增门闩证明两种格式共享worker/预算、紧预算串行、超预算逐行Finished、源变大/换格式受限；真实阻塞子进程写出PID后再取消/超时/关闭，确认进程退出、临时目录回收、预约归零。
- 首次沙箱内cargo测试出现目录规范化告警，批次预检失败造成17个门闩/结果回归失败；正常权限下完整核心回归通过，未修改安全预检或放宽用例。cargo fmt/桌面资源构建与pnpm数据库访问亦受沙箱限制，按权限流程执行。
- 真实混合入口已接入jpeg:core:check。首轮原20项无损、92个有损组合/67个实际输出通过；混合重试用例因Windows规范路径与输入字符串不同而断言失败，改用原生目录身份与相对目标校验。随后独立隔离目录target/jpeg-mixed-c28ba449d2a84377ade9143612165535的真实混合入口通过，覆盖模式/回退、冲突/重试、备份/原始名称/结构、凭据隔离、能力/扫描上限与关闭。
- 待完成：最终统一检查、生成类型一致性、完整JPEG脚本独立复验和正式无安装包构建；随后核对diff/文档并交接。

### 最终验证进展

- 复核补齐JPEG非法原配置检查：必须先校验再收紧，不能把超出合法域的上限修正为有效参数；新增PNG继续执行/JPEG逐行拒绝回归。标记前缀即使全为填充字节也最多读2MiB。
- pnpm types:generate成功，两份生成TS均无diff；最终pnpm check通过155项前端、43项核心单元、全部Rust集成回归、107项桌面、2项编译型doctest、34份PNG语料及类型一致性/全目标全特性Clippy。首轮统一检查仅报新增测试的err().expect与两处clone切片写法，已修正并完整复验，无lint豁免。日志target/j1b-mixed-check-261009-final.log。
- 最终pnpm jpeg:core:check通过：原20项无损（14有收益）、92个有损/回退组合与67个实际输出，加上真实混合批次完整入口；4个混合JPEG产物完成独立djpeg解码、无损系数及元数据复验。证据target/jpeg-lab-core-yinnBn/core-report.json与mixed-results/checks.json，helper SHA256仍为f9e810c25a86369d9479b4f67bd0256c94c8a768d756208f8b7bd81a39340f56。日志target/j1b-mixed-jpeg-261009-final.log。
- 正式无安装包构建进行中，Vite生产构建已通过，等待Rust release链接；尚未启动GUI。文档首轮diff检查与7份文档106个本地链接/NUL检查通过。

### 最终交付与下一入口

- pnpm tauri build --no-bundle --ci成功，Rust release构建用时1m56s；日志target/j1b-mixed-desktop-build-261009.log。target/release/pixofold.exe为0.1.0、10,772,480 bytes，构建时间2026-10-09 10:54:13（Asia/Shanghai），SHA256 1BDABD423BA11279891915A1503300B6D757B919C17336A0DE5C395A2A20F991。未启动GUI或生成安装包，helper尚未随桌面部署，工作台仍只开放PNG。
- 最终cargo fmt --all -- --check与git diff --check通过，7份文档108个本地链接/NUL检查通过；生成TS及两个锁文件无diff。新增业务模块batch/image、混合调度/生命周期回归batch/mixed_tests和真实入口examples/jpeg_mixed_check；批次/导入/预算、桌面内部转换与现有调用方、JPEG脚本及相关文档同步修改。改动已提交为681f050并推送origin/dev（见下节），未修改参考仓库或用户原图。
- 平台边界：本轮已验证Windows本机自动回归、真实工具和无安装包构建；尚未取得新SHA的macOS/Ubuntu CI证据，历史c287f4e全绿不覆盖这些改动。提交推送后按新SHA核对三平台应用及JPEG完整core-check，满足后归档本任务。
- 下一开发入口为J2第一步：固定helper构建与身份清单、可信随包加载、无.tools/额外PATH运行，并进行Windows安装/启动小试验；随后再演进实际能力、协议/生成类型、混合导入/结果/重试和方向缩略图。PNG原生A1继续合并用户手测；大图/摄影观感、RSS和其他平台安装仍需各自证据。

## 2026-10-09 编写提交信息、提交与推送

- 用户要求根据工作区内容编写提交信息、提交并推送，无需执行其它任务。复核范围：45份文件（含A0/J1a/J1b第一段三份记录从_plan迁至_fin/261009、新增jpeg-mixed-batch计划、batch/image与mixed_tests、jpeg_mixed_check入口；其余为批次/导入/预算、桌面内部转换、JPEG脚本与文档），无未暂存改动与未跟踪残留，git diff --check通过。
- 提交681f050「feat: 支持PNG/JPEG混合批次与共用资源准入」（45 files changed、2163 insertions、230 deletions）。沙箱内.git只读，add/commit/push按规则提升同一条命令执行，未绕开沙箱约束；c287f4e..681f050  dev -> dev已同步origin/dev，本地与远端一致。
- 沿用本轮已完成验证（pnpm check：155项前端、43项核心单元与完整Rust回归含107项桌面/2项doctest、34份语料与类型一致性；jpeg:core:check无损20项、有损92组合/67输出及真实混合入口与独立复验；10:54:13正式无安装包构建）；本次未改业务代码，不重复构建或测试。推送按push触发CI，三平台应用与JPEG完整core-check结果须以最终HEAD取得，不把本次推送写成已通过。

## 2026-10-09 下一阶段计划复核时的平台进展

- 本机HEAD和GitHub只读branch查询均为786365246c203cbdf0b9714f571600080b143d67，包含业务681f050；开工工作区干净。此前摘要中的“未提交”已过时，以实际Git状态为准。
- 对应[CI run37878230376](https://github.com/cct124/pixofold/actions/runs/37878230376)于2026-10-09 11:12:49（Asia/Shanghai）触发；截至本轮11:20查询整体仍在运行。三平台JPEG jobs的固定工具构建、实验和真实核心/混合批次步骤均success；macOS/Ubuntu统一检查已通过并进入桌面构建，Windows统一检查仍在执行。

| 平台 | 应用Check/构建 | JPEG实验与真实核心 |
| --- | --- | --- |
| Windows | job113651694066，Check运行中、构建待执行 | job113651693846，success，11:15:46完成 |
| macOS | job113651694014，统一检查success、构建运行中 | job113651694198，success，11:14:25完成 |
| Ubuntu 24.04 | job113651694128，统一检查success、构建运行中 | job113651694053，success，11:14:12完成 |

- 当前证据已覆盖新代码三平台JPEG真实核心，但尚不能将整轮CI或应用构建写为通过。待同一HEAD三个Check jobs及构建完成后归档；若失败，先读失败步骤再决定修复范围。未重跑CI或本机测试，未修改业务代码。
- 下一开发交付按[阶段计划末节](../../_plan/260930/next-development-plan.md)推进J2可信helper随包与隔离运行，随后接桌面混合工作流；GUI、安装、照片/大图观感和RSS仍保留各自验收边界。

## 2026-10-09 最终平台收口与归档

- 用户授权J2实施时只读复核同一HEAD：run37878230376六个jobs全部success。应用job113651694128（Ubuntu）11:20:53、113651694014（macOS）11:22:42、113651694066（Windows）11:27:58完成，时间均为Asia/Shanghai；统一检查和正式无安装包构建均通过。JPEG三平台完整真实核心包含混合入口，结果见上节。
- J1b第二段目标满足，按实际日期移至_fin/261009，保留创建日期与此前进行中记录。本次没有重跑远端CI；新J2改动的平台结果另验收，当前全绿不覆盖尚未提交的随包实现。
