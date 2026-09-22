# PNG 有损与统一质量模型

- 创建日期：2026-09-22（Asia/Shanghai）
- 状态：Windows 本机实现、统一检查、发布构建与 CLI 验证完成；待新改动提交后的跨平台验收，暂不归档
- 分支：`dev`
- 代码基准：`3f6c6178ff635946666dd63768e09ac61bfa4d92`（已推送 origin/dev）
- 依据：[PNG 核心前序记录](../../_fin/260922/png-core-foundation.md)、[项目方案](../../../架构设计文档/pixofold-proposal.md)、[质量交互设计](../../../架构设计文档/ui-interaction-design.md)

## 目标与验收

在已有静态 PNG 单文件流水线上接入 imagequant 有损量化与质量映射版本 1，保留无损独立路径和现有原图安全契约；从 Rust API / 开发 CLI 验证，不提前接入 UI 或任务队列。

- [x] QualityValue 仅接受 0–100 整数，默认 80；无损模式不接受有损质量字段，Rust/TS 类型与边界测试一致。
- [x] 锁定 imagequant 4.4.1，关闭默认 threads；映射 min=0、target=q，速度/抖动为独立固定基线参数。记录实际 remapping 质量，不把 100 或库评分当作通用无损。
- [x] 8-bit/低位深 PNG 正确展开为 RGBA 量化，保留透明语义与适用显示元数据。16-bit 默认无损回退，不提供隐式降位深；无法可靠处理的 ICC/HDR/表示相关元数据明确回退。
- [x] 候选质量不达目标、透明度保护不通过或体积不优于源文件/无损候选时回退无损；结果报告实际路径、映射版本及回退原因。
- [x] 有损实际产物重新解码校验，复用现有副本/覆盖/备份/取消/源变化/冲突流程，无收益保留原图。
- [x] 扩展渐变和透明语料，记录 q 锚点的真实体积、库评分及独立误差指标；本机类型/lint/测试和 release 核心基线构建通过。跨平台和视觉验收范围如实注明。
- [ ] 新依赖/有损代码的 macOS/Linux CI 构建与文件回归（前序 CI 不包括本轮未提交改动）。

## 范围与关键约束

- 产品默认有损 q=80 的设计不变；已有开发 API 的默认请求继续无损以保持已发布单文件调用行为，调用方显式选择 Lossy。不迁移正式界面或新增 IPC 命令。
- 颜色处理采取保守策略：不实现 ICC 色彩转换；16-bit、ICC/HDR、依赖原色型/调色板的数据选择无损回退，不能悄悄改成普通 sRGB 或删元数据。有损路径不承诺隐藏 RGB 不变，无损仍严格保持。
- 输出只由现有 output 层提交；没有本次另建的覆盖捷径。不复制 png-palettes 的直接 File::create、固定默认 60 或 unwrap 处理。
- 用户本轮要求先提交推送现有工作再继续开发：前序代码已提交推送；本任务新改动在验证后保留工作区，交接时明确状态。

## 2026-09-22 开工

- 已重新定位 PngRequest、ProcessingReport、全部调用方、编码/解码与输出引用，复核原设计 min=0、target=q 和高位深保护要求。
- 已检索并局部读取旧项目的 imagequant 参数、gamma、remap、调色板生成，仅作参考，不修改旧仓库。
- 官方 crates.io 查询 imagequant 当前稳定版为 4.4.1（GPL-3.0-or-later，Rust 实现），下载官方源码核对 quality、gamma、remapping_quality 与协作取消 API；计划关闭默认 threads feature，复用现有 png 0.18.1 / oxipng 10.2.1。
- 前序 push CI 初次查询实际运行中；后续复查并将成功/失败记录到对应任务，不用 CI 配置代替证据。

## 2026-09-22 实施与验证

### 实际改动与决策

- 新增 model/quality、quality 纯映射、codecs/png_lossy；PngRequest 增加 mode，ProcessingReport 增加实际 processing/output_image。QualityValue/PngMode 从 Rust 生成 TypeScript；其余报告仍是核心 API，未增加 IPC 命令。既有构造器行为不变，使用结构体字面量的调用方需显式补 mode。
- 先生成独立无损候选；量化输出既必须达到 remapping 目标，也必须比源图和无损候选更小才被采用。最终采用的调色板/索引做 RGBA 回读校验，落盘产物再按候选严格验证；无损/回退对照源图验证。
- 透明端点固定，半透明误差独立限制为 8；不为了低 q 的体积收益放宽保护。gamma 必须在 0–1 开区间，sRGB 冲突或无法可靠转换的颜色元数据走无损。PNG 有损可改变色型/交错方式，不改变宽高/EXIF 方向。
- 追加 5 个192×128渐变样本，总数32；新增10项有损文件测试、3项质量契约及1项alpha单元测试。开发 CLI 支持 --lossy/--lossless，质量基线支持新输出目录保存产物。
- 只参考 png-palettes 的 imagequant 参数和调色板组织，没有修改旧仓库，没有迁移其直接覆盖实现。README、AGENTS、架构方案、第三方说明和语料说明同步。

