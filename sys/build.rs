extern crate bindgen;
extern crate pkg_config;

use std::env;
use std::path::PathBuf;
use cmake;


fn update_submodules(modules: &[&str], dir: &str) {
    let mut args = vec![
        "submodule",
        "update",
        "--init",
        "--depth", "1",
        "--recommend-shallow",
    ];

    args.extend_from_slice(modules);

    let output = std::process::Command::new("git")
        .current_dir(dir)
        .args(args.into_iter())
        .output()
        .expect("Failed to update submodules");

    if !output.status.success() {
        panic!("Update submodules failed with status {:?}", output);
    }

}

fn main() {

    let mut config = Config::new("azure-iot-sdk-c");
    config
    .define("use_edge_modules", "ON")
    .define("skip_samples", "ON")
    .define("CMAKE_C_FLAGS", "-Wno-array-parameter -Wno-deprecated-declarations -Wno-discarded-qualifiers");

    // Builds the azure iot sdk, installing it
    // into $OUT_DIR
    use cmake::Config;

    let mut modules = vec![
        "c-utility",
        "deps/umock-c",
        "deps/parson",
        "deps/azure-macro-utils-c"
    ];

    // Tell cargo to tell rustc to link the azureiot libraries.
    println!("cargo:rustc-link-lib=iothub_client");

    if env::var_os("CARGO_FEATURE_AMQP").is_some() {
        modules.push("uamqp/");
        config.define("use_amqp", "ON");
        println!("cargo:rustc-link-lib=uamqp");
    } else {
        config.define("use_amqp", "OFF");
    }

    if env::var_os("CARGO_FEATURE_MQTT").is_some() {
        modules.push("umqtt");
        config.define("use_mqtt", "ON");
        println!("cargo:rustc-link-lib=iothub_client_mqtt_transport");
        println!("cargo:rustc-link-lib=umqtt");
    } else {
        config.define("use_mqtt", "OFF");
    }

    if env::var_os("CARGO_FEATURE_PROV_CLIENT").is_some() {
        config.define("use_prov_client", "ON");
        modules.push("provisioning_client/deps/utpm/");
        println!("cargo:rustc-link-lib=prov_auth_client");
        println!("cargo:rustc-link-lib=hsm_security_client");
        println!("cargo:rustc-link-lib=utpm");    
    } else {
        config.define("use_prov_client", "OFF");
    }

    if env::var_os("CARGO_FEATURE_HTTP").is_some() {
        config.define("use_http", "ON");
        modules.push("deps/uhttp/");
        println!("cargo:rustc-link-lib=uhttp");
    } else {
        config.define("use_http", "OFF");
    }

    if env::var_os("UPDATE_SUBMODULES").is_some() {
        update_submodules(&["azure-iot-sdk-c/"], ".");
        update_submodules(&modules, "azure-iot-sdk-c");    
    }

    // Tell cargo to tell rustc to link common azureiot libraries.
    println!("cargo:rustc-link-lib=parson");
    println!("cargo:rustc-link-lib=aziotsharedutil");

    // check for dependencies
    pkg_config::probe_library("uuid").unwrap();
    pkg_config::probe_library("openssl").unwrap();
    pkg_config::probe_library("libcurl").unwrap();
    pkg_config::probe_library("uuid").unwrap();

    println!("cargo:rustc-link-lib=pthread");
    println!("cargo:rustc-link-lib=m");
    println!("cargo:rustc-link-lib=rt");

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=wrapper.h");

    let dst = config.build();
    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    
    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("wrapper.h")
        // additional clang arguments.
        .clang_arg(format!("-I{}/include", dst.display()))
        .clang_arg(format!("-I{}/include/azureiot", dst.display()))
        .clang_arg("-DUSE_EDGE_MODULES")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}