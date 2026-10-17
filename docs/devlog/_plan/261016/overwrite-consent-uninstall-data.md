# 首次无备份覆盖确认与卸载数据清理

- 创建日期：2026-10-16（Asia/Shanghai）。状态：实现、自动门槛及Windows MSI交付完成，待用户新包手测。
- 基准：dev/2af6c09，开工工作区干净。用户授权实施、验证并构建Windows MSI；本任务未授权提交推送。

## 目标与验收

1. 当前用户首次实际启动无备份覆盖前明确确认；取消不启动、不记住确认、不自动再次弹出，保留导入列表。确认记录独立持久化，重启/升级/清空列表不重置，删除应用数据后重新提示。覆盖选择/目录/拖放/Ready启动/重试/凭据确认入口，按实际冻结输出策略判断。
2. 先验证现有MSI各卸载入口与界面级别，再增加默认不勾选的当前用户应用数据删除选项。清理设置、确认记录、应用WebView缓存和日志；保护图片、输出、备份、其他用户数据与共享运行时。升级和静默卸载保留数据。
3. 完成受影响前后端、持久化、竞态及卸载条件回归，检查中英文/亮暗/小窗口与键盘操作；完整检查和真实包内验证通过后构建新的Windows MSI。区分自动验证、实际安装/卸载及GUI证据。

## 初始定位

- 设置默认overwrite且backupBeforeOverwrite=false；现有导入可直接携带设置自动启动，不能仅在显式start按钮前确认。重试沿用原任务输出，凭据确认可以另选输出；需覆盖所有原生接纳入口。
- 语言/主题与压缩偏好当前保存在WebView localStorage，正式日志位于系统temp_dir()/pixofold-logs。默认MSI使用perMachine与WixUI_InstallDir，尚无应用数据清理UI。
- Windows Installer的完整/基本/静默UI能力不同，不能把自定义对话框存在当成系统设置入口已显示；升级可能执行旧包卸载，删除必须显式排除升级及静默入口。

## 实施顺序

1. 设计并接入持久确认与原生启动边界；完善弹窗、取消/继续和回归。
2. 检查实际MSI表、卸载入口和安装范围；实现限定当前用户的清理流程并验证默认、取消、升级、静默及文件安全。
3. 运行统一检查和包内验证，构建并核对新MSI；更新本记录、README及索引，明确剩余真实手测范围。

## 2026-10-16 首次确认实现与专项验证

- 协议升级v16：未确认的无备份覆盖导入仅扫描，接纳返回可识别延期标志；start、普通重试及凭据确认按实际输出策略原生拦截。新ACK会话专属命令在后台原子保存固定确认标记，路径仅由桌面解析，保存失败不启动；旧客户端必须重载握手。
- 前端保持首次导入冻结设置；等扫描有候选后弹窗，取消保留列表并暂停自动启动，提供明确开始按钮。确认持久化后提交原操作，取消/Escape/失效会话不写确认。修复初轮发现的pending期间取消按钮不可聚焦问题。
- 前端Workspace与TaskActions共71项回归、TypeScript检查通过；原生7项专项通过，覆盖重启/删除标记、损坏/不可写存储、硬链接保护、真实扫描和压缩、权限、普通重试及凭据确认。
- 初轮Rust测试补齐显式测试target的模块导出；Rust1.98.1增量缓存出现ICE，专项改用CARGO_INCREMENTAL=0重编译。沙箱文件身份/硬链接权限导致6项PermissionDenied，按同一命令提升权限后7/7通过，未跳过测试。后续完整门槛仍待执行。
- 下一步以独立安装身份探测MSI全UI/基本UI/系统设置/控制面板/静默与升级；生产卸载尚未改动。

## 2026-10-16 卸载入口探测与手动交接

