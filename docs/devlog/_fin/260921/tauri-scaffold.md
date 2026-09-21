# Tauri 2 工程初始化

- 创建日期：2026-09-21（Asia/Shanghai）
- 归档日期：2026-09-21（Asia/Shanghai）
- 状态：已完成脚手架初始化
- 开发分支：`codex/initial-plan`；提交分支：`dev`
- 代码基准：`da73894`

## 目标与验收

在保留设计资料的独立仓库中初始化 React / Tauri 2 / Rust 核心三层结构。直接依赖使用官方仓库当前最新稳定版本并锁定；提供可重复的安装、开发、检查、测试与构建入口，实际验证 Windows。脚手架不实现或模拟图片压缩。

## 2026-09-21

- 初始化前工作区干净。
- npm 与 crates.io 官方元数据确认：Tauri runtime 2.11.6、build 2.6.3、CLI 2.11.5、JS API 2.11.1；React 19.3.0、Vite 8.3.0、TypeScript 7.0.2、Zustand 5.0.15。
- 工具链锁定 Node 26.9.0（Current 稳定版）、pnpm 12.5.1、Rust 1.98.1。现有全局 Node 24.19.0 / Rust 1.96.0 不修改默认版本；本次使用项目内便携 Node 和并存的 Rust 工具链验证。
- TypeScript 7 超过 typescript-eslint 8.70.0 声明的 `<6.1.0` 兼容范围，改用 Oxlint 1.83.0 做语法规则检查，TypeScript 本身负责类型检查。
- Windows 已安装 Visual Studio 2022 C++ 工具链与 WebView2 Runtime。

## 已完成

- 根目录建立 Cargo workspace，桌面壳与独立核心分别放在 `src-tauri/`、`crates/pixofold-core/`；桌面库使用默认 rlib，不生成无关的移动端库产物。
- React / TypeScript / Vite / CSS Modules / Zustand 启动页支持亮暗、中英和版本化偏好；太阳/月亮保持图标按钮。浏览器预览不会伪装为原生连接。
- `get_app_info` 只读命令返回核心版本及规划格式，明确 `compressionAvailable: false`；当前权限仅允许主窗口调用该命令。没有接入文件写入、对话框、shell 或压缩引擎。
- Rust DTO 用可选 ts-rs feature 生成 TS；`types:check` 只读比较，发现过期时失败。
- SVG 品牌图复制至 `src/assets/logo.svg`，Tauri CLI 生成桌面图标，移除本阶段不需要的 Android/iOS 图标。
- 配置固定开发端口、生产/开发 CSP、单一前端锁文件、共享 Cargo.lock、格式/lint/类型/测试/构建入口。
- CI 使用 Windows、Ubuntu 24.04、macOS matrix；GitHub Actions 取官方稳定 release，并固定 SHA。README、AGENTS 和架构文档同步当前已实现范围，补充直接依赖许可说明。

## 验证

| 范围 | 结果 |
| --- | --- |
| 依赖 | `pnpm install --frozen-lockfile --offline` 通过，未重新解析锁文件；没有忽略 peer 冲突 |
| 统一检查 | `pnpm check` 通过：Prettier、rustfmt、Oxlint、TypeScript、Vitest、生成类型一致性、全工作区/全部 feature 的 Clippy 与 Rust 测试 |
| 前端测试 | 5 项通过：浏览器与原生调用边界、失败传播、启动失败重试、主题/语言/偏好和无效持久化数据回退 |
| Rust 测试 | 核心序列化契约测试通过；桌面 crate 与 doctest 入口正常运行，无图像算法测试（尚未实现） |
| 生产构建 | `pnpm tauri build --no-bundle --ci` 通过，生成 `target/release/pixofold.exe`；构建指纹确认 `tauri` 与宏启用了 `custom-protocol` |
| Windows 启动 | 关闭 Vite 后，后台启动最终 release exe；事件循环就绪、窗口响应正常、标题为 PixoFold · 轻图，检测到 WebView2 子进程；随后关闭该测试进程 |
| 浏览器实测 | 本机 Vite 下检查亮色中文、暗色英文、390×844 窄窗口、刷新恢复设置；无横向溢出，SVG 加载正常，无 console error/warn |

生产构建与进程启动检查不等同于原生 WebView 内逐项 UI/IPC 自动化；界面交互实测来自浏览器，原生 IPC 边界另由单元/契约测试覆盖。macOS/Linux、远端 CI、安装包与签名尚未运行或配置。临时 Vite 服务和桌面测试进程已关闭。

## 关键修正与环境情况

- 全局 Node 保持 24.19.0，默认 Rust 工具链仍是原有 stable；项目单独固定 26.9.0 / 1.98.1。`.tools/` 中 Node 压缩包经官方 SHA256 清单验证，pnpm 由 npm 精确版本启动。
- Windows 沙箱最初限制联网缓存与 Node→Cargo 子进程，取得执行权限后完成依赖安装及类型生成。
- 最新 Vitest 的 `beforeEach` 会把返回函数作为清理回调；测试钩子最初直接返回 mockReset 的函数结果，改为不返回值后，原生失败传播测试正常通过。
- 生产 CSP 只允许同源图片，Vite 小 SVG 默认内联为 data URL 会被拦截，因此配置 `assetsInlineLimit: 0` 并重新构建。
- 桌面第一轮沿用多 crate-type 模板会额外链接 DLL 并产生无关链接器输出；本项目当前仅做桌面，改为默认 rlib。最终构建通过。

## 版本来源

版本查询日期为 2026-09-21；直接 npm 依赖取 registry 的 latest 稳定标签，crate 取 max_stable_version，工具链取官方 stable/Current 发布。来源：

- [npm 官方仓库](https://registry.npmjs.org/)
- [crates.io](https://crates.io/)
- [Node 官方版本清单](https://nodejs.org/dist/index.json)
- [Rust stable manifest](https://static.rust-lang.org/dist/channel-rust-stable.toml)
- [Tauri Vite 配置](https://v2.tauri.app/start/frontend/vite/)
- [ts-rs TS trait](https://docs.rs/ts-rs/latest/ts_rs/trait.TS.html)
- [Oxlint 官方说明](https://oxc.rs/docs/guide/usage/linter.html)

## 下一步与交接

脚手架任务已完成。本次提交包括新增工程、配置、锁文件、图标和本记录，以及 README、AGENTS、两份架构文档；没有修改旧 png-palettes。2026-09-21 用户授权在 `dev` 分支提交并推送至 `origin/dev`，具体提交与同步状态以 Git 记录为准。

下一步入口为 `docs/架构设计文档/pixofold-proposal.md` 的第 0 阶段：准备可分发图像语料并验证编码器构建，产出真实压缩基线。之后在 `crates/pixofold-core/` 完成质量模型、格式探测和可靠输出，再实现任务队列并连接 HTML 原型对应的业务界面。

## 2026-09-21 提交与日志规范补充

- 脚手架已提交为 `9112ff4`（`chore: 初始化 Tauri 2 工程脚手架`），并成功推送至 `origin/dev`；`dev` 已建立远端跟踪，推送完成时本地与远端一致、工作区干净。
- 根据用户要求，将 `AGENTS.md` 中开发日志的“按需记录”改为“开发相关工作必须记录”，明确开工建档、过程更新、验证证据、提交推送状态及交接要求；相关开发文档纳入同一任务记录。
- 本次仅修改开发规范与交接记录，验证范围为文档 diff 和规则一致性，不重复运行上一轮已通过的代码测试。
- 本次文档补充尚未提交或推送；不改变脚手架的功能范围和后续实施顺序。
