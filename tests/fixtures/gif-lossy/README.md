# GIF有损实验补充素材

本目录只提供开发实验输入，不是应用资源。公开原件及完整CC BY-SA 4.0许可按[SOURCES.json](SOURCES.json)固定SHA256，统一fixtures:check会只读核对；不重许可为本项目原创图片。

- `openrefine-wikidata.gif`：Pintoch的真实OpenRefine/Wikidata录屏，243,942 bytes，806×779、130帧；原始字节未改，来源与署名见清单。其累计解码超过当前生产验证额度，实验保留该拒绝，另以固定有界Gifsicle helper生成320×309衍生GIF，派生方式和原始许可跟随报告。
- 实验收益相对320×309衍生输入计算，不能将前置缩放节省归为有损编码收益；原始录屏不被覆盖。
- 其余输入复用[人工预期GIF语料](../gif/manifest.json)、[公开手测GIF](../../manual-images/README.md)，并由[确定性生成器](../../../crates/pixofold-core/examples/gif_lab/quality_fixtures.rs)生成渐变运动、透明插画、合成UI和照片平移动画到target。
- 照片平移以Pillow hopper.gif为父素材，12帧96×96裁剪，继承上游通知；合成UI明确不冒称真实录屏。新增样本都保留生成方法和原始哈希。
- 4B生成器另提供transparent-seam三像素/两帧循环最小样本，锁定末帧恢复背景改为保留画布后的第二轮残留；真实核心验收另在隔离目录生成固定种子暗部噪声，用于质量回退负例。均为本项目生成，不新增外部素材。

运行与验证边界见[GIF实验说明](../../../tools/gif-lab/README.md)。
