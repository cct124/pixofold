# JPEG引擎与语料最小实验

- 创建日期：2026-09-30（Asia/Shanghai）。
- 状态：Windows最小构建/实验已通过，已随d5b3445推送origin/dev，三平台新SHA实验CI已触发、结果待完成；正式核心/桌面接入未开放。按[阶段计划](next-development-plan.md)J0推进，A0平台基线待复验。
- 基准：`dev/317c6d6`，保留阶段计划、A0修复与交接小修；实施阶段未提交推送，2026-09-30用户已明确授权本轮提交推送，实际结果续记末节。

## 目标与验收

- 固定MozJPEG源码身份，构建可重复的开发实验入口，在隔离目录验证系数层无损和有损重编码、元数据、错误和资源边界。
- 语料由本项目生成；覆盖baseline/progressive、灰度、EXIF1–8、ICC、CMYK、已优化和损坏输入；不接产品导入或声明JPEG已经可用。
- 源文件哈希保持、输出重新解码、无损不变量和元数据保持均明确验证；有损q=100不叫无损。明确正式适配仍需的调用/资源/颜色/凭据策略。
- Windows实际构建/运行、CI配置与未来真实三平台结果分别记录。

## 2026-09-30 引擎定位

- 官方GitHub release接口的latest记录仍为v4.1.1，而tags另有v4.1.5；本实验选择明确的v4.1.5稳定标签，解析到不可变提交`6c9f0897afa1c2738d7222a0a9ab49e8b536a267`，不将release接口结果误称最新tag。
- 安全Rust包装层当前Cargo源码为mozjpeg0.10.13，文档要求catch_unwind，公开解码/编码接口不提供系数转写；系数无损需独立jpegtran/受控系数接口。实验先用同一官方源码构建cjpeg/djpeg/jpegtran证明引擎行为，后续正式适配按此结果选择Rust包装与受控系数工具边界。
- 本机已有Visual Studio附带CMake，未发现NASM；实验明确关闭SIMD作为可构建的正确性基线，不将此配置或实验耗时当作最终产品性能结论。

## 2026-09-30 实施与问题处理

- 新增`tools/jpeg-lab.mjs`、固定引擎清单、项目原创C系数验证器/CMYK及YCCK生成器和自生成语料脚本，入口为`pnpm jpeg:lab:build`/`pnpm jpeg:lab:check`。未修改Cargo依赖、协议、核心任务池或已支持格式；实验不是产品JPEG功能。
- 官方不可变归档SHA256为`a577564110eb81045a9bea24aa3253d605dbc7bc304d683ffb2c40c67427ca75`。沙盒联网失败后经批准下载，逐字节核对哈希，再解压；初次手工解压后补同身份校验标识，后续入口拒绝合并没有标识的既有源码目录。缓存与工具在Git忽略的`.tools/jpeg-lab/`，实验输入/输出在每次新建的`target/jpeg-lab-*/`，不触碰用户图片。
- 首次用add_subdirectory遇到上游明确拒绝，改为独立构建上游、另构建项目验证器并链接其静态库；没有绕过上游断言或修改缓存源码。MSVC对UTF-8中文注释报C4819后显式使用/utf-8，项目验证器维持/W4 /WX（其他编译器Wall/Wextra/Werror）。上游原有转换/符号警告及旧CMake策略提示如实保留，不把第三方编译叫无警告。
- 固定Release/静态libjpeg API 6.2，关闭SIMD、TurboJPEG、Java、PNG输入依赖、12-bit和算术编解码；BUILD字符串固定为`6c9f0897afa1`，Windows动态CRT。配置与验证器源文件哈希写入构建身份；源/配置变化后check要求重建，报告另存工具二进制SHA256。该最小配置不代表正式编码性能或全部JPEG变体支持。
- 首次元数据严格断言暴露JFIF重建位置：把ICC插在JFIF前时jpegtran会将JFIF移到前面。规范语料按JFIF首标记要求生成，仍逐项比较全部APP/COM载荷与顺序；另保留非规范排列输入/输出的确定边界断言，未改为无条件排序比较来掩盖结构变化。
- 子进程仅处理自生成语料，单次30秒/输出64MiB限制；标准输入拒绝只接受1/2正常退出码，启动错误、超时、信号/原生崩溃分别失败。沙盒对管道子进程返回EPERM后经批准执行，不把启动错误当作损坏输入通过。
- 同步根README、实验说明及第三方依赖来源/许可，新增独立三平台CI job；上游源码通知未修改，实验二进制不进入桌面分发。CI仅上传自生成语料/报告，上传action固定到核对的v7.0.0提交。

