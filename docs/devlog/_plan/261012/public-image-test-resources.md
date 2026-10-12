# 公开图片手测素材收集

- 创建日期：2026-10-12（Asia/Shanghai）。
- 首次归档日期：2026-10-12（Asia/Shanghai）；同日按用户后续要求续开，保留收集过程。
- 状态：本机整理与验证完成；Git目录、命令及说明可审阅，尚未提交/推送。
- 基准：dev/9cccc2a；开工工作区干净。
- 用户要求：从公开网络搜集PNG、JPEG、GIF、APNG，在仓库.temp下新建目录，供本机测试。

## 目标与验收

- 素材落在.temp/public-image-samples-20261012/，按格式分目录；保留下载原始字节，不运行压缩或覆盖用户图片。
- 兼顾照片、透明、调色板、灰度/位深、Exif方向、静态/动画与帧合成；APNG核对动画控制块及多帧，不能把普通PNG误标为APNG。
- 保存逐文件来源、许可信息、大小、尺寸、帧数与SHA256；实际解码核对格式与完整性，说明当前产品能力与素材有效性是不同验收。
- .temp已被Git排除；不提交素材、不提交或推送代码。

## 进展与问题

- 已检索公开素材页及上游测试资源；优先保留来源和授权可追溯的文件。
- 沙箱内curl无法解析api.github.com；按工具规则提升联网只读查询，成功取得Pillow 12.0.0公开素材清单。
- 首轮取得29张；3个PngSuite下载任务在写入前失败。补充错误上下文、预创建格式目录后再次获取，全部32张通过；没有覆盖已下载的原始图片。

## 交付与验证

- 目录：.temp/public-image-samples-20261012/。PNG 8张、JPEG 12张、GIF 6张、APNG 6张；图片合计19,060,153 bytes（18.18MiB），32个SHA256互异，无.part残留。
- 来源：Wikimedia Commons的透明骰子、旋转地球、全景照片、APNG弹跳球；libpng v1.6.50所附PngSuite；Pillow 12.0.0测试图片；固定219294e144531b0c01247913cb58b6f5531b5081的Exif方向测试集。原始字节保留，五份Pillow APNG仅在本地改用.apng扩展名。
- README.md提供目录、代表样本、手测入口与能力边界；SOURCES.json逐项记录下载/来源/许可、尺寸、模式、帧数/延时、原始哈希；SHA256SUMS.txt保留下载字节清单，licenses/保存三个上游许可说明。Pillow只记录项目许可，不笼统声称所有测试图均属公有领域。
- python .temp/public-image-samples-20261012/collect.py下载与验证退出0；使用既有Pillow 11.3.0，未安装新依赖。全部图片verify及逐帧load通过；PNG/APNG逐块CRC、APNG acTL/fdAT及动画帧数、JPEG Exif方向1–8均已核对。
- python .temp/public-image-samples-20261012/collect.py --verify-only退出0；只读比较原始SHA256和解码，不修改图片与清单。样本README的17个本地链接检查通过，git diff --check通过。
- 保留真实边界：GIF dispose_prev首帧0ms；CMYK与ICC JPEG；10109×4542大图；部分APNG默认静态图独立于动画帧。已在README说明当前GIF仅无损、零延时多帧严格拒绝、APNG未接入及JPEG预览限制；素材有效性不等于应用已支持。
- 本次未运行PixoFold压缩、GUI或完整工程测试；无业务代码改动，不复跑既有构建。git check-ignore确认素材目录按已有.temp/规则排除；仅开发索引及本记录为未提交文档修改，未提交或推送。

## 下一步

- 用户可导入png/jpeg/gif目录做现有格式手测，apng目录用于不支持格式行为及后续动画开发。重复比较可用指定目录副本输出，保留下载原件。
- 素材收集已完成，本任务归档；GIF第三阶段的安装/GUI和平台验收继续由原动画任务记录。

## 2026-10-12 续开：版本管理与跨电脑复用

