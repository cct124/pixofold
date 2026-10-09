# JPEG应用资源

runtime/由pnpm jpeg:bundle:prepare生成，产物不入库。Tauri把本目录映射到应用资源jpeg/。
普通debug检查缺少runtime时保持PNG-only；release构建要求固定来源/目标/哈希验证通过。
运行时信任Rust内嵌哈希，不信旁置manifest.json。桌面JPEG入口在J2第二段开放。
