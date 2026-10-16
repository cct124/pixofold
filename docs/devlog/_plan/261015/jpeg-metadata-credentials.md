# JPEG元数据兼容与内容凭据确认

- 创建日期：2026-10-15（Asia/Shanghai）。状态：本机实现、自动验收、MSI交付及用户手测反馈完成；2026-10-16正式日志复核完成。功能已包含在dev/e7b7b7a，本地origin/dev跟踪引用一致；新SHA三平台CI尚未在本记录核对。
- 开工基准：dev/12a34dc；保留前轮APNG v14全部32份未提交功能/测试/文档，不提交推送，不修改png-palettes。
- 用户授权：先实现JPEG元数据兼容与JPEG内容凭据处理；后者与已有PNG处理逻辑一致。

## 目标与验收

1. 扩展普通Exif/XMP等已识别JPEG元数据的有界结构校验，保留方向、颜色信息和原始字节，不静默清理GPS或未知元数据。候选与源的非凭据元数据仍须独立核对。
2. 已识别JPEG凭据允许导入，但默认不压缩：完整结构、其余元数据和真实解码检查通过后进入现有确认列表。混合损坏、未知保护元数据或资源错误不获得移除许可。
3. 复用PNG确认弹窗、全量同revision选择、冻结模式/输出/备份策略、单次来源版本授权和统一输出提交；确认后只去除已识别凭据，保留JPEG原文件名/扩展名。普通重试不继承移除授权，源变化必须重新检查确认。
4. 核心、扫描/批量/预算、协议/前端及日志同步；源安全、混合PNG/JPEG确认、取消/冲突/备份/NoGain、受限方向预览有真实或确定性回归。验证后提供新Windows MSI供用户验收，GUI与自动证据分别记录。

## 实施顺序

- 先定位JPEG format/engine/pipeline与PNG凭据链路，核对Exif/XMP/C2PA格式规范，明确已识别凭据与未知APP11边界。
- 实现元数据解析/验证及可移除凭据识别，再贯通来源授权、实际去除和重新验证；保持共用worker及原预算上限。
- 接入扫描候选、混合批次确认、报告和协议，更新文案/契约/测试；在同一确认列表中处理PNG与JPEG。
- 运行受影响回归、真实JPEG核心和部署验收、统一检查与Windows构建；最终更新所属文档、当前证据及未提交范围。

## 开工发现

- JPEG当前在扫描阶段拒绝APP11，APP1仅接受32字节单Orientation Exif；批量RetryJob与FormatOptions的元数据许可只用于PNG，前端JPEG报告禁止credentialsRemoved。须贯通全链路，不能只放开扫描或弹窗。
- PNG仅在完整解码且其他元数据通过时返回ContentCredentialsSource；源版本由路径/属性/SHA256绑定，confirm操作按真实失败行取授权，普通retry使用Preserve。沿用这些不变量。
- C2PA Manifest Store按JUMBF UUID和标签识别，JPEG APP11也承载其他标准；不能把任意APP11都作为可删除凭据。使用C2PA规范与Exif/XMP发布方文档核对，不引入网络读取图片或远程凭据下载。

## 2026-10-15 首轮实现与验证

- 新增jpeg/metadata：Exif大小端/IFD/标准子目录与数据范围校验，保留MakerNote原始载荷；标准UTF-8 XMP有界XML/命名空间解析，拒绝DTD、外链凭据和未支持扩展包；Photoshop资源有界检查，内嵌XMP同样检查。APP11按完整连续分片、JUMBF根UUID/标签和盒结构识别C2PA，其他APP11不授权删除。不做签名/证书真实性验证。
- 复用Cargo.lock已有quick-xml 0.42.0（MIT，无默认特性）作为核心直接依赖，仅新增锁文件依赖边；无新下载或版本升级。元数据总量仍限1MiB，Exif最多64个IFD/4096条目，XMP深度64/16384元素、每元素128属性，JUMBF深度16/4096盒；解析及分片重组计入工作集。
- MetadataPolicy共用策略并保留PngMetadataPolicy兼容别名，来源令牌增加PNG/JPEG格式绑定。JPEG默认先完整系数解码及源复查再返回待确认；确认时内存仅去除已识别APP11，实际落盘复验仍核对其余元数据、像素/系数及来源。普通重试使用Preserve，PNG令牌不能用于JPEG。
- v15桌面沿用同一确认DTO/弹窗/目录授权与输出策略，报告允许JPEG成功项标记凭据移除，NoGain禁止伪造已移除；通用文案不再写死caBX。增加PNG/JPEG混合确认和真实部署四种输出策略回归，仍在验证。
- 首轮4项元数据单元回归通过；pnpm jpeg:core:check通过，证据target/jpeg-lab-core-nznWBV，含原20项无损、92组合有损、混合批次及新增5项元数据/凭据输出独立系数/像素/标记复验。随后补充已入库的8种Exif方向真实照片，尚待新一轮执行。
- v15生成类型成功，2文件/66项前端定向回归通过。核心库首轮91/92通过：旧64MiB预算夹具已无法接纳增加元数据余量后的JPEG，串行调度用例等待超时；调整为96MiB并新增“各单项可容纳、总和超预算”的断言，保持测试语义，待复验。Tauri编译/格式化和pnpm本地数据库曾被沙箱拒绝，按相同命令提升权限后继续；未绕过门禁。

