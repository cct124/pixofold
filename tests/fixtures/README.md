# PNG 可复现语料

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
- 共 32 个固定样本；新增 5 个 192×128 的整数确定性渐变/噪声图，覆盖 RGB、半透明、二值透明、显示元数据及 gAMA=0.5。它们不是照片语料。
- 质量基线记录无损及 q=0/40/80/100 的实际体积、耗时、remapping 评分/回退原因，并独立计算黑白背景合成后的 RGB 编码值 RMSE（0–255 单位）和最大 alpha 误差；不把该指标称为线性光误差、SSIM 或感知质量。无损回退误差为零，但不算量化成功。
- 除微图外，正常样本故意使用未压缩 DEFLATE，便于稳定触发有收益分支；它们的缩小比例不能作为真实照片/素材的压缩率承诺。
- 基线在独立临时目录运行，不覆盖此目录的文件；覆盖/故障测试也只使用隔离临时目录。
