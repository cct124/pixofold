# Windows MSI 应用数据清理

本模块只负责卸载选择和当前用户应用数据清理，使用项目 GPL-3.0-or-later 许可。生产DLL嵌入MSI的Binary表，不安装为通用删除工具，不开放路径IPC。

## 行为和边界

- 标准Tauri WiX模板通过 `src-tauri/windows/installer.wxs` 链接片段。完整/基本UI的卸载在 `InstallInitialize` 前展示原生确认，删除选项每次默认不勾选，默认按钮为取消。取消返回1602，保留软件和数据。
- `UPGRADINGPRODUCTCODE` 存在或 `UILevel <= 2` 时不显示、不清理。动作先清空选择属性；命令行预设删除属性不能使静默卸载删除数据。安装、修复也不会触发清理。
- 勾选后仅在 `InstallFinalize` 成功后执行。清理失败不回滚已完成的卸载，提示部分数据保留。弹窗语言跟随当前Windows用户的UI语言（中文/英文）。
- 发起方 `UserSID` 必须匹配执行令牌；服务账号、无法解析用户或身份不一致时保留数据。根目录由当前用户Known Folder解析，不从MSI属性接收任意路径，不遍历其他用户配置。
- 固定处理 `%LOCALAPPDATA%/app.pixofold.desktop/EBWebView`、`overwrite-consent-v1.json` 和 `log-location-v1.bin`。未知根目录文件保留；用户图片、输出目录、备份和共享WebView2运行时不属于清理目标。
- WebView子树逐级持有目录句柄，拒绝重解析点，深度最多64层、条目最多100,000个。通过句柄删除目录项，不修改只读属性；硬链接其他名称指向的内容保持。
- 日志定位记录由正式应用保存实际诊断目录，格式为8字节魔数（PFXLOG1及NUL）、u32小端UTF-16单元数、原始UTF-16路径；没有有效记录时仅尝试当前用户LocalAppData下的 `Temp/pixofold-logs`。目录必须属于当前用户且名为 `pixofold-logs`。
- 只删除符合固定名称、首行schema/event的日志；先取得目录协调锁和日志活动锁。活动文件、链接和未知文件保留；零字节协调锁与可能残留的空目录保留，避免其他实例仍持旧句柄时破坏互斥。自定义TEMP路径不会被当成任意目录递归清空。

## 构建与验证

`pnpm installer:prepare` 使用现有CMake发现逻辑、MSVC、静态CRT和 `/W4 /WX` 编译，产物在 `src-tauri/windows/runtime/`，由Git忽略。Windows桌面构建钩子自动执行；其他平台返回不适用。

`pnpm installer:check` 已纳入统一检查，在隔离target目录验证清理条件、实际删除、硬链接、junction、当前用户身份和活动文件保护。另构建无删除能力的 `dialog-preview.exe` 及仅供独立MSI验证的probe DLL，不进入正式安装包。

`pnpm installer:msi:check` 需要本机WiX 3.14（Tauri工具缓存），构建独立ProductCode/UpgradeCode的当前用户测试包，实际安装、静默卸载、升级并核对测试数据哈希。测试身份与正式PixoFold独立。管理员终端可执行 `node tools/windows-installer/check-msi.mjs --per-machine` 验证正式安装范围；不会自动提升UAC。报告必须标明实际验证的安装范围。

Windows设置、控制面板和维护向导的新弹窗、勾选/取消结果以及正式包全机安装需单独手测，不能由无GUI回归替代。证据与步骤见[连续记录](../../docs/devlog/_plan/261016/overwrite-consent-uninstall-data.md)。
