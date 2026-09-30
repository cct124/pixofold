# 原生文件名测试跨平台修复与交接小修

- 创建日期：2026-09-30（Asia/Shanghai）。
- 状态：实现与Windows自动验证完成，已随d5b3445推送origin/dev；A0新SHA三平台检查和构建已触发、结果待完成。用户授权按[下一阶段计划](next-development-plan.md)推进。
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
