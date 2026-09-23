//! 文件写入由tools负责；生成器不启动桌面或任务线程。
fn main() {
    print!("{}", pixofold_desktop_lib::task_type_declarations());
}
