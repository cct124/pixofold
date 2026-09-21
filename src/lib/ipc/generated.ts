// 由 Rust 数据模型生成；请运行 pnpm types:generate，勿手工编辑。
export type ImageFormat = "png" | "jpeg" | "gif" | "apng";
export type AppInfo = { name: string, version: string, plannedFormats: Array<ImageFormat>, compressionAvailable: boolean, };