### 问题与修正

1. 首次编译时内部 png 模块遮蔽外部 png crate，改用 lossless 别名。
2. ts-rs 对 serde 的 try_from/into/deny_unknown_fields 提示无法表达，改成手写边界 Deserialize + 私有 WireMode，不全局关闭警告。后续回归发现内部 tag 的 unit variant 忽略额外 quality 字段；WireMode 改为 Lossless {} 空结构变体后，严格反序列化回归通过。
3. 自生成 RGB 渐变的 q=80 实测评分只有79、半透明样本可能破坏 alpha 端点；保留明确回退，不修改目标或放宽透明检查来制造成功。质量/体积不是简单线性关系。
4. npm 在沙箱中因离线缓存缺失报 ENOTCACHED，按权限规则授权重跑锁定 pnpm 后通过；未更改全局工具链。
5. 复核无收益报告，量化产物不小于原图时提前回退，避免无收益时报告未实际写出的索引色输出属性。

### 已执行验证

- cargo test -p pixofold-core --locked --test quality_contract：3项通过，覆盖全部0–100映射、默认值、负数/超限/浮点/字符串/null/缺失/额外字段。
- cargo test -p pixofold-core --locked --test png_lossy：最终10项通过；包含RGB/灰度/低位深索引/tRNS/Adam7、透明与质量回退、16-bit/ICC/表示元数据保护、gAMA/EXIF/安全未知chunk保持、实际体积/输出属性、覆盖原始备份、各阶段取消、有效PNG候选替换、备份/源变化、晚到副本冲突和RGBA展开上限。追加由固定渐变派生的alpha=128真实RGBA用例，验证半透明并非一概回退，也能安全量化；该派生样本只在隔离测试目录生成。
- pnpm types:generate 更新生成文件；pnpm check 全流程通过：格式/lint/严格类型、5项前端测试、32样本字节/SHA256复查、Rust/TS一致性、workspace all-targets/all-features Clippy、34项Rust测试（原21项＋新增13项）。无新增GUI业务，未重复UI主题/语言验收。
- 随后补上半透明成功用例及无损反序列化/重复字段回归，再运行 cargo clippy --workspace --all-targets --all-features --locked -- -D warnings 和 cargo test --workspace --all-features --locked，最终35项Rust测试通过（原21项＋新增14项）。cargo tree 核对imagequant依赖树不含threads/rayon。
- cargo run -p pixofold-core --release --locked --example png_quality_baseline -- --output-dir target/png-quality-baseline-260922：25组真实单文件流水线完成、源图不变。数据见下表，产物留在忽略的target目录；基线默认临时目录会自动清理。
- pnpm tauri build --no-bundle --ci 通过：前端生产构建及Windows release桌面可执行文件成功，输出target/release/pixofold.exe；没有启动GUI或构建安装包。沙箱npm缓存错误按授权流程重跑后成功，未规避权限。
- release optimize_png CLI 实测有损80及显式无损副本分别98510→14052、98510→49273 bytes；阶段来自真实流水线。负数、101、小数、模式冲突、重复模式、缺少质量六类参数均失败且不写目标，源SHA256不变。产物保留于忽略的target/png-quality-cli-260922。
- 最终检查已跟踪diff及新增未跟踪文本，空白/rustfmt通过；34个本轮文档相对链接有效。检查脚本初次因CRLF警告和正则字符串转义误报，修正只读检查后通过，未为绕过检查修改项目换行配置。

### 质量基线

环境沿用前序：Windows11 x64 / Ryzen9 7945HX / NTFS / Rust1.98.1。imagequant4.4.1单线程、speed4、dither1；png0.18.1、oxipng10.2.1 preset1。耗时是release单次完整流水线含I/O，不含编译；没有峰值RSS、批量性能、旧项目速度比较。