## 2026-10-15 收尾复核与部署验证

- 资源检查调整为先做元数据工作集准入，再解析XML/IFD或重组JUMBF；ICC合并检查放在段头校验之后，增加空/短APP2及MPF拒绝回归，避免未校验序号索引。Photoshop内嵌XMP同样拒绝外链凭据；XMP增加命名空间别名重复属性和非法字符检查。
- 首轮Rust Clippy发现计数循环、div_ceil、is_multiple_of和单Range测试写法问题，已直接修正，未关闭lint。按仓库固定Node26.9.0/pnpm12.5.1重新验收；前期全局Node24的输出只作中间证据，不作为最终工具链门槛。
- 新增8张已入库Landscape方向照片均通过真实预览方向1–8和无损处理（保持原件）；其结果为NoGain。target/jpeg-lab-core-j4i2mB保留新一轮核心结果及5项实际输出的独立验证。
- 下载固定上游contentauth/c2pa-rs提交9ae558e8477ac459cdf9ab0deaf735761f38e8bd的sdk/tests/fixtures/C.jpg及LICENSE-MIT，仅保留target/jpeg-public-261015，不入库或随包分发。输入132,518 bytes、SHA256 a2d14755db55de67a47c04090340d8266e892367be4104a45626d7a6fa6e9ffd；真实核心先待确认、确认后副本45,495 bytes。独立coeffdump/djpeg及标记比较确认系数/像素/非凭据元数据一致、原件不变，证据report.json。此项不验证签名真实性。沙箱内工具身份检查未成功，按原命令在允许环境复验通过，未放宽工具信任规则。
- jpeg:bundle:check通过，target/jpeg-bundle-IM3l2f：v15 PNG/JPEG共同确认，平铺副本/保留目录结构/覆盖备份/无备份覆盖四轮；含过期revision拒绝、冲突不写入、普通重试不继承许可及重新确认、8份凭据输出预览。首轮checker使用相对输出根与生产规范化路径比较导致“缺少JPEG行”，已规范化验收目录，不改生产路径逻辑。
- gif:bundle:check通过，target/gif-bundle-otypQS：同时包含上述JPEG四轮确认、既有GIF有损、APNG六轮四格式验收及资源独立降级。最终统一检查的格式/类型/170项前端/Clippy及核心各组已通过（含93项核心库）；桌面122/123通过，剩余失败是旧GIF测试写死协议14。改用统一协议常量并重命名该测试，待最后一次统一门槛。
- quick-xml原始MIT通知已保留tools/jpeg-lab/licenses/quick-xml-LICENSE-MIT，并作为licenses/资源随MSI分发；仅Cargo.lock核心依赖边变化，无依赖版本升级。根README、AGENTS、JPEG说明及UI规范同步v15和已实现范围；扩展XMP/外链凭据等边界保留。

## 2026-10-15 最终验收与交付

