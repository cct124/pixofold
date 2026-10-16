# JPEG损坏输入分类与错误提示

- 创建日期：2026-10-16（Asia/Shanghai）。归档日期：2026-10-16。状态：本机实现、自动验证、新MSI局部手测及日志复核完成，按Windows验收范围归档；实现已在3425b5b，本次日志补记/归档尚未提交，新SHA平台门槛后续独立核对。
- 基准：dev/e7b7b7a；保留JPEG手测复核对两份devlog文档的未提交修改，不提交推送。
- 用户反馈：06-jpeg-corrupt-entropy.jpg按预期失败，但详情只显示“压缩引擎拒绝输入或异常退出”，希望错误提示更明确。

## 目标与验收

- 明确损坏/截断的JPEG解码错误与未知引擎退出分开；只能依据可信helper的固定分类，不能把全部非零退出码都称为图片损坏，也不解析/展示原始stderr。
- 沿用现有invalid_input/validation/tool_exit协议类别，提示包含原因、未写入/原图安全状态及可行处理建议；中英文同步。源输入损坏不进入凭据确认，候选损坏仍报告结果验证失败。
- 真实损坏样本验证分类、原件不变及无输出；已有JPEG核心/桌面确认回归与受影响静态检查通过，重新准备helper并构建新MSI。新包GUI验证与自动检查分别记录。

## 实施发现与决策

- native/jpeg/helper.c当前将所有libjpeg错误/警告汇为退出码2，Rust父进程统一转换为ToolExit；06样本的空熵数据因此不能区别于库内部/运行故障。
- 对固定MozJPEG解码器的明确坏标记/Huffman/截断类错误和警告新增退出码6；其他库错误、编码器错误及未知退出保持原分类。Rust只在可信引擎层识别此码，进程回收和取消优先级保持。
- 扩展错误文案与现有详情浮层适配，不增加路径、stderr或凭据内容；不修改压缩质量/输出/移除授权策略。

## 2026-10-16 实现与核心验证

- helper仅对解码器的明确坏标记、Huffman、扫描控制和截断警告/错误使用6；未知退出、内存/内部/编码器错误保留原分类。Rust在引擎身份验证和真实进程回收后将6映射为InvalidJpeg，批量及v15 DTO沿用invalid_input；候选回读中的InvalidJpeg统一为ValidationFailed。新增未知退出/取消不误分类回归，IPC回归确认内部错误详情不进入快照。
- 中英文invalid_input、decode、tool_io、tool_exit文案补齐原因、未改写原图和重新下载/导出或重启应用的建议；通用退出不推断原图损坏。已有详情浮层按视口限制宽高并可换行/滚动，未改变布局或主题。现有前端浮层回归补查安全状态及建议。
- 初次pnpm prepare/test受沙箱数据库访问限制，按原命令提升权限后通过；没有绕过资源身份核对。Node26.9.0/pnpm12.5.1固定工具链下重新prepare，helper SHA256为7b1a0fe98640844041c873c06bf30e755bf24378a100b97b78822665b33976eb。
- `pnpm check`通过：170项前端、331项Rust测试及2项doctest，含格式/lint/类型/生成类型一致性/语料/Clippy。证据target/jpeg-error-261016-check.log；单独前端首轮亦通过，之后未再重复全套检查。
- `pnpm jpeg:core:check`通过，证据target/jpeg-lab-core-oTPLe4及target/jpeg-error-261016-core.log：原有20项真实无损/92有损质量及回退组合、混合批次、5份元数据/凭据输出独立复验保持通过。损坏熵样本明确断言InvalidJpeg，带凭据损坏源的无损及质量70两种模式均拒绝且原件不变，不进入凭据确认。
- 回归中的metadata-results/bad-entropy.jpg与用户06-jpeg-corrupt-entropy.jpg SHA256均为84546b17f206f5e5f79954046aacf8bf21b05e4f5b56b8e07fe1259d7a5fbce3，证明覆盖同一内容。输入仍可通过结构扫描，实际解码失败后详情使用新的invalid_input提示。

## Windows交付

