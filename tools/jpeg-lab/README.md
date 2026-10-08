# JPEG开发实验

本入口为下一阶段JPEG核心提供引擎行为和资源边界证据。实验独立于PNG任务与桌面IPC，当前产品仍只开放静态PNG。

## 构建与运行

需要工程固定的Node版本、CMake 3.15+、C编译器和tar。Windows使用Visual Studio C++工具链；macOS使用Xcode命令行工具；Linux使用系统C工具链。首次构建访问官方codeload并校验SHA256，之后复用经校验的缓存，不安装到系统。

```powershell
pnpm jpeg:lab:build
pnpm jpeg:lab:check
pnpm jpeg:core:check
# CMake不在PATH时可显式传入已有工具位置：
pnpm jpeg:lab:build --cmake 'C:/path/to/cmake.exe'
```

`engine.json`固定MozJPEG v4.1.5的提交与源码归档SHA256。构建为Release/静态libjpeg API 6.2和cjpeg/djpeg/jpegtran，关闭SIMD、TurboJPEG、Java、12-bit、算术编码及PNG输入依赖，Windows使用动态CRT；BUILD字符串固定到源码提交。SIMD关闭用于最小正确性基线，实验耗时不能代表产品性能。上游独立CMake配置，项目系数工具单独链接其静态库；不修改上游构建文件。CMake 4的旧策略兼容参数只用于上游配置。实验前核对源码/配置身份及系数验证器源码哈希，变化后必须重新构建；报告另记录工具二进制SHA256。

自生成原始PPM/JPEG和输出均写入每次新建的`target/jpeg-lab-*/`，源输入逐项核对SHA256。`report.json`记录引擎身份、平台、体积、耗时和拒绝边界。缓存/产物由Git排除，失败产物也保留供诊断。CI另设三平台实验job并保留自生成证据；配置存在不代表平台已经通过。

## 验证范围与接入门槛

- 21项baseline/progressive、灰度、YCbCr 4:2:0/4:2:2/4:4:4、RGB、CMYK/YCCK、奇数尺寸、EXIF方向1–8、单段/分段ICC、COM与不透明APP11；ICC来自本项目的PNG生成语料，仅验证载荷保持、不作色彩校准；APP11是合成字节，不是真实签名内容凭据。
- 系数层转写逐字节比较尺寸、组件/采样、量化表及有符号DCT系数，另比较解码像素与全部APP/COM载荷及顺序。`jpegtran -copy all`显式保留标记；默认参数会丢ICC，不能用于产品默认路径。额外构造JFIF前有ICC的输入，记录上游重建JFIF位置的边界，规范语料的顺序断言不因此放松。
- 有损q=0/40/80/100重新解码并记录编码值RMSE/体积/系数变化；另以q=80验证灰度组件数和8项Orientation元数据重编码保持。RMSE不是感知画质指标，q=100不能视为无损；方向语料仅含自生成字段，不代表任意Exif缩略图/私有字段可直接复制。这里尚未实现产品的ICC/CMYK保色、缩略图方向显示或凭据策略。
- 损坏、截断、单边/总像素、12-bit不支持、progressive扫描次数、引擎内存上限分别检查正常拒绝退出码；原生崩溃、启动失败、超时、输出超限单独判为实验失败。子进程30秒超时与64MiB输出限制是实验约束，不能代替产品资源预约与执行预算。
- 系数工具允许最多8Mi像素、单边16,384、64扫描、32MiB libjpeg内存参数。只处理脚本生成的实验输入；标准libjpeg错误终结子进程，不是安全的产品FFI。内存参数实际语义由实验和上游源码核对，不声明RSS硬上限。

J0实验已归档，持续证据见[开发日志](../../docs/devlog/_fin/260930/jpeg-engine-lab.md)。J1a新增单独的`pixofold-jpeg-helper`目标及Rust单文件核心，构建时记录helper源码/二进制SHA256；`core-check`使用上述共享自生成语料调用真实核心，并独立比对系数、像素和元数据。工具缺失或身份变化必须失败，不能静默跳过；三平台JPEG job已增加此入口，实际结果另记。

J1b第一段已新增单文件保守有损。core-check继续使用J1a无损回归，再运行jpeg_lossy_check及lossy-check.mjs：质量0/40/80/100、元数据/颜色回退、备份/副本/NoGain、源和合法候选被替换、冲突、取消与资源限制。独立djpeg/coeffdump验证已提交产物；lossy-report.json记录质量锚点误差/体积。另使用固定源码附带的testimages/testorig.jpg玫瑰照片，校验SHA256并在证据包保留原始README.ijg、LICENSE.md与来源说明。该227×149小照片仅用于最小真实内容观察，不代表完整照片集或色彩管理验收。

正式核心的生命周期、默认保护、严格Exif子集和安全输出边界见[JPEG核心说明](../../native/jpeg/README.md)，剩余单池预算/共用模型与桌面随包属于J1b第二段/J2。产品当前仍只开放静态PNG。
