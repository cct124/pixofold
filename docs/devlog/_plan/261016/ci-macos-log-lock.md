# macOS CI日志文件锁失败排查

- 创建日期：2026-10-16（Asia/Shanghai）。状态：本机修复与统一检查完成；用户已授权提交推送dev、合并main并核对新SHA CI，正在执行交付。
- 开工基准：dev/2d7d1ab，工作区干净；不提交推送，不修改main触发过滤。
- 用户请求：排查切换main默认分支和CI触发范围后的上一次Actions打包失败。

## 目标与验收

- 核对真实run/attempt/job/失败步骤，区分检查失败、构建失败及未执行的打包步骤。
- 保存远端日志，复查诊断日志锁及活动文件保护；定位可验证的问题后修复并增加确定性回归，不靠重跑、睡眠或全局串行测试掩盖。
- 本机受影响检查通过，记录无法在Windows直接验证的macOS范围；新代码未提交前不能声称GitHub/macOS验收成功。

## 2026-10-16 远端证据

- 查询main最近3次运行：38858977573/c1f13a5成功；38863703936/0014780首轮macOS失败、attempt2重跑成功；当前38867054969/2d7d1ab首轮macOS再次失败（查询时Windows Check仍运行中）。
- 新失败job116862193470与旧失败job116851592520均位于Check (macos-latest)的第8步“Check formatting, types, lint, tests and Rust”；桌面测试126通过/1失败，active_instances_are_never_deleted_and_all_busy_slots_refuse_an_eleventh_file在storage/tests.rs:47的顺序append返回WouldBlock。后续Build desktop executable被跳过，当前workflow本身使用--no-bundle，并非MSI打包命令失败。
- main触发规则正常生效；同一锁失败在相邻两个提交重现，前次“偶发、重跑通过”不构成根因修复。继续核对目录协调锁的生命周期与并行子进程的句柄继承。

## 2026-10-16 复现、修复与证据边界

- 本机固定Rust1.98.1标准库源码及其同版本公开源码均说明：File锁随所有复制/继承句柄关闭才释放，或由unlock显式释放；Apple/Linux后端使用flock。原Directory::lock、活动sidecar与清理正文锁均仅依赖File关闭，没有显式解锁。并行创建子进程时继承描述符可在fork到exec之间暂时延长临界区；原始CI日志未记录具体持锁句柄，不能直接证明当次的继承者身份。
- 在Windows以复制句柄确定性复现：目录锁所有者drop后新持有者仍WouldBlock；Store.close后被保留的lease副本仍阻止下一个写入器轮转。新增两项回归在修复前均稳定失败（0/2），证据target/ci-macos-log-lock-261016/before-fix.log；独立std::fs最小复现也得到after_owner_drop=WouldBlock、显式unlock后可取得锁。
- 使用已有WSL Ubuntu/Python标准库，通过fork与双向管道握手，无sleep地保持子进程的继承描述符；父进程仅close时下一把锁失败，显式LOCK_UN时在子进程仍持句柄期间可取得锁。target/ci-macos-log-lock-261016/fork-lock.py及fork-lock.json保留证据。这是Linux机制复现，不等同macOS完整应用测试。
- WSL初次在仓库路径查询rustc误触发项目1.98.1自动同步，已中断，未使用该同步作为验证；后续只读确认原有stable为1.89.0，实际Unix补充验证使用已有Python。没有把非固定Rust版本输出写作项目构建门槛。
- 新增私有LockedFile所有者：正常release显式unlock并传播失败；提前返回/错误展开用Drop尽力解锁、File关闭兜底。目录轮转、活动lease及历史文件清理共用该所有者；真实竞争仍立即WouldBlock，非竞争I/O错误保留原类别，不增加固定重试等待，不把全部测试改为串行。
- 当前9项Windows存储回归全部通过，包括两项新增、10个活动实例保护、第11个拒绝、正常轮转、独立进程锁及活动JSONL读取，证据storage-after-fix.log。获取锁失败后也保持原持有者锁，释放后才允许下一实例使用；活动文件保护不放宽。
- 远端运行38867054969最终于2026-10-16 15:46:53（Asia/Shanghai）结束，11/12成功，仍只有macOS Check失败；Windows/Ubuntu桌面构建及三平台格式实验通过。远端观测和两份失败日志保存于target/ci-macos-log-lock-261016/remote-observation.json、job-116862193470.log、job-116851592520.log。未重跑旧SHA，未提交/推送修复。

## 最终本机验证与交接

- 固定Node26.9.0/pnpm12.5.1/Rust1.98.1执行`pnpm check`通过：15个前端文件173项，333项Rust测试及2项doctest，含125项Windows桌面测试、格式/lint/类型/生成一致性/Clippy与语料哈希。证据target/ci-macos-log-lock-261016/check.log。沙箱pnpm数据库访问被拒后按原命令提升权限运行；没有跳过失败用例或放宽lint。
- `git diff --check`及本轮所属文档本地链接通过；README补充显式解锁生命周期，索引和既有main CI记录补充后续排查入口。保留前次重跑通过的历史过程，不将其改写为已解决根因。
- 当前main/dev及本地origin引用均为2d7d1ab；本轮仅修改诊断存储模块与两个回归、README、索引及开发记录。工作流触发、压缩核心与打包配方均未修改，不重复构建MSI。
- 修复确定性证明了句柄副本/继承造成锁超出所有者生命周期的问题，且Windows生产模块回归与Linux机制实验一致；原macOS CI没有锁持有者跟踪，仍需新提交的macOS完整运行确认该失败是否已消除。

## 2026-10-16 提交与远端验收

- 用户明确授权提交并推送本次修复到dev，再合并到main并核对新一轮Actions，重点确认macOS检查及后续构建。此授权替代开工时仅本机排查、不提交推送的范围。
- 提交前重新核对：dev/main及origin两分支均为2d7d1ab，fetch成功；工作区仅有本任务6个文件，代码diff与已通过本机检查的内容一致。沿用上述完整验证，不重复运行相同检查。
- 按dev提交及推送、main快进合并及推送顺序执行；普通推送，不强制改写历史。实际提交号、远端同步结果与新SHA CI结论随执行补记。

## 下一步

- 按已获授权提交推送dev并合并main，以main push触发新SHA CI，重点核对macOS统一检查、桌面构建及部署验收；不能用2d7d1ab重跑成功代替修复验证。
- 若新SHA仍在同类位置失败，按新的锁错误类别和日志继续查实际持有者；保持非阻塞竞争与活动日志保护，不全局串行化测试或固定延迟重试。
