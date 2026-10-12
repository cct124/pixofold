# 公开图片手测原件

这套固定素材用于原生工作台、混合导入、压缩观感与格式边界手测。共 **32张、19,060,153 bytes（18.18MiB）**，最大单文件14,679,474 bytes。普通Git保存原始文件，首次提交并推送后，其他电脑clone/pull即可获取同一批素材。

图片采集于2026-10-12，源字节未转码、缩放或压缩；5份Pillow APNG仅改为.apng本地扩展名。这些第三方图片保留各自许可，与[自生成自动回归语料](../fixtures/README.md)分别维护。

## 日常使用与恢复

在仓库根目录使用项目锁定的Node/pnpm工具链，Windows、macOS、Linux采用相同命令：

~~~sh
pnpm samples:check
pnpm samples:prepare
~~~

- `samples:check`离线检查32张原件及4份许可的SHA256、图片大小和清单一致性，已接入`fixtures:check`及`pnpm check`。无需Python、Pillow或联网下载。
- `samples:prepare`先校验原件，再创建新的`.temp/manual-images-<随机标识>/`，输出可导入的绝对目录，含png/jpeg/gif/apng四个子目录。可在这份副本上进行覆盖、备份或输出冲突测试。
- 需要恢复测试图片时，再运行一次`samples:prepare`即可得到全新副本。旧测试目录及输出保留，便于比较；它们由现有.temp/规则排除，不进入Git。

**首次提交这些原件之后**，如果误把原件压缩或删除，可从仓库根目录恢复全部图片：

~~~sh
git restore --source=HEAD --worktree -- tests/manual-images/png tests/manual-images/jpeg tests/manual-images/gif tests/manual-images/apng
pnpm samples:check
~~~

该命令把这四个目录中的已跟踪图片恢复到当前提交，丢弃其未提交内容改动；不会清除压缩工具额外生成的未跟踪备份。原件尚未首次提交时，Git还没有可恢复基准，请使用已准备的工作副本手测。

## 内容与推荐入口

| 目录 | 数量 | 覆盖内容 |
| --- | --- | --- |
| [png](png/) | 8 | 透明/半透明、16位灰度、4位调色板、8位RGB、Adam7交错、照片 |
| [jpeg](jpeg/) | 12 | Exif方向1–8、灰度、ICC、CMYK、10109×4542全景大图；均为非渐进编码 |
| [gif](gif/) | 6 | 两张静态图、44帧旋转地球、三种帧清除方式的动画 |
| [apng](apng/) | 6 | 20帧弹跳球、OVER混合、恢复背景/上一帧、调色板透明、不同延时 |

代表图片：[透明骰子](png/PNG_transparency_demonstration_1.png)、[JPEG方向1](jpeg/Landscape_1.jpg)至[方向8](jpeg/Landscape_8.jpg)、[旋转地球](gif/Rotating_earth_large.gif)、[全景大图](jpeg/Fronalpstock_big.jpg)、[APNG弹跳球](apng/Animated_PNG_example_bouncing_beach_ball.png)。弹跳球保留.png扩展名，实际内容为APNG。

## 当前产品边界

- v11的GIF仅支持无损；有损模式的GIF逐行失败，切无损后重试。
- APNG尚未接入，6份APNG可用于不支持格式的拒绝/跳过行为及后续开发。
- pil_sample_rgb.jpg与Fronalpstock_big.jpg含ICC，pil_sample_cmyk.jpg为CMYK；当前JPEG预览对这些颜色边界有限制，大图也超过8Mi像素预览上限。
- dispose_prev.gif为5帧动画，首帧0ms；当前GIF核心严格拒绝零延时多帧，可用于验证保留源文件的失败路径。dispose_none.gif与dispose_bgnd.gif各5帧、每帧1000ms；旋转地球44帧、每帧90ms。
- 部分APNG含独立默认静态图；清单frames是可解码画面总数，animation_frames是acTL动画帧数，default_image标明两者差异。
- 32×32的PngSuite样本用于格式边界，不能代表真实照片压缩率。文件有效性、压缩结果、GUI和播放观感各自验收。

## 来源、署名与许可

[SOURCES.json](SOURCES.json)逐项记录作者/来源页面、下载地址、许可、用途、大小、尺寸、帧数/延时与SHA256；[SHA256SUMS.txt](SHA256SUMS.txt)保存图片原始字节的校验值。入库素材未改变像素、帧、元数据或压缩数据；图片沿用各自许可，不重新声明为本项目原创或GPL图片，也不进入桌面安装资源。

| 来源及素材 | 作者/归属 | 许可与证据 |
| --- | --- | --- |
| Wikimedia：PNG透明骰子 | Daniel G. / Ed g2s / CyberShadow | CC BY-SA 3.0；完整出处见逐文件清单 |
| Wikimedia：旋转地球 | Marvel；基于NASA图像 | CC BY-SA 3.0；本地只调整文件名 |
| Wikimedia：Fronalpstock全景照片 | Hannes Röst | CC BY-SA 3.0 |
| Wikimedia：APNG弹跳球 | Holger Will | 来源页作者公有领域授权 |
| PngSuite，libpng v1.6.50中4张 | Willem van Schaik | [使用/复制/修改/分发许可原文](licenses/PngSuite-README.txt) |
| Exif方向测试集，固定提交219294e144531b0c01247913cb58b6f5531b5081 | Dave Perrett / contributors | [MIT原文](licenses/Exif-orientation-LICENSE.txt) |
| Pillow 12.0.0中16张测试图 | PIL / Pillow contributors；具体原始路径见清单 | [MIT-CMU项目许可](licenses/Pillow-LICENSE.txt)，不笼统声称所有图片均为公有领域 |

三张Wikimedia图片选择源页面提供的CC BY-SA 3.0选项，附[完整许可正文](licenses/CC-BY-SA-3.0.txt)。作者、原始页面和许可链接随清单保留，源文件其他许可选项不用于重新许可本集合。

## 验证与维护

- 初次采集已通过Pillow 11.3.0的verify及逐帧load；PNG/APNG逐块CRC、APNG acTL/fdAT/动画帧数及Exif方向1–8均核对通过。此解码证据保留在开发记录，日常Node命令只读校验固定字节身份。
- 图片通过.gitattributes按二进制处理，许可原文关闭换行转换，保证跨平台checkout后的SHA256一致。
- 更新原件须同时核实来源/许可并更新SOURCES.json、SHA256SUMS.txt和本文计数；不要把压缩测试产物作为新的原件提交。
- 本集合规模固定；新增大量高清图片或频繁替换大图时再评估独立数据集或Git LFS。