- 独立探测包位于target/overwrite-consent-261016/installer-probe/probe.msi，显示名PixoFold Installer Probe，ProductCode=f0d81c2b-a697-472f-9ea7-dee507c63b05。包仅含一个测试文本文件，使用独立UpgradeCode及当前用户安装范围，不替换正式PixoFold；探测UI入口不等同正式perMachine跨用户卸载验收。
- WiX3.14编译和ICE校验完成；ICE61来自与现有配方相同的允许降级规则，ICE91对应探测包有意使用的当前用户目录。沙箱无法访问Windows Installer服务，按相同命令提升权限后完成。msiexec初次使用正斜杠路径失败，改为Resolve-Path得到的Windows原生路径后静默安装退出码0，日志install-native-path.log。
- computer-use技能及相关约束已读取；工具明确拒绝自动访问Windows“设置”，未执行卸载或修改该窗口。用户要求提供操作步骤并自行验证。当前探测包仍安装，删除应用数据功能尚未加入。
- 用户手动检查：系统设置的已安装应用、控制面板的程序和功能、直接运行已安装的MSI三个入口；每次卸载后重装探测包再检查下一个入口，反馈完整向导/系统确认及进度/异常的实际表现。之后继续实现正式卸载选项及安全清理，完成完整门槛与新MSI构建。
- 首次确认已通过71项前端及7项原生专项；后续完善了Ready状态文案和旧会话弹窗拒绝，完整检查及视觉验收仍待执行。所有改动未提交推送。

## 2026-10-16 手动反馈与卸载实现

- 用户已手测：Windows设置和控制面板仅有确认框/进度窗口，直接运行已安装MSI进入维护向导，三条路径均卸载成功。该证据来自独立探测包；用户授权继续清理功能并构建新Windows MSI。
- 采用WiX片段嵌入原生MSI动作：InstallInitialize前在基本/完整交互UI统一显示原生TaskDialog，默认不勾选且默认焦点取消；仅InstallFinalize成功后清理。升级及UILevel≤2均重置选择并保留数据，取消返回1602且不执行清理。无需修改系统卸载入口或复制整份Tauri模板。
- Windows原生模块按发起用户SID与执行线程/进程令牌复核，根目录由Known Folder解析；仅处理EBWebView、首次确认标记、受限日志定位记录及已识别且未被占用的日志。目录句柄在遍历期间阻止替换，拒绝重解析点，不修改只读属性，不遍历其他用户资料或任意输出目录。零字节日志协调文件保留以避免活动实例持旧锁时失去互斥。
- 新增installer:prepare/check及MSI构建钩子，MSVC静态CRT、/W4 /WX编译，DLL仅嵌入MSI；测试预览与探测DLL独立构建不分发。首轮103项原生检查通过，覆盖实际文件删除、静默/升级条件、SID、硬链接、目录junction与活动日志保护；真实MSI接线及完整门槛继续验证。

## 2026-10-16 完整自动门槛与手测准备

- 前端179项、格式/lint/TypeScript/语料/生成类型一致性通过；Clippy与Rust341项测试及2项doctest通过。新日志定位记录覆盖Windows原始UTF-16（含孤立代理单元），与覆盖确认独立。
- 初轮完整检查因JPEG共用构建脚本新增findCmake导出导致资源配方身份过期而阻断，按固定配方重新prepare JPEG/GIF后继续。Clippy发现测试中的固定chunks_exact写法，改为as_chunks；DTO字段说明移到类型文档，避免生成器产生行尾空格。未关闭检查或跳过用例。证据为target/overwrite-consent-261016/full-check-final.log和remaining-check.log，前者保留首次Clippy失败，后者记录修正后的剩余全部门槛。
- 最新原生103项通过，target/installer-cleanup-UUostq；DLL SHA256为27cba26d7f17381dd56b87524b25ab7b976dc804631b1c8350cd7a4d3f46a7b4。真实独立MSI当前用户安装、带预置删除属性的静默卸载、major upgrade及最终卸载均保留测试数据，target/installer-msi-MeQI4l/result.json明确scope=perUser。
- 补测perMachine时真实自定义动作已验证用户SID匹配，但安装以1925（缺少管理员权限）/1603失败，不能记为全机安装通过，证据target/installer-msi-cANj2k/install.log。失败留下的本轮测试文件经哈希核对逐项移除，未触及正式PixoFold。脚本新增显式--per-machine及管理员预检，默认仍独立perUser范围。
- 用户要求亲自手测，新UI自动预览未取得有效结果，后续不再进行Windows界面操作；原生新弹窗、正式包三入口/勾选删除/取消以及亮暗、中英、窗口适配留作用户实际验证。旧探测包的三入口反馈不能替代新弹窗验证。
- 已创建可覆盖素材.temp/manual-images-2W4KSz（32张完整副本），重点可用png/uncompressed_rgb.png和jpeg/Landscape_1.jpg。Windows MSI构建中，交付后提供按顺序的手测步骤。全部修改未提交推送。