- 用户提出将这套素材交由Git记录，以便恢复原件并在其他电脑开发时复用。本轮先整理可审阅的目录与命令；尚无新的明确提交或推送指令。
- 开工仍为dev/9cccc2a，保留前轮未提交索引与本记录；.temp中的32张下载原件仍在。
- 方案：固定原件复制到tests/manual-images，保留逐文件来源/许可/哈希；普通Git保存18.18MiB有限语料，最大单文件约14MiB。补充二进制属性及Node离线校验/隔离副本命令，使用项目已有Node工具链，无需额外Python依赖或重新联网下载。
- 验收：32张原件逐字节与下载哈希一致，复制出的新工作目录同哈希；工作副本改动后可重新准备，不覆盖既有测试结果。根README与第三方说明同步，恢复原件的Git命令说明首次提交前后差异。

### 本机实施与验证

- tests/manual-images保留32张图片、SOURCES.json、SHA256SUMS.txt及4份许可正文；补充CC BY-SA 3.0官方原文，Commons API再次核对4张素材的作者和许可均与清单一致。复制前后32张SHA256与原下载清单一致；原.temp采集目录保留。
- 新增tools/manual-images.mjs和pnpm samples:check/prepare。check仅离线检查图片/许可原件、大小与双清单一致性；prepare先验证全部输入，再在.temp独占创建新目录，以校验过的内存字节写入32张工作副本。不会覆盖先前测试目录；原件有变化时拒绝准备。只用Node标准库，没有新增依赖。
- samples:check接入fixtures:check，因此统一检查/CI也会校验这些固定原件；此检查不是再次解码或GUI验收。原件的首次逐帧解码证据沿用上节。
- .gitattributes增加JPEG/GIF/APNG二进制属性，并禁止4份许可原文的换行转换；git check-attr确认四类图片text/diff/merge均unset，许可text为unset，避免跨平台checkout破坏字节哈希。普通Git保存此有限数据集，不引入LFS。
- pnpm fixtures:check通过：34项PNG、45项GIF确定性语料及32张手测图片/4份许可。新增工具的Oxlint --deny-warnings和Prettier检查通过。首次pnpm在沙箱报unable to open database file，按规则提升相同检查后通过；没有绕过或关闭检查。
- target/manual-images-verify-261012.mjs通过：两次prepare得到不同目录，64张副本均核对原始哈希；对首份副本模拟覆盖后，再次prepare不改变旧副本，新副本与原件仍完全一致。验证结束已把模拟改动恢复；最近可用副本为.temp/manual-images-tZhGRN/。
- 根README、第三方说明、素材README及旧.temp说明同步，明确首次提交后才能git restore已跟踪原件，跨电脑共享依赖提交/推送。未运行GUI/压缩、未复跑无关Rust或桌面构建，也未提交或推送；实际其他系统checkout/运行仍待后续CI或开发机验证。
- 最终diff已复核，git diff --check通过；5份新增/修改Markdown的94个本地链接、NUL与尾随空白检查通过。git ls-files确认素材目录39份文件可正常纳入Git，工作副本按.temp规则忽略，暂存区为空。

### 交接

- 变更范围：.gitattributes、package.json、tools/manual-images.mjs、tests/manual-images整套原件与说明、README/THIRD_PARTY_NOTICES、开发索引及本连续记录；源码原件目录和脚本当前尚未跟踪，未暂存。
- 用户随后要求提交推送，本轮结果已提交737f622并推送origin/dev（9cccc2a..737f622），其他电脑clone/pull即可复用；也可继续用samples:prepare反复获得干净副本。

## 2026-10-12 编写提交信息、提交与推送

- 用户要求根据工作区内容编写提交信息、提交并推送，无需执行其它任务。复核范围：46份文件（tests/manual-images 39份：32张图片/4份许可/SOURCES.json/SHA256SUMS.txt/README；新增tools/manual-images.mjs与本计划；.gitattributes、package.json、根README、第三方说明与索引），无未暂存改动与未跟踪残留，git diff --check通过。
- 提交737f622「feat: 纳入手测公开图片素材与离线校验命令」（46 files changed、1727 insertions、1 deletion）。沙箱内.git只读，add/commit/push按规则提升同一条命令执行，未绕开沙箱约束；9cccc2a..737f622  dev -> dev已同步origin/dev，本地与远端一致。
- 沿用本轮已完成验证（fixtures:check含34项PNG、45项GIF与32张手测图片/4份许可；两次prepare副本哈希与原件一致且互不覆盖；Oxlint/Prettier通过；此前collect.py的逐帧解码与Exif方向证据保留在上节）；本次未改业务代码，不重复下载、解码或构建。其他系统checkout与运行仍待后续CI/开发机验证，不据此声明通过。
