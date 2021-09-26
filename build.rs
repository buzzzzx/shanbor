// build.rs 可以在编译 cargo 项目时做额外的编译处理
// 将 abi.proto 编译到指定目录下，编译结果是转换后的 Rust 数据结构
fn main() {
    prost_build::Config::new()
        .out_dir("src/pb")
        .compile_protos(&["abi.proto"], &["."])
        .unwrap();
}