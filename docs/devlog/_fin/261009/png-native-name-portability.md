# 原生文件名测试跨平台修复与交接小修

- 创建日期：2026-09-30（Asia/Shanghai）。
- 归档日期：2026-10-09（Asia/Shanghai）。
- 状态：完成。当前c287f4e三平台应用统一检查/构建与JPEG jobs均通过，原生名称、行尾及Windows日志测试基线任务收口；历史复现/修复记录保留如下。
- 基准：`dev/317c6d6`；开工保留前轮未提交的交接索引和阶段计划。实施阶段未提交推送，2026-09-30用户已明确授权本轮提交推送，实际结果续记末节。

## 目标与验收

- 修复当前macOS CI在测试输入创建阶段对非UTF-8名称的错误前提，同时覆盖备份和导入副本。纯名称逻辑保留非Unicode契约，真实I/O明确文件系统前提；不能吞掉权限、磁盘或未知失败。
- 当前文件系统拒绝非法编码名称时，验证无产物并继续执行可创建名称的真实备份/副本/扫描流程，不忽略测试或有损转换名称。
- 修正About中英可选备份文案、两处过期设计状态和opener第三方依赖说明。
- 运行相关核心回归、统一检查和Windows正式构建；macOS/Linux实际结果须对应后续真实执行，不能以Windows通过代替。

## 2026-09-30 开工与实施

- 复核两处失败前提：`png_backup_names.rs`及`png_import.rs`均用`cfg(unix)`创建`x\xff`名称。当前CI在第一个测试的`fs::write`收到OS92，尚未进入备份调用。
- 生产备份前缀、原名副本和产物排除使用原始`OsStr`；本任务不改变这些策略。计划增加无I/O的非Unicode名称契约，测试夹具仅在macOS的已知非法字节拒绝时改用支持的Unicode名称继续真实流程，其他错误仍使测试失败。

- 已增加共享原生名称夹具：Windows未配对代理项、Unix非UTF-8字节先尝试真实创建；仅macOS的Darwin EILSEQ（92）验证目录无产物后改用中文/组合字符/emoji名称继续完整工作流。权限、磁盘和其他错误使测试失败。Linux/Windows原用例保留，无I/O名称契约独立覆盖备份前缀、严格产物排除和副本规划。
- About中英文改为覆盖时可选择备份；同步两处设计文档的静态PNG/v9实现状态、核心API默认无损与工作台默认有损80的区别。第三方说明补充已使用的Rust侧tauri-plugin-opener 2.5.4。

## 验证与下一步

- `cargo fmt --all -- --check`及`cargo test -p pixofold-core --locked --test png_backup_names --test png_import --lib`通过，定向回归53项（25 lib、6 backup、22 import）。
- `pnpm check`通过：155项前端测试、34份语料/SHA256、Rust/TS类型一致性、全目标/全特性Clippy及Rust全套（含105项桌面和2项doctest）。使用项目便携Node 26.9.0和固定pnpm；沙盒内pnpm签名校验失败后经批准重跑，未关闭校验。
- `pnpm tauri build --no-bundle --ci`通过。Windows正式EXE：2026-09-30 14:09:13（Asia/Shanghai），10,632,704 bytes，SHA256 `E10D4C8BBBC3FE4336C83265215B64C6B9C9B83738BFC4E2699574E348C14CA6`。仅构建未启动GUI；未修改用户图片。
- `git diff --check`通过。macOS/Linux尚未运行新代码，不以旧SHA的CI为修复背书；未获用户明确提交/推送要求，不触发远端新SHA验证。
- J0入口与CI补齐后最终统一检查仍通过；本机EXE时间/大小/SHA256复核保持上述标识。10份相关文档103个本地链接、23份变更/新增文本空白/NUL检查通过；已有分析计划与索引修改保留，未暂存、提交或推送。
- 后续入口：A1用户原生收尾与J0 JPEG引擎实验；完整阶段排序仍维护在阶段计划。

## 2026-09-30 提交与推送授权

