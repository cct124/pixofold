# PixoFold（轻图）

基于 Tauri、React 和 Rust 的本地批量图片压缩工具，计划支持 PNG、JPEG、GIF 和 APNG，由 png-palettes 重构演进。

当前已完成方案、UI 设计与可交互 HTML 原型，尚未创建 Tauri 应用工程或迁移压缩业务代码。界面支持亮暗主题，压缩质量统一采用 0–100 连续滑块和精细输入，默认 80；当前质量描述以纯文字显示在「压缩质量」标题行右侧。

HTML 原型采用导入即自动模拟的交互：空闲时显示中央识别区与底部操作技巧，拖入或选择图片、文件夹后切换为图片列表，逐张展示压缩效果；底部同步切换为总数量、体积变化与总进度，清除后恢复技巧，不设置开始压缩按钮。文件夹包含子目录；原型不会生成、覆盖或上传图片。

## 项目方案

设计、图片引擎选型、架构、实施阶段和验收标准见 [项目方案](docs/架构设计文档/pixofold-proposal.md)。

- [UI 交互与质量映射设计](docs/架构设计文档/ui-interaction-design.md)
- [亮暗主题界面设计图](docs/UI界面设计/README.md)
- [HTML 交互原型](docs/UI界面设计/PixoFold.html) · [使用与修改说明](docs/UI界面设计/HTML原型说明.md)

## 开源协议

除另有声明外，本项目依据 GNU General Public License 第 3 版或其后版本（`GPL-3.0-or-later`）发布，完整条款见 [LICENSE](LICENSE)。

第三方组件保留各自的版权和许可声明。计划中的 PNG/APNG 量化核心继续使用 imagequant；独立图片工具按各自许可证分发。
