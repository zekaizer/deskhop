// build.rs — generate Rust types from C sub-structs via bindgen.
//
// C header (structs.h) is the source of truth. Bindgen generates:
// 1. bindgen_types.rs — struct definitions with layout tests
// 2. device_offsets.rs — additional const assertions for Rust type aliases
//
// Verified types: device_hid_t, device_config_t, device_fw_t, device_led_t
// Excluded: device_hw_t (contains SDK-dependent opaque types, not bindgen-able)

use std::env;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let project_root = manifest.parent().unwrap();
    let c_include = project_root.join("src/include");
    let header = c_include.join("structs.h");

    let bindings = bindgen::Builder::default()
        .header(header.to_str().unwrap())
        .clang_arg(format!("-I{}", c_include.display()))
        .clang_arg("--target=arm-none-eabi")
        .use_core()
        .layout_tests(true)
        .derive_default(true)
        .derive_partialeq(true)
        .derive_eq(true)
        .allowlist_type("device_hid_t")
        .allowlist_type("device_config_t")
        .allowlist_type("device_fw_t")
        .allowlist_type("device_led_t")
        .generate()
        .expect("bindgen failed — is structs.h SDK-free?");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Write bindgen types to a file that structs.rs includes
    bindings.write_to_file(out_dir.join("bindgen_types.rs"))
        .expect("failed to write bindgen_types.rs");

    println!("cargo:rerun-if-changed={}", header.display());
    for dep in ["flash.h", "packet.h", "screen.h", "constants.h", "api_config.h"] {
        println!("cargo:rerun-if-changed={}", c_include.join(dep).display());
    }
}