- `pnpm desktop:build --bundles msi --ci`通过，证据target/jpeg-error-261016-build.log。新包target/deliverables/261016-error-feedback/PixoFold_0.1.0_x64_en-US.msi，5,111,808 bytes，SHA256 `cb061bb4b1fa9aca51755298d2b6cb2a2900a265d668ad959f52b3fc79e135ec`。应用0.1.0/协议v15未变。
- dark解包及target/jpeg-error-261016/check-msi.ps1核对25文件、JPEG/GIF清单与XML许可均通过，主程序仅有预期MSI类型标记补丁；报告target/jpeg-error-261016/msi/report.json。没有自动安装，旧MSI及其验收证据保留。
- `pnpm jpeg:bundle:check --resources E:/Project/pixofold/target/jpeg-error-261016/msi/restored`通过，证据target/jpeg-bundle-ciEQGd/report.json及target/jpeg-error-261016-bundle.log；包内新helper身份、混合工作流、PNG/JPEG共同确认、四种输出策略与退出验收通过。
- `git diff --check`通过；本轮无依赖或DTO变化。保留上轮手测日志的历史ToolExit记录，不改写为新分类；新构建才对明确损坏源返回invalid_input。功能/测试/文档仍为本机修改，未提交推送，尚无对应新SHA的三平台结果。

## 收口与后续

- 用户已反馈新MSI手测完成且06损坏JPEG提示正常；正式日志确认新的输入错误分类，原件保持，未授予凭据移除且未提交输出。局部验收收口，不重复要求手测；语言/窗口细分覆盖不超出用户反馈，日志不记录实际渲染文案。
- 复核结束时功能已提交为3425b5b，本地origin/dev跟踪引用一致；后续按此SHA核对平台门槛，不沿用旧提交CI为本轮背书。本次日志补记与归档仍为本机文档修改。

## 2026-10-16 新包局部手测与日志复核

- 用户反馈新MSI手动验证完成，06-jpeg-corrupt-entropy.jpg错误提示正常。本轮核对最新正式会话、错误类别、无凭据授权/无输出提交、原件哈希与退出健康；不重复构建或改动功能代码。
- 开始时仍为dev/e7b7b7a，上轮错误分类/文案及日志验收改动均未提交，保留全部既有修改。
- 新会话run-1792122994795-Z9hDpkIcElZJ.jsonl共16条，Asia/Shanghai 2026-10-16 11:56:34–11:56:56，Windows x86_64/release/0.1.0/v15，JPEG/GIF引擎均校验成功。11:56:44的唯一JPEG质量70、batch1/job1/attempt1在Reading阶段返回InvalidInput，批次1失败/0成功，符合损坏输入预期。
- 未出现credentials确认/移除、output或backup事件；唯一WARN为该预期InvalidInput，没有ERROR。一次缩略图Unavailable与损坏且仍含凭据源的受限预览一致。正常退出码0，dropped_events=0、write_failures=0；序列完整，唯一任务终态及批次统计一致。
- 06源文件仍为1,362 bytes，SHA256 84546b17f206f5e5f79954046aacf8bf21b05e4f5b56b8e07fe1259d7a5fbce3，与交付清单一致。build_source仍为unknown，日志不含文件名、MSI哈希或渲染文案；样本关联及界面验收结合用户反馈，不能单靠日志宣称精确安装包身份。
- 同时核对先前会话run-1792119967321-hAxdT0j2in32.jsonl（11:06:07–11:56:01，81条）：其中11:06:28/11:06:38的两次ToolExit发生于错误提示修正构建之前，属于历史记录；11:56:34新会话已是InvalidInput，不将两份会话混作新包回归失败。
- 固定Node26.9.0执行target/jpeg-error-261016/manual-log-review.mjs，退出码0；报告target/jpeg-error-261016/manual-log-review.json保留日志SHA256、任务与健康核对、原件哈希及前后会话范围。`git diff --check`及链接核对通过；本轮仅更新/归档记录与索引，没有修改功能代码、重复测试构建或提交推送。
- 最终状态复查发现仓库在本轮读取期间已更新到dev/3425b5b（完整SHA 3425b5bbb3d909fdd28e26acfdc473f64f22680c），本地origin/dev同SHA，提交包含错误分类/文案实现和先前记录。此为外部已发生的Git状态变化，本助手本轮没有执行提交或推送；仅剩索引及本任务归档补记未提交。远端状态和新SHA CI未实时查询。