- 使用固定Node26.9.0/pnpm12.5.1完成最终`pnpm check`：15个前端测试文件/170项通过，Rust330项测试及2项doctest通过；格式、lint、类型、生成类型一致性、语料哈希和Clippy均通过。证据target/jpeg-v15-unified-261015.log。此前96MiB预算夹具、Clippy及旧协议常量问题均已收口。
- `pnpm jpeg:core:check`最终证据target/jpeg-lab-core-j4i2mB；`pnpm jpeg:bundle:check`证据target/jpeg-bundle-IM3l2f；`pnpm gif:bundle:check`证据target/gif-bundle-otypQS。真实核心含5项元数据/凭据输出独立系数、像素和剩余标记验证及8张方向照片；桌面后端包含PNG/JPEG共同确认的四种输出策略、过期revision/冲突/普通重试授权和预览。
- `pnpm desktop:build --bundles msi --ci`成功，构建日志target/jpeg-v15-msi-261015.log。交付副本为target/deliverables/261015-jpeg-v15/PixoFold_0.1.0_x64_en-US.msi，5,111,808 bytes，SHA256 `fe638b732eb1c8dbdd7069bb338fe04f41f7d1531968a25134fb46946d24a96c`。应用版本仍为0.1.0，v15为任务协议版本。
- WiX dark解包及target/jpeg-v15-261015/check-msi.ps1核对通过：25个文件、JPEG清单5项/GIF清单14项、quick-xml原始MIT通知已包含、开发checker未分发，主程序仅有UNK→MSI三字节预期补丁。报告target/jpeg-v15-261015/msi/report.json，installed=false；未将解包检查记为实际安装。
- 从上述MSI解出的资源执行`pnpm gif:bundle:check --resources E:/Project/pixofold/target/jpeg-v15-261015/msi/restored`，退出码0；证据target/gif-bundle-X8WnJH/report.json和target/jpeg-v15-261015/msi-resources.log。四种JPEG凭据输出、GIF有损与回退、APNG六轮四格式、独立资源降级及退出检查均通过，报告protocolVersion=15。
- `.temp/jpeg-v15-261015/inputs`准备8份手测副本：普通Exif/XMP/IPTC、JPEG与PNG凭据、未知APP11、不完整C2PA、坏JPEG熵数据、方向6照片及公开C2PA JPEG；`outputs`为空输出目录。README.txt说明预期，input-manifest.json记录每项来源与SHA256，公开样本附许可副本。未知/不完整/损坏样本预期拒绝且不得取得移除许可；03 PNG与02/08 JPEG应进入共同确认列表。
- 手测图片、公开下载原件、验收输出及MSI仅位于Git排除的.temp/target，未加入仓库；新增可复现夹具通过tests/support/jpeg_metadata.rs及部署checker生成。公开样本只验证容器识别、确认和图像保持，signatureAuthenticityVerified=false。
- 收尾修正根README残留的JPEG八份输入预算、APP11一律不可确认和旧协议版本描述，使其与v15代码及JPEG说明一致；此次构建后的变更仅为文档。`git diff --check`通过，工作仍在dev/12a34dc，保留前轮全部APNG改动，未提交或推送。
- 最终工作区共60份已修改或新增文件（包含前轮APNG）；本轮手测8张图片及1份许可文件的9项SHA256全部吻合，.temp和target排除规则有效。README与两份交接文档的74个本地链接均存在。额外对根README运行Prettier发现格式提示，经`git show HEAD:README.md`核对基线同样未通过；根Markdown本就不在仓库format:check入口内，本轮保留既有排版，未扩大为整篇格式重写，统一检查结论不受影响。

## 下一步与范围

- 用户已于2026-10-16反馈新MSI各项手测完成；当日正式日志、测试原件哈希及现存输出复核见下节。此前APNG手测结论保留，不重复要求整套基础功能。
- 功能现已包含在e7b7b7a，本地origin/dev跟踪引用一致；下一步核对该SHA三平台CI，不能沿用12a34dc结果为本轮背书。本次日志任务未查询远端CI，也未提交或推送验收文档。
- 普通Exif/XMP/Photoshop-IPTC的支持有明确有界结构范围；扩展XMP、外链凭据、未知APP/APP11及混合保护信息继续拒绝。当前不提供凭据签名验证、重签或普通元数据清理。

## 2026-10-16 新MSI手测反馈与日志复核

