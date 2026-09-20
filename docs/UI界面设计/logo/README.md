# PixoFold（轻图）Logo

先使用内置生图模型，以 png-palettes 的原 logo 为参考生成设计图，再按用户要求将生成图复刻为圆角 SVG。

- 延续蓝白配色，用蓝色渐变圆角方形作为应用图标底形，圆角之外透明。
- 白色图片符号保留对图片工具的直观识别，浅青色折角呼应 PixoFold 中的 Fold。
- 简化原来围绕文件的四个箭头，采用较大的几何形状与留白。
- 图形不包含文字，适合与界面中的「PixoFold · 轻图」名称组合使用。

主要文件为 **PixoFold-logo.svg**，由矢量路径、圆形、圆角矩形和渐变构成，不嵌入位图。`viewBox` 为 `0 0 1024 1024`，可按需要缩放；同时提供 **1024×1024 透明 PNG** 预览。

已检查亮、暗背景，以及 32、64、128 像素显示效果；SVG 外缘由几何轮廓决定，不沿用生图透明稿的边缘杂色。

| 文件 | 用途 |
| --- | --- |
| [PixoFold-logo.svg](PixoFold-logo.svg) | 圆角矢量源文件 |
| [PixoFold-logo-圆角.png](PixoFold-logo-圆角.png) | 从 SVG 渲染的透明背景预览 |
| [PixoFold-logo.png](PixoFold-logo.png) | 1254×1254 生图原稿，保留为复刻参考 |
| [原始参考 SVG](参考-png-palettes-logo.svg) / [参考 PNG](参考-png-palettes-logo.png) | png-palettes 的原始 logo 与渲染参考 |
| [完整生图提示词](生图提示词.md) | 生图过程与后续矢量复刻记录 |

![PixoFold 圆角 Logo](PixoFold-logo-圆角.png)