| 样本 | q | 实际路径/库评分 | 输入 bytes | 最终 bytes | 耗时 ms | 黑/白背景 RMSE | 最大 alpha 误差 |
| --- | --- | --- | ---: | ---: | ---: | --- | ---: |
| gradient-rgb8 | 无损 | 无损 | 73929 | 42354 | 12.302 | 0/0 | 0 |
| gradient-rgb8 | 0 | 有损/0 | 73929 | 1542 | 27.578 | 39.937/39.937 | 0 |
| gradient-rgb8 | 40 | 有损/51 | 73929 | 8137 | 37.076 | 7.168/7.168 | 0 |
| gradient-rgb8 | 80 | 评分79未达标，回退 | 73929 | 42354 | 40.242 | 0/0 | 0 |
| gradient-rgb8 | 100 | 评分79未达标，回退 | 73929 | 42354 | 38.999 | 0/0 | 0 |
| gradient-rgba8 | 0/40/80 | 透明保护回退 | 98510 | 50483 | 26.030/34.288/36.029 | 0/0 | 0 |
| gradient-rgba8 | 100 | 评分84未达标，回退 | 98510 | 50483 | 37.504 | 0/0 | 0 |
| gradient-binary-alpha | 无损 | 无损 | 98510 | 49273 | 15.232 | 0/0 | 0 |
| gradient-binary-alpha | 0 | 有损/0 | 98510 | 239 | 26.543 | 41.899/41.899 | 0 |
| gradient-binary-alpha | 40 | 有损/56 | 98510 | 6074 | 32.570 | 6.244/6.244 | 0 |
| gradient-binary-alpha | 80 | 有损/85 | 98510 | 14052 | 40.307 | 3.303/3.303 | 0 |
| gradient-binary-alpha | 100 | 评分87未达标，回退 | 98510 | 49273 | 37.265 | 0/0 | 0 |
| gradient-display | 40 | 有损/51、元数据保持 | 74017 | 8225 | 36.142 | 7.168/7.168 | 0 |
| gradient-gamma | 无损 | 无损 | 73945 | 42370 | 12.272 | 0/0 | 0 |
| gradient-gamma | 0 | 有损/0 | 73945 | 1595 | 27.389 | 39.834/39.834 | 0 |
| gradient-gamma | 40 | 有损/50 | 73945 | 8154 | 35.698 | 7.213/7.213 | 0 |
| gradient-gamma | 80 | 有损/80 | 73945 | 16896 | 41.907 | 4.175/4.175 | 0 |
| gradient-gamma | 100 | 评分80未达标，回退 | 73945 | 42370 | 38.022 | 0/0 | 0 |

RMSE为0–255编码值在黑/白背景合成后的独立RGB误差，非线性光/SSIM/感知模型；库评分也非跨格式统一画质。自生成样本采用未压缩DEFLATE，比例不代表真实照片压缩率。查看RGB原图/q40和二值透明原图/q80产物：可见量化噪声与细节损失，未发现尺寸/通道错位；仅作小样本目视检查，透明正确性以独立alpha回归为证，不宣称完整透明边缘、真实照片或显示器色彩校准验收。

## 下一步与阻塞

本机实现与上述验证完成，无用户决策阻塞。当前修改尚未提交/推送，HEAD和origin/dev仍为3f6c617，前序三平台CI不能替代本轮验证；新代码等待后续提交授权/安排后触发CI。本次仅执行用户要求的先提交前序代码，再开展下一步开发，没有再次提交新有损改动。

未提交范围：Cargo清单/锁文件，核心质量模型/适配器/流水线/CLI与测试，Rust生成TS，5个新语料及生成器/清单，README/AGENTS/架构/第三方说明和devlog（前序归档及索引）。png-palettes未修改。

下一步入口：先核实新增有损代码的三平台CI并补真实素材/柔和透明边缘验证，再从pipeline::optimize_png上层建立Rust权威批量任务、有界并发/总工作集预算、批次参数快照、取消/重试；最后连接Tauri原生导入和正式PNG桌面闭环。当前仅有同步单文件API，不在本轮预建未接入的队列或假进度。

## 2026-09-22 提交与推送

- 用户明确要求提交、推送本轮PNG有损代码。已核对dev分支、origin地址及工作区范围，改动与前轮交接一致，未发现额外无关文件或已有暂存内容。
- 沿用前轮已执行的统一检查、最终35项Rust测试、Windows发布构建与CLI证据；本次只补交接文档，不重复未变更业务的全套构建。提交前重新检查diff/暂存内容和空白；保留前序归档移动，不提交target中的实验产物。
- 本轮按正常提交推送origin/dev，不改写历史或强推；实际提交号和同步结果在成功后补记，不提前声明远端成功。
