# GIF 桌面资源

`pnpm gif:bundle:prepare` 共用固定 Gifsicle 1.96 配方，生成本目录下由 Git 排除的 `runtime/`。包含有界 helper、完整上游源码归档、封装与构建配方及许可通知。

构建期校验目标、配方、源码及产物，将 helper SHA256 编入程序。运行时从宿主资源目录加载，不从清单、PATH 或图片输入获取信任。正式构建必须有有效资源；普通 debug 检查可以没有资源。

`pnpm gif:bundle:check` 使用独立无 GUI 验收程序，在隔离资源目录执行真实处理、身份拒绝和桌面混合检查。该程序不随默认安装包分发。验收证据见[动画连续记录](../../../docs/devlog/_plan/261009/animation-foundation.md)。
