# CI仅对main推送与目标main的PR自动运行

- 创建日期：2026-10-16（Asia/Shanghai）。状态：已随0014780提交并推送dev与main，main push已按新配置触发；运行与事件验收待核对。
- 开工基准：dev/c1f13a5；保留现有“关于”仓库入口与文档修改，不提交或推送。
- 用户授权：仅推送main或向main提交PR时自动运行GitHub Actions，保留手动运行，避免dev推送重复执行。

## 目标与验收

- push配置branches: [main]；pull_request配置branches: [main]（匹配目标分支）；workflow_dispatch保留。
- dev/功能分支仅push且没有指向main的PR时不自动运行；已有main目标PR时更新来源分支仍运行PR检查，合入main后运行push检查。
- 保留现有四类任务、三平台矩阵、权限和并发取消策略。仅检查YAML格式/语法、diff及文档一致性，不重复压缩测试或MSI构建。
- 本机改动不能声称远端已生效；提交到相应分支并推送后再按实际事件验收。

## 2026-10-16 实施与验证

- `.github/workflows/ci.yml`仅新增两行branches: [main]，分别作用于push与pull_request；手动触发、4类×3平台共12个job、权限及按github.ref分组取消旧运行均未修改。PR使用默认opened/synchronize/reopened事件；目标main的PR更新仍检查，合入main后的push仍检查。仅配置branches且未配置tags，因此标签push不会自动触发。
- README新增CI触发范围，明确无main目标PR的dev推送不运行、已有main目标PR的来源更新仅触发PR检查、手动检查保留；dev开发阶段可继续使用本地pnpm check或按需手动运行。
- 使用固定Node26.9.0执行仓库Prettier3.9.8，对ci.yml的YAML语法及格式检查通过；`git diff --check`通过，diff确认工作流其余内容保持。未新增依赖/测试，未运行与触发过滤无关的压缩测试或安装构建。
- 保留既有“关于”仓库入口和README/AGENTS/devlog修改，本任务仅变更ci.yml、README新增说明、devlog索引及本记录。本地验收不代表GitHub已运行新策略，未提交推送或启动手动Actions。

## 2026-10-16 提交与推送

- 提交0014780含ci.yml两行branches: [main]、README触发范围说明及本记录；origin/dev与origin/main同步至同一提交，新配置自该提交起生效。
- 本次dev与main在同一提交推送：main push应触发检查、dev push按过滤不触发；实际运行列表以远端查询为准，结果待核对。

## 下一步

- 未包含此修改的旧分支仍按其旧workflow执行；按实际事件核对：main push运行；无main目标PR的dev push不运行；dev→main的PR新建/来源更新运行一次PR检查；手动运行可用。不为验证而创建测试PR或空提交。
