# JPEG可信随包引擎与隔离运行（J2第一段）

- 创建日期：2026-10-09（Asia/Shanghai）。
- 状态：代码实现、本机自动回归、正式MSI与包内资源隔离验证完成；待真实Windows安装/GUI启动/卸载及新代码三平台验收，保持活动。桌面JPEG工作流属于J2第二段。
- 分支/基准：dev/7863652，业务基准681f050；本轮实现已提交6fc7198并推送origin/dev（7863652..6fc7198），开工前的索引、阶段计划、J1b平台进展三份文档随同入库。
- 关联：[阶段计划](../260930/next-development-plan.md)、[J1b混合核心](../../_fin/261009/jpeg-mixed-batch.md)。

## 目标与验收

1. 生产与实验共用固定MozJPEG源码/构建配方，生成目标专属helper、身份及许可资料；运行不依赖.tools或开发PATH。
2. 构建时验证产物/来源并把预期哈希编入Rust；运行时只按固定资源布局加载，旁置清单不能改变信任。
3. 接通Tauri开发/正式构建资源，PNG能力在JPEG资源异常时保持可用；JPEG桌面入口继续由第二段完整迁移后开放。
4. 生产加载器在仓库外、中文/空格路径和受限环境完成真实无损/有损/NoGain；覆盖工具缺失、替换与现有取消/超时回收。
5. 统一检查、真实JPEG回归、Windows正式安装包构建；安装/启动/卸载与无开发环境验证分别记录实际证据，不把构建等同GUI验收。

## 2026-10-09 开工与设计

- 当前HEAD的CI run37878230376已有三平台JPEG和macOS/Ubuntu应用jobs通过；Windows统一检查已通过、桌面构建进行中。最终结果另续记J1b。
- 提取共享构建模块，Windows使用静态CRT配合静态MozJPEG，避免依赖开发机VC运行库；原配置改变后必须重建并跑完整JPEG回归。
- 固定资源布局承载生成产物，普通无工具debug检查可保持PNG-only；正式release构建缺少或过期资源应失败，Tauri构建钩子负责准备资源。预期SHA256从构建输入生成到OUT_DIR，运行时清单仅用于追溯。
- 通过独立无GUI验证程序复用生产加载模块，参数只用于开发验收且不新增IPC。实际桌面由应用持有验证后的ImageEngines，为后续单实例注入准备。
- 本阶段不提交推送；GUI/安装手测使用隔离环境或用户执行，保留明确未验证边界。

## 实现与验证进展

- 新增共享build.mjs，实验与桌面使用同一原生目标/源码/配方；新的源码缓存从校验归档解压，再逐文件复核。Windows使用静态CRT，构建目录与旧动态CRT缓存分离。桌面dev/build钩子prepare，helper/许可/来源清单映射至jpeg/runtime；生成资源忽略入库。
- 新增build-support/jpeg.rs，校验schema/目标、固定源码清单、工具及许可字节，复用已验证的同一份字节生成OUT_DIR哈希；缺工具仅普通debug允许，release拒绝。复用现有sha2/serde_json，Cargo.lock只增加桌面sha2依赖关系，无新版本。
- 应用setup使用Tauri资源根和固定布局加载，拒绝目录链接/reparse point和工具身份变化；应用持有ImageEngines，PNG任务仍使用v9兼容路径。失败只记录resource_directory/tool_io/tool_identity稳定类别，不记录原始错误或路径。
- 无GUI的jpeg-bundle-check特性程序复用生产加载器；Node入口复制资源/程序至系统临时中文空格路径，清空开发环境，验证真实无损/有损/NoGain、预取消和原图不变，再验证工具缺失和伪造清单不能授权替换。实验工具独立解码、无损系数和标记复验输出。CI应用job新增此入口及证据留存，结果随本轮推送后的新SHA执行。
- pnpm check通过：155前端、43核心单元、完整集成回归、110桌面、2 doctest、34语料、生成类型与全目标/全特性Clippy。首次pnpm受沙箱数据库限制，按规则提升后完成，无跳过/豁免。日志target/j2-check-261009.log。
- 静态CRT的jpeg:lab:check通过21项无损、4质量锚点、9灰度/方向等边界；jpeg:core:check通过20无损、92有损/回退组合、67实际输出以及混合入口与4份混合JPEG独立复验。证据target/jpeg-lab-CwIV9B、target/jpeg-lab-core-5KlNfB；日志target/j2-jpeg-lab-261009.log与target/j2-jpeg-core-261009.log。
- helper为685,056 bytes，SHA256 8fc117b4851ae02d405995361eddccee646b3307a7584ef15fc62fe1d1acfd60；dumpbin /DEPENDENTS仅显示KERNEL32.dll。隔离部署入口通过，target/jpeg-bundle-X9w6N6/report.json记录输入13,762 bytes、无损12,883、有损4,447，以及NoGain/取消/源不变/替换拒绝。此证据不是在全新Windows系统实际安装/启动的证明。
- pnpm tauri build --bundles msi --ci已生成正式Windows安装包；后续包内资源验证与产物身份见下节。

