# JPEG 受控单文件无损核心（J1a）

- 创建日期：2026-09-30（Asia/Shanghai）。
- 状态：J1a已随ff0a837入库；2026-10-08取得a2031e7三平台JPEG真实核心通过的结果。当前Windows应用失败另由A0定位修复；本轮新增J1b第一段保守有损，详见关联新任务，批次/桌面仍未开放JPEG。
- 分支与代码基准：dev/7925c48；本轮在保留前轮计划、索引、A0记录及J0归档/链接变更的基础上完成实现，已提交ff0a837（见末节）。
- 关联：[阶段计划](next-development-plan.md)、[Windows基线](png-native-name-portability.md)、[已完成J0](../../_fin/260930/jpeg-engine-lab.md)。

## 目标与验收

1. 先复现Windows干净检出的格式问题并最小修复；不降低检查标准，不宣称未执行的新SHA远端CI通过。
2. 固定J0的MozJPEG源码，新增后端自有的无损工具适配；绝对工具位置/可信哈希、不使用PATH或shell，有界管道、超时/取消、kill和wait及临时资源清理。编码器只处理字节，不获得用户文件路径。
3. JPEG专属请求/报告/错误与严格格式/元数据检查，DCT系数、量化、采样及颜色解释等无损验证；APP11及未知应用元数据保守拒绝，不复用PNG凭据移除授权。
4. 复用现有输出层的源/候选复查、NoGain、备份和独占副本；JPEG备份保留原始jpg/jpeg扩展名及大小写。
5. 真实引擎和故障替身回归，完整PNG回归不退化；三平台CI增加核心实测入口，未运行的平台明确待验收。

## 范围边界

- 仅J1a核心，不开放桌面JPEG/IPC，不实施有损、多格式批次/资源预约迁移、缩略图或安装打包。
- 继续用户手动GUI验收；不启动/控制桌面，不使用用户图片，不修改png-palettes，不提交推送。
- 原生引擎单进程单图片；管道线程仅负责I/O，不是嵌套编码线程池。资源限制不声称操作系统RSS硬限额。

## 2026-09-30 开工

- 核对当前分支、未提交文档及J0固定工具缓存；读取源/目标输出契约和PNG调用关系。
- Windows优先线索为受Prettier检查的tools/jpeg-lab/README.md缺少LF检出属性；需要实际干净检出复现，不仅依赖内存模拟。
- 开始实现前登记本记录；实际改动、验证与未完成项持续追加。

## 2026-09-30 实现与定向验证

- Windows真实checkout-index检出受检Markdown为CRLF且Prettier失败；增加LF属性后同样检出为LF且检查为true。不是用target被Prettier忽略后的CLI空检查冒充通过；远端失败行仍未取得，统一检查待下节。
- 新增独立jpeg请求/限制/报告/错误、结构/元数据预检、固定路径/可信SHA256/身份复查和受控子进程。原生helper链接J0固定静态MozJPEG，只接收字节；源系数、熵优化、候选系数各单进程，输入/输出/16KiB stderr有界，30s默认超时、取消kill/wait、管道join与私有工作目录清理。
- 采用严格白名单元数据：JFIF/Adobe、完整ICC、COM、单Orientation的Exif；APP11及未知/复杂元数据拒绝，未接JPEG凭据移除。验证DCT系数/实际量化/采样/颜色解释、维度和元数据原字节。J1a格式范围不变成桌面能力声明。
- 复用共享输出层；Source显式JPEG备份后缀策略保持原jpg/jpeg大小写，不依据文件扩展名改变PNG备份行为。补PNG内容误命名.jpg回归与JPEG原始OS非Unicode名称、扫描排除和目录树输出。
- 真实回归首轮发现MozJPEG默认扫描脚本把基线图转为渐进；系数相同但类别校验正确拒绝。显式清理默认扫描脚本并仅为渐进源建立脚本后通过，没有降低验证。另一失败来自Windows测试目录backup-jpg/backup-JPG大小写冲突，改为编号测试目录，不改变文件名用例。
- 固定C工具在Windows构建通过。沙盒Node派生编码器EPERM后经批准重跑，未绕过权限或关闭校验。最新真实结果：target/jpeg-lab-core-Zl4qol/core-report.json；20项接受语料/14项有收益，合成APP11明确拒绝；独立coeffdump、djpeg像素和标记复验也通过。
- 已验证有/无备份、精确原扩展名、NoGain不留副本/备份、原名目录树、早期/迟到冲突、各阶段取消、源变化、临时文件破坏、可解码量化表篡改被无损校验拒绝、损坏熵流/截断/12bit/像素/尺寸/扫描/系数预算和缺失/错误哈希/运行前工具变化。只使用隔离生成图片，未处理用户素材。
- 核心Clippy全目标/全特性与8项JPEG普通单元/进程故障回归通过。运行中取消测试等待自有子进程ready后再取消，阻塞stdin的超时/取消均回收后清理工作目录；不是只测预取消。标准完整PNG/应用检查和最新最终复验待执行，不沿用旧结果。
- 新增显式jpeg:core:check及三平台JPEG CI步骤，不把缺工具写成跳过测试。native/jpeg/README.md、README及第三方说明记录资源/工具信任/元数据/部署边界。工作区无新Cargo依赖/锁文件变更，协议仍v9，尚未修改桌面JPEG入口。