- 用户反馈已完成本轮MSI各项手动验证，授权检查相关日志。本轮先核对正式会话、构建/协议身份、凭据确认与任务终态、输出/备份、展示操作和退出健康，再更新结论；日志未记录的安装/视觉细节仅按用户反馈保留。
- 开始复核时工作区干净，当前dev/e7b7b7a已包含前轮APNG v14与JPEG v15实现；本地origin/dev跟踪引用同为e7b7b7a。此信息来自本地Git，本轮未提交、推送或查询远端CI。上一节“未提交”是2026-10-15交付时的历史状态。
- 检查正式日志目录中的两个新会话：run-1792118616882-L6MVJlY1fPkP.jsonl（123条，Asia/Shanghai 10:43:36–10:44:17）和run-1792118806390-w8E6ILF3em0a.jsonl（179条，10:46:46–10:52:18），共302条。均为Windows x86_64/release/0.1.0/protocol_version=15；JPEG/GIF引擎校验成功。build_source=unknown符合本地构建来源策略，因此仅凭日志不能绑定具体MSI哈希或e7b7b7a。
- 使用固定Node26.9.0运行`target/jpeg-v15-manual-review-261016.mjs`只读复核日志及手测文件，退出码0；报告`target/jpeg-v15-manual-review-261016.json`保留日志SHA256、逐任务/批次/确认链、展示结果、样本和现存输出摘要。序列无缺号或重复，任务启动与唯一终态匹配，提交顺序与批次统计一致，预约均在全局预算内；未发现ERROR、worker故障、提交失败、未知日志字段或私人路径字段。两次退出码均0，dropped_events/write_failures均0。
- 第一会话为5张PNG无损覆盖及5张PNG质量70有损覆盖，10项全部成功，策略均为无备份覆盖；6次缩略图成功。第二会话为两轮JPEG/PNG质量70混批：各接收6项、扫描拒绝2项，普通元数据JPEG与方向照片直接成功；每轮1张PNG/2张JPEG先待确认，再以相同行ID的attempt=2完成明确移除及成功提交。两轮分别为指定目录平铺副本和preserve_structure=true副本；6次凭据成功中PNG2次/JPEG4次。合计28个执行尝试为20成功、6次待确认、2次坏输入拒绝。
- 两轮最终各5成功/1失败；失败均为job3的ToolExit，没有获得凭据移除许可，也没有提交输出。结合确定性扫描顺序、其他五项输入大小、样本清单和现存输出，job3对应06-jpeg-corrupt-entropy.jpg；实际检查其首个SOS后熵数据为0，符合故意损坏样本预期。此关联结合本地素材推断，日志本身不含文件名。每轮2项unsupported_jpeg_credentials扫描拒绝与未知APP11/不完整C2PA两份负例一致。
- 共32次缩略图请求，22次成功；5次Unavailable均为上述损坏凭据JPEG行。另2次ResourceLimit为公开C2PA JPEG确认成功后的结果：2048×1365、4:4:4采样，按当前公式仅系数工作集已需34,668,544 bytes，超过预览32MiB（33,554,432 bytes），加像素工作区为51,441,664 bytes。压缩结果为44,133 bytes且已成功提交，预览拒绝属于现有限额，不是压缩失败；大图缩放解码预览未实现，本轮未放宽预算。方向6照片两次预览均成功，显示方向观感以用户反馈为准。
- 10:51:57另有3次FileMissing，均发生在第一轮平铺副本成功及先前预览成功之后；当前平铺产物也已不存在，第二轮保留结构的5份产物仍在。这与清理/移动第一轮产物相符，但日志不记录外部删除/移动操作，不能断言具体操作者或操作原因。没有同批输出提交失败或原件损坏证据。
- 8张测试原件及1份许可的9项SHA256全部匹配交付清单。现存5份结构化副本大小与第二轮日志一致；独立解析JPEG APP/COM及PNG chunk，确认非凭据元数据保持、两份JPEG凭据APP11和一份PNG caBX已移除、JPEG尺寸/采样保持。02仍在outputs/inputs/sub下且保留JpEg扩展名大小写；坏输入及扫描拒绝样本未产生副本。此次为有损70操作，不宣称源结果像素相等。
- 结论：用户反馈的Windows新MSI手测已登记，日志及文件证据未发现无法解释的压缩/提交或退出故障；大图预览限额和文件不存在提示已如实记录。两份日志没有覆盖备份、定位、冲突或普通重试事件；这些场景按用户总体手测反馈及此前自动部署证据分别保留，不能补写成此次日志证明。安装/卸载、主题/语言/DPI及视觉观感同样以用户反馈为准。
- 本轮仅更新本任务与devlog索引，复核脚本/报告位于Git排除的target；不修改功能代码，不重复构建或运行已通过的整套测试。新SHA平台CI留作后续独立核对。