## 2026-10-16 新MSI交付与手测清单

- pnpm tauri build --bundles msi --ci通过，正式包为target/deliverables/261016-overwrite-uninstall/PixoFold_0.1.0_x64_en-US.msi，5,296,128 bytes，SHA256=03eaf59c06f8a5f945191290154d6bd91d8b237ed0be4cedd302b0c52dd26d4d。应用仍为开发版本0.1.0，协议v16；本机修改未提交/推送，不引用旧CI作为本轮证据。
- MSI管理解包完成；嵌入原生DLL哈希与103项测试产物一致，只有两个生产动作导出、仅依赖Windows系统DLL，未分发preview/probe。包内exe SHA256=0917a1fe69d92249e2a6f5216208a8f624318cfe9db716503c978827ac218496，与构建exe仅Tauri包类型标记MSI/UNK三字节不同。
- 直接只读查询MSI表确认：ConfirmPixoFoldDataRemoval=1499、InstallInitialize=1500、RemoveExistingProducts=1501、InstallFinalize=6600、RemovePixoFoldDataAfterUninstall=6602；清理条件包括非升级、UILevel>=3、明确勾选。dark反编译对WiX标准UI抽象产生DARK1059并把MajorUpgrade写成afterInstallFinalize，原始MSI表确认实际为1501，不用反编译简化结果替代数据库。表证据target/overwrite-consent-261016/package/action-sequence.json。
- JPEG随包验收初轮仅因检查脚本仍断言协议15而失败（实际返回16）；检索全部协议断言并同步两个部署脚本到16，格式/lint通过，重跑未跳过业务断言。新MSI实际解包资源的JPEG验收通过target/jpeg-bundle-unGUat/report.json；四格式/动画有损无损/回退/重试/预览/独立降级通过target/gif-bundle-HfkdNO/report.json。

### 用户手测顺序

1. 安装本节新包，使用.temp/manual-images-2W4KSz中的副本。先确认覆盖并备份、保存副本均不会出现无备份警告；再切无备份覆盖导入新副本，确认警告出现前没有开始压缩。
2. 点取消或Escape，列表保留、原图不变；修改质量不自动启动。点击开始压缩再次提示，明确确认后启动；清空再导入、退出重启后不再提示。如果之前已经确认过，先按第4项勾选删除并重装后再测首次行为。
3. 修改可辨识设置（如英语、深色、质量73），退出应用；从Windows设置卸载，确认新增删除选项默认不勾选，直接继续卸载。重装后设置及覆盖确认保留，图片/结果/备份仍在。
4. 从控制面板卸载，先勾选删除再取消，确认软件/数据仍在；再次卸载时选项重新为未勾选。勾选并完成卸载，启动前核对旧数据；重装后中文、跟随系统主题、质量80等默认值恢复，无备份覆盖重新提示，素材和备份保留。空目录/零字节协调锁保留及启动后重新生成缓存/日志均允许。
5. 重装并设置可辨识偏好后，直接运行同一已安装MSI，从维护向导选择Remove；核对同样的默认不勾选/继续卸载/重装保留行为。三种入口均须看到新选项，旧探测包的结果不能替代。应用警告还需中英、亮暗、小窗口和键盘检查。
6. 可在管理员终端运行node tools/windows-installer/check-msi.mjs --per-machine：独立安装身份自动验证全机安装范围的静默卸载/属性注入/真实major upgrade保留数据。正式PixoFold的静默卸载需另用msiexec /x 新MSI /qn /norestart /L*v 日志，再重装核对；同一MSI维护/修复不作为major upgrade证据。

下一步：等待用户反馈新MSI三入口的实际弹窗、取消、保留/删除及首次警告行为；按反馈查看新日志/标记和测试产物，若无问题再归档。源码、原生模块、WiX片段、配置、生成类型和上述文档均未提交。
