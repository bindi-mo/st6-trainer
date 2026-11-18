fn main() {
    tauri_build::build();

    // Verify that the scanner DLL was copied to src-tauri/ by CMake POST_BUILD
    let dll_path = std::path::Path::new("st6_scanner.dll");
    if !dll_path.exists() {
        panic!(
            "st6_scanner.dll not found at {}. \
            Build scanner-cpp first (CMake) so POST_BUILD copies the DLL.",
            dll_path.display()
        );
    }
        println!("cargo:rerun-if-changed=st6_scanner.dll");
}