## 2026-09-30 Windows实验结果

- 本机Node 26.9.0、Visual Studio 2022/MSVC 19.44.35213.0、Windows SDK 10.0.26100.0；使用Visual Studio已有CMake，未安装NASM/其他编码依赖。实际命令为`node tools/jpeg-lab.mjs build --cmake <已有VS CMake路径>`与`node tools/jpeg-lab.mjs check`，随后核对pnpm入口。
- 21项无损语料通过：baseline/progressive、灰度、YCbCr 4:2:0/4:2:2/4:4:4、原生RGB、CMYK、YCCK、奇数尺寸、单段/分段ICC、COM、合成APP11、EXIF1–8。每项系数/量化/组件/采样、解码像素、元数据载荷/顺序一致，输入文件SHA256保持。progressive/CMYK/YCCK样本无收益，证明提交仍需沿用NoGain保留原图策略。
- q=0/40/80/100均可重新解码，4项系数均发生变化。另9项q=80灰度/EXIF重编码验证尺寸、灰度组件数及Orientation载荷保持；只覆盖自生成方向字段，不承诺完整Exif缩略图/私有字段的复制策略。实际结果（192×128，原始baseline 13,762 bytes）：

| 路径 | 输出bytes | 编码值RMSE | 说明 |
| --- | ---: | ---: | --- |
| 系数无损 | 12,297 | 0（像素逐字节相等） | 同一系数/量化/采样 |
| 有损q=0 | 551 | 61.63 | 单一实验样本，不作画质承诺 |
| 有损q=40 | 4,020 | 16.73 | 同上 |
| 有损q=80 | 9,188 | 9.57 | 同上 |
| 有损q=100 | 34,545 | 0.53 | 系数变化且体积更大，不能叫无损 |

- 明确拒绝损坏/截断、单边16,385、4096×4096总像素超8Mi、12-bit输入、progressive超过1扫描、1024×1024输入在1m引擎预算下的系数转写。内存拒绝为`Backing store not supported`：对应上游jmemnobs虚拟数组预算不足；不是进程RSS硬上限或产品预算实现。正式探测/预约/执行仍需单独验证。
- 默认jpegtran丢ICC得到确定回归，`-copy all`不能省略；非规范JFIF顺序重建单独记录。合成APP11原字节保留但输出整文件哈希变化，不能将其称为真实内容凭据仍有效。
- 完整报告/输入/输出留在`target/jpeg-lab-ukBlkM/`（本机扩展语料验证批次；最终入口回归目录另在下节登记）。这里保留摘要而不将所有二进制或完整工具输出提交到Git。

## 正式接入决策与剩余门槛