- 用户明确要求提交并推送本轮代码。复核dev分支、23份已验证变更/新增文件与未暂存状态，远端dev仍为317c6d6，无其他新增改动；未配置自定义hooks，默认pre-commit/commit-msg不存在。
- 本次一并提交A0/J0实现、所属设计说明与持续记录；复用本轮已完成的统一检查、定向回归、JPEG实验和Windows正式构建证据，不重复构建或启动GUI。文档链接/空白检查与待提交diff另作复核，缓存/引擎源码/图片产物不进入提交。
- 2026-09-30 16:11:44（Asia/Shanghai）形成业务提交`d5b3445cfa9c4e30f1a8da82a448fedd66308097`，23份文件、1095行新增/46行删除；提交前`git diff --cached --check`通过。`git push origin dev`成功，实际同步范围`317c6d6..d5b3445`，未改写历史。
- 16:12:07触发[CI run36688238456](https://github.com/cct124/pixofold/actions/runs/36688238456)，查询时in_progress；仅确认新SHA启动，未取得macOS/Linux完成结果。提交/推送事实通过后续交接文档提交入库；最终验收以该文档提交后的HEAD运行结果为准。

## 2026-09-30 当前HEAD平台复验与Windows待办

- 只读查询当前HEAD7925c48的[CI run36688753527](https://github.com/cct124/pixofold/actions/runs/36688753527)：Check(macOS/Ubuntu)均success，旧macOS名称夹具失败已不阻断全套检查与构建；Windows在统一检查步骤失败，构建skipped。JPEG实验三平台独立通过不替代Windows应用基线。
- 日志REST接口403、公开页面要求登录，注释仅exit code 1，尚不能确认实际失败行。已用固定Node26.9.0内存复现新tools/jpeg-lab/README.md的LF/CRLF Prettier差异（true/false），并确认该受检Markdown无eol属性；作为首要排查线索而非已证实CI根因。
- 下一步读取失败日志或干净Windows检出复现，最小修复行尾/格式契约并复验，不能通过跳过文件或关闭规则恢复绿灯。本轮未改业务/attributes或重跑远端CI；A0继续活动，不归档。

## 2026-09-30 授权后Windows行尾修复

- 使用git -c core.autocrlf=true checkout-index将受检实验README检出到隔离target/windows-eol-before，实际字节含CRLF，Prettier API检查为false。直接Prettier CLI会因target在ignore中而忽略，故不将其“通过”计作验证。
- 增加Markdown的LF属性，不跳过文件或放松格式；同时固定JPEG原生工具C源/CMake文本的LF，避免源码身份哈希因平台检出换行不同而漂移。
- 这是确实复现并修复的Windows检出缺陷；远端实际失败日志仍未取得，不能承诺是唯一CI根因。后续以本机统一检查及授权推送后的新SHA三平台结果验收。

- 同样checkout-index检出修正后的实验README为LF，Prettier API为true；本机`pnpm check`及最终Windows正式构建通过，实际结果和EXE标识见[J1a交接](jpeg-lossless-core.md)。这恢复的是本机检出行尾契约与检查，不冒充未执行的新SHA远端Windows结果。待用户明确授权提交推送后以三平台CI验收，A0继续活动。

## 2026-10-08 授权实施、实际失败与确定性修复

- 用户授权按阶段计划实施；开工 dev/a2031e7，工作区干净。优先修复 Windows 当前失败，再推进独立 JPEG 保守有损；不提交推送，不自行恢复 GUI 控制。
- 隔离 target/ci-windows-a2031e7-261008 用 core.autocrlf=true checkout-index 检出、锁定依赖离线安装，pnpm check 全套通过（155 前端、34 份语料、Rust 全套含 105 桌面/2 doctest）。源码来自干净索引，增量产物目录复用；没有据单次通过关闭远端问题。
- GitHub 连接器成功读取真实日志：旧 7925c48/job109800752943 确为 tools/jpeg-lab/README.md 格式失败；当前 a2031e7/job109835378142 格式通过，在 diagnostics::tests::real_credentials_processing_logs_wait_then_only_the_chosen_backup_policy 查找 output_commit_succeeded 时 None.unwrap() 失败（104 桌面通过/1 失败）。原 REST 的403限制已通过可用连接器解决。
- 增加测试诊断后，4 测试线程配置第一轮即复现：策略0已真实备份和提交，但事件仅有 session/image_policy/凭据等待/内存移除/session_finished；队列丢失和写入失败均为0。排除压缩失败、日志排空或慢盘超时。
- 固定 tracing-core 0.1.36 只有一个作用域订阅器时，以触发线程默认订阅器注册新调用点；并行无订阅线程可把调用点缓存为 never。新增确定性回归先在作用域外线程触发同一调用点，再在作用域内及继承 dispatcher 的 worker 发事件，修复前独立运行实际0条/应有2条，稳定失败。
- 测试辅助入口安装一次无写盘全局 registry，再安装每项独立 EventLayer，模拟生产先全局初始化的前提。作用域外事件不混入，作用域内/worker保持独立字段。仅修改测试，不改生产日志、压缩/输出，不串行化测试、不加休眠或跳过断言。
- 7 项日志定向回归及独立进程的确定性回归通过；按复现配置连续5轮完整桌面测试（每轮106项、4测试线程）通过，日志 target/diagnostics-ci-fixed.log。最终统一检查与正式构建待本轮整体改动完成后执行。
- 期间发现 A0 修复及开发记录恢复为旧内容，用户明确表示没有主动还原并要求恢复；已按已验证方案恢复，交付前再次核对实际 diff。未归档或声明远端全绿。
- 本轮最终pnpm check及pnpm tauri build --no-bundle --ci通过，实际命令/产物身份集中记录于[J1b交接](jpeg-lossy-core.md)。全套包含新增106项桌面回归，源代码与恢复后的测试修复一致；新SHA三平台结果仍待提交后取得，A0继续活动。

## 2026-10-09 当前提交平台验收与归档

- 本日只读核对远端dev与本地同为c287f4e440b27ef210900b5f0cec0c7d30e9ed47，包含365e520修复；[CI run37761724641](https://github.com/cct124/pixofold/actions/runs/37761724641)已于2026-10-08 18:25:58（Asia/Shanghai）完成success。
- Windows应用job113259883712、macOS应用job113259883689、Ubuntu应用job113259883639的统一检查与正式无安装包构建均success；三个JPEG jobs也全部success。当前Windows红灯已由对应新代码的实际运行验收，不再列为后续开发前置。
- 本任务完成并按实际归档日移入_fin/261009；不扩大为原生GUI/正常退出/安装或后续混合JPEG批次验证。本日没有重跑测试/构建或改业务，阶段下一步见[持续计划](../../_plan/260930/next-development-plan.md)。
