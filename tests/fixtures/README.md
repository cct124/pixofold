# 可复现图像语料

## PNG

全部由本项目自生成，适用 GPL-3.0-or-later；没有用户图片或第三方图片。来源、预期和 SHA256 见 [manifest](png/manifest.json)。生成器仅用于测试，不是产品 PNG 编码器。

在仓库根目录使用锁定的 Node 版本：

~~~powershell
node tests/fixtures/generate.mjs
node tests/fixtures/generate.mjs --check
cargo test -p pixofold-core --locked
cargo run -p pixofold-core --release --locked --example png_baseline
cargo run -p pixofold-core --release --locked --example png_quality_baseline
# 可选：保留结果供检查，目录必须尚不存在；省略参数则自动清理临时目录。
cargo run -p pixofold-core --release --locked --example png_quality_baseline -- --output-dir target/quality-review
~~~

- 正常样本：RGB/RGBA、灰度/灰度透明、1/2/4/8-bit 索引色、16-bit、Adam7、非零隐藏 RGB 和半透明边缘、tRNS。
- 显示元数据：gAMA、sRGB、pHYs、EXIF 方向、文本和自生成 ICC matrix/shaper 数据。验证原始 chunk 保留，不把此项等同于色彩管理或显示器校准验收。
- 边界：已优化微图、真实 PNG 的错误扩展名、假 PNG、CRC 错误、截断、非法 DEFLATE、尾随数据、两种 APNG 默认图布局。
- 共34个固定样本；5个192×128的整数确定性渐变/噪声图覆盖RGB、半透明、二值透明、显示元数据及gAMA=0.5。新增caBX/vpAG元数据保护边界样本：PNG结构及CRC有效，载荷为合成测试文字，不含真实签名，不用于C2PA凭据真实性验证。它们不是照片语料。
- 质量基线记录无损及 q=0/40/80/100 的实际体积、耗时、remapping 评分/回退原因，并独立计算黑白背景合成后的 RGB 编码值 RMSE（0–255 单位）和最大 alpha 误差；不把该指标称为线性光误差、SSIM 或感知质量。无损回退误差为零，但不算量化成功。
- 除微图外，正常样本故意使用未压缩 DEFLATE，便于稳定触发有收益分支；它们的缩小比例不能作为真实照片/素材的压缩率承诺。
- 基线在独立临时目录运行，不覆盖此目录的文件；覆盖/故障测试也只使用隔离临时目录。

## GIF

[GIF manifest](gif/manifest.json) 的45项由 [独立生成器](../../tools/gif-lab/fixtures.mjs) 确定性生成，适用 GPL-3.0-or-later。清单固定输入 SHA256、接纳或拒绝类别及人工给定的完整画布/厘秒时间轴；使用自写微型 GIF 编码器，不调用 Gifsicle 或独立验证器生成预期。`interlaced-dictionary.gif` 的固定 LZW 字典码流同样由本项目构造。

```powershell
pnpm gif:fixtures:generate
pnpm gif:fixtures:check
cargo test -p pixofold-core --locked --test gif_lab
pnpm gif:lab:build
pnpm gif:lab:check
```

- 27项有效输入：GIF87a/89a、静态/动画、全局/局部调色板、透明、偏移局部帧、dispose 0/1/2/3、首次 PREVIOUS、背景色初始画布、交错和跨行字典码、不等延时、无/有限/无限循环、重复画面合并、正延时透明空帧、循环接缝和模式图。修改过的延时、偏移、像素和注释也保留为有效输入，供语义负例比较。
- 18项拒绝输入：截断、尾随数据、零画布、帧越界、缺调色板、保留 dispose、用户交互、零延时动画、未知应用扩展、非法索引、坏 LZW/缺 EOI，以及输入/画布/帧/帧数/累计解码/元数据预算。预算样本以收紧测试额度触发，不创建巨型文件。
- 8组等价/不等价比较；循环接缝样本另有人工第二轮画布，证明最后一轮画布在下一轮仍参与合成。验证器存有界画布与摘要，不缓存全部 RGBA 帧。透明和逻辑背景分别比较，不宣称浏览器零延时钳制或 GUI 观感已验收。
- 优化实验另生成一项512×256、16帧模式压力图；固定语料与压力图都只复制到隔离实验目录。微型编码器故意不追求压缩率，这些素材的节省比例不能代表真实用户动画。

实验参数、资源范围、许可资料及实际结果见 [GIF实验说明](../../tools/gif-lab/README.md) 和 [连续记录](../../docs/devlog/_plan/261009/animation-foundation.md)。