1. 无损走DCT系数转写，有损独立重编码；先以官方工具的受控子进程适配为原型，避免将实验C标准错误处理接到主进程FFI。正式工具定位、子进程所有权、stderr有界/脱敏、取消后kill/wait与输出管道限额在核心适配任务实现，不能直接把本脚本用于用户输入。若换Rust包装，须单独验证panic/unwind与不受其保护的原生失败边界。
2. 初版普通8-bit灰度/RGB/YCbCr有损；ICC、CMYK/YCCK或未经证明的颜色语义采用系数保色无损回退或明确拒绝。当前ICC语料只验证载荷、不做色彩校准；不新增未经验证的色彩转换。Exif方向可保持原像素坐标与tag，但含缩略图/私有字段或未知标记不能依本实验静默复制，须有保守回退与验证规则。
3. JPEG含APP11/JUMBF或检测到内容凭据声明/引用时，默认保护拒绝优化；不沿用PNG caBX识别、不默默删除，也不以-copy all宣称签名有效。J0的APP11是假数据，不验证C2PA签名；以后开放显式移除确认须独立设计授权、真实语料及确认列表。凭据JPEG结构/字节绑定边界依据[C2PA硬绑定说明](https://c2pa.org/specifications/specifications/2.0/specs/C2PA_Specification.html)，不是逐系数相等即可保留。
4. J1只演进实际PNG/JPEG共用模型：`model/compression.rs`的PngRequest/报告、`model/quality.rs`的模式/实际处理、`batch/model.rs`参数、`import/model.rs`和planning、pipeline、batch/resources与output/paths。格式报告保留专属参数/验证；备份扩展名/扫描排除、输入探测与最终执行预算一起改，复用唯一任务池和当前安全输出层，不建立插件框架。
5. 新代码的A0三平台统一检查/桌面构建与J0三平台构建/实验结果是进入J1/J2的基线门槛。A1原生/视觉收尾继续用户手测；纯视觉项按阶段计划可明确移交，不将其重新变为JPEG实验或全部功能的长期阻塞。J2再更新DTO/协议/能力、混合批次、受限缩略图及真实方向展示。

## 验证与下一步

- Windows扩展实验21+4+9项通过；新增脚本格式/lint通过，`pnpm check`通过（155项前端、34份语料/SHA256、类型一致性、Clippy、Rust全套含105桌面与2 doctest）。收紧坏输入正常拒绝退出码及ICC确实丢失的断言后，最终`pnpm jpeg:lab:check`与`pnpm check`再次通过；最终隔离报告为`target/jpeg-lab-O86Y0d/report.json`。
- `pnpm jpeg:lab:build --cmake <已有VS CMake路径>`通过，验证入口及带空格参数转发，不安装系统编码器。10份相关Markdown的103个本地链接、全部23份变更/新增文本的行尾空白与NUL检查、`git diff --check`通过；最初shell链接解析得到0匹配未计为通过，改用确认实际匹配的Node本地检查完成验证。
- 未提交范围：`tools/jpeg-lab.mjs`与`tools/jpeg-lab/`、`package.json`两条实验入口、`.github/workflows/ci.yml`独立实验job、根README/第三方说明及本连续记录/阶段计划/索引；A0与A1交接改动另见各记录。缓存、下载源码、工具、生成语料/报告、检查助手及构建产物均由Git排除，未暂存任何文件。
- macOS/Linux未执行本轮代码；未触发旧SHA CI代替新验证。J1/J2尚未实施，正式JPEG能力未开放，用户PNG/参考仓库未修改，改动未暂存/提交/推送。下一步须在用户明确允许提交/推送后获取新SHA实际三平台结果，出现失败仍在当前任务修复。

## 2026-09-30 提交与推送授权

- 用户明确同意提交推送本轮A0/J0改动。提交范围包含固定源码/构建身份、自生成实验、独立三平台CI与说明/日志；不包含.tools、target、用户图片或产品JPEG接入。
- 源码与已验证状态一致，本次仅补交接文档，不重复运行完整测试。后续以新SHA对应的三平台CI实际结果验收，尚不宣称macOS/Linux通过或进入J1/J2。
- A0/J0业务提交为`d5b3445cfa9c4e30f1a8da82a448fedd66308097`（2026-09-30 16:11:44，Asia/Shanghai），`git push origin dev`成功，远端dev从317c6d6推进至d5b3445。暂存diff检查通过，提交包含实验入口、固定引擎身份/验证器、CI及相关说明，实验产物和引擎下载缓存仍留在忽略目录。
- [CI run36688238456](https://github.com/cct124/pixofold/actions/runs/36688238456)于16:12:07由该SHA触发，查询时in_progress；正式产品JPEG能力继续关闭。后续交接文档另行提交推送，可能按CI并发规则替代本次运行，最终HEAD三平台实验与应用基线全部通过后才进入J1/J2。
