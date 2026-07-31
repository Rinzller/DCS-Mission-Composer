fn main() {
    println!("cargo:rerun-if-changed=../public/icons/favicon.ico");
    println!("cargo:rerun-if-changed=../public/icons/favicon.png");
    println!("cargo:rerun-if-changed=../public/icons/favicon.svg");
    tauri_build::build();
}