## 2026-09-30 最终验证与交接

- `pnpm check`通过：格式、lint、严格类型、155项前端、34份PNG语料/SHA256、Rust/TypeScript类型一致性、全目标/全特性Clippy及Rust全套（含105项桌面和2项doctest）。沙盒中固定pnpm版本签名因网络不可达失败，经批准提升执行后通过，没有关闭签名校验或修改锁文件。
- 随后审计补充kill与自然退出竞争的处理（wait已确认退出时不再误报kill失败）、双字节序Exif、ICC缺段/重复及元数据预算回归。最终10项JPEG单元/管道故障测试和核心全目标/全特性Clippy再通过；相关生产变更只影响尚未接桌面的JPEG进程回收。不把先前全套检查写成新增测试也在同次执行。
- 最终 `pnpm jpeg:core:check`通过，报告 `target/jpeg-lab-core-9A2xWY/core-report.json`；20项正常语料/14项有收益，APP11拒绝，独立系数/像素/元数据及真实安全输出回归通过。helper SHA256：`4f7848d38e4b1ed03f8d77656c28ef81b84cc5e3afd08f0586597f2f7e331cb9`。
- 因共享了语料生成函数，重新执行 `pnpm jpeg:lab:check`，21项无损、4个质量锚点、9项灰度/方向重编码及拒绝边界通过；报告 `target/jpeg-lab-6ccj5m/report.json`。这是脚本回归，不是重复引擎选型或扩大J0结论。
- 最终 `pnpm tauri build --no-bundle --ci`通过，未启动GUI/安装器。Windows EXE：2026-09-30 17:45:54（Asia/Shanghai），10,633,728 bytes，SHA256 `37699917D60769B55C540C8F1B5822C4EEB646C00486A56C7787188E06F5CC74`。桌面仍只支持静态PNG，helper未随此EXE分发。
- 文档本地链接、diff空白检查通过；额外对不在仓库format:check范围内的根README/第三方说明尝试Prettier，旧HEAD和当前均为false，确认是已有整篇样式，未扩大本轮全篇重排。受检tools文档与新增native说明检查通过，不能将额外检查写成全通过。
- J0归档及前序计划改动保留。业务涉及 `crates/pixofold-core/src/jpeg/`、`native/jpeg/`、共享输出最小适配、JPEG回归入口/CI/命令及对应说明；详细文件以本轮提交diff为准。未修改用户图片或png-palettes；参考仓库工作区保持干净。

## 2026-09-30 编写提交信息、提交与推送

- 用户要求根据工作区内容编写提交信息、提交并推送，无需执行其它任务。复核范围：26份文件（含J0记录从_plan迁至_fin、新增jpeg-lossless-core计划、core/src/jpeg四份、native/jpeg两份、core-check脚本与示例入口；另有.gitattributes/CI/依赖脚本/README/第三方说明与共享输出最小适配），无未暂存改动与未跟踪残留，git diff --check通过。
- 提交ff0a837「feat: 实现受控JPEG单文件无损核心并修复Windows检出格式」（26 files changed、2018 insertions、28 deletions；含jpeg-engine-lab.md从_plan到_fin的重命名）。沙箱内.git只读，add/commit/push按规则提升同一条命令执行，未绕开沙箱约束；7925c48..ff0a837  dev -> dev已同步origin/dev，本地与远端一致。
- 沿用本轮已完成验证（pnpm check：155项前端、Rust全套含105项桌面与2项doctest、34份语料与生成类型一致性；jpeg:core:check 20项语料/14项有收益与helper哈希`4f7848d38e4b1ed03f8d77656c28ef81b84cc5e3afd08f0586597f2f7e331cb9`、jpeg:lab:check 21项无损及锚点、17:45:54正式无安装包构建）；本次未改业务代码，不重复构建或测试。推送按push触发CI，三平台Check/桌面构建与JPEG job的新结果须以最终HEAD取得，不把本次推送写成已通过。
## 下一步与未完成项

1. a2031e7三平台JPEG核心已通过；应用Windows日志测试故障已在A0本机修复并随365e520推送，待以新SHA三平台结果验收，不将本机修复写成远端通过。
2. [J1b第一段](../261008/jpeg-lossy-core.md)已实现单文件保守有损；继续第二段必要的PNG/JPEG共用模型、探测/实际预算与单池预约，不放宽复杂Exif/签名保护。
3. J2：能力声明、协议/生成类型、过滤/导入/重试、混合批次、方向缩略图、工具随包和Windows原生运行，再做安装小试验。当前无新JPEG界面可供手测；既有PNG用户手测与A1收尾继续独立跟踪。

## 2026-10-08 平台复核与有损阶段关联

- 当前HEAD a2031e7 的CI run36699374325中，三平台JPEG引擎实验及真实无损核心均通过，macOS/Ubuntu应用检查/构建通过；Windows应用日志测试失败，实际原因与本机修复见[A0](png-native-name-portability.md)。
- 新增有损共用helper后，原有20项正常语料/14项有收益、APP11拒绝及可靠输出/故障回归再次通过；本轮实际验证维护在[J1b记录](../261008/jpeg-lossy-core.md)。保留无损独立验证，不把本轮92个有损/回退组合计为新三平台结果。