## 正式产物与安装包资源验收

- pnpm tauri build --bundles msi --ci通过，Vite生产构建与Rust release成功；Tauri获取并校验WiX 3.14.1后生成MSI，日志target/j2-desktop-msi-261009.log。
- target/release/pixofold.exe：0.1.0，10,779,648 bytes，2026-10-09 11:55:25（Asia/Shanghai），SHA256 74A5ED8F4AE17604E37B9A03108A8522E9194391CEFCBC96FE0B6E123D85BB9F。
- target/release/bundle/msi/PixoFold_0.1.0_x64_en-US.msi：4,055,040 bytes，11:55:21，SHA256 472128C09BED631F826BD60333A7E47AFEEA7B7AD29CEBD825376BA13E516FEE。默认安装清单包含主程序、helper、身份清单、许可/IJG致谢与资源说明，不含实验编码器或验收程序。
- 用msiexec /a /qn管理解包到新建的target/j2-msi-extracted-261009，退出码0；这不是产品安装、GUI启动或卸载测试。包内helper SHA256与构建身份一致。包内主程序SHA256 5CDDCE0C52E0DC21608784C1B597433B98160AC13CA8F3534CE8407582B1B5C7；与未打包EXE等长，只差Tauri写入的3个ASCII字节UNK→MSI，单独记录两种产物身份。
- pnpm jpeg:bundle:check --resources E:/Project/pixofold/target/j2-msi-extracted-261009/PFiles/PixoFold通过：把实际解包资源复制到仓库外，再用同一生产加载器执行无损/有损/NoGain/取消/替换拒绝，两个产物通过独立系数/解码/标记验证。证据target/jpeg-bundle-HfqAoV/report.json，日志target/j2-msi-resources-check-261009.log。临时目录保留验收材料，不影响用户原图。

## 剩余验收与下一入口

1. 在无Node/Rust/CMake/Visual Studio的Windows隔离系统或由用户执行真实安装/启动/卸载；用当前MSI，记录WebView2准备、打开空工作台、纯PNG样本处理和正常退出。在诊断日志确认jpeg_engine_verified；资源异常的降级另用隔离副本核验，避免改动现用安装。当前未操作用户GUI，没有取得此项证据。
2. 新代码的三平台构建/部署验证待用户明确授权提交推送后按新SHA核对；7863652全绿只完成J1b，不能覆盖本轮。macOS/Ubuntu实际安装、签名后工具身份及公开发行源码资料各自验收。
3. J2第二段从应用共享引擎注入、实际能力及协议/生成DTO演进开始，再接混合导入/重试/报告、方向缩略图与原生闭环；当前产品仍只开放PNG/v9，不把helper随包写成JPEG工作台完成。
4. 本轮变更含共享构建/暂存脚本、构建期身份和加载模块/回归、无GUI部署入口、Tauri资源/钩子及CI、文档与J1b归档；未修改参考仓库或用户图片。改动已提交6fc7198并推送origin/dev，见下节。

## 最终交接检查

- 最终pnpm format:check（含cargo fmt --check）与pnpm lint通过；新增--resources参数已实际用于MSI包内资源验证，最后的核心改动仅纠正过期API注释，没有重复无关功能测试。日志target/j2-final-static-261009.log。
- 已复核diff与新增文件；git diff --check通过，10份变更/新增文档125个本地链接有效、无NUL。生成TS与pnpm-lock.yaml无diff，Cargo.lock仅复用已有sha2版本的桌面依赖关系。J1b归档与链接更新保留完整历史。
开工基准dev/7863652，本轮改动已提交6fc7198（7863652..6fc7198）；下一验收入口为上述MSI的真实安装/启动/卸载与新代码三平台，J2第一段记录继续活动。

## 2026-10-09 编写提交信息、提交与推送

- 用户要求根据工作区内容编写提交信息、提交并推送，无需执行其它任务。复核范围：28份文件（含J1b记录从_plan迁至_fin、新增jpeg-desktop-bundle计划与resources说明、build-support/jpeg.rs、jpeg_bundle加载模块、bundle-check程序与两个Node脚本、共享build.mjs；其余为钩子/资源映射/CI/依赖与文档），无未暂存改动与未跟踪残留（生成的runtime资源按设计由.gitignore排除），git diff --check通过。
- 提交6fc7198「feat: 接入JPEG可信随包helper与无开发环境部署验收」（28 files changed、975 insertions、173 deletions）。沙箱内.git只读，add/commit/push按规则提升同一条命令执行，未绕开沙箱约束；7863652..6fc7198  dev -> dev已同步origin/dev，本地与远端一致。
- 沿用本轮已完成验证（pnpm check：155项前端、43项核心单元与完整集成回归、110项桌面、2项doctest、34份语料与类型一致性；静态CRT的jpeg:lab:check 21项与jpeg:core:check无损20项/有损92组合67输出/混合入口；11:55:25 EXE与11:55:21 MSI及包内资源隔离验收）；本次未改业务代码，不重复构建或测试。真实Windows安装/启动/卸载与三平台结果仍待验收，不把本机与历史CI结果写成新代码通过。
