fn main() {
    // personas/ 是 include_dir 嵌入的内置人格目录，新增文件也要触发重编译
    println!("cargo:rerun-if-changed=personas");
    println!("cargo:rerun-if-changed=skills");
    tauri_build::build()
}
