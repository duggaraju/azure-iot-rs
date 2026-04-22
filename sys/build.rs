extern crate bindgen;
extern crate pkg_config;

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::PathBuf;

fn to_pascal_case(input: &str) -> String {
    let mut out = String::new();
    for part in input.split('_').filter(|p| !p.is_empty()) {
        let lower = part.to_ascii_lowercase();
        let mut chars = lower.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.extend(chars);
        }
    }
    out
}

fn sanitize_variant(name: String) -> String {
    match name.as_str() {
        "Self" | "SelfType" | "Type" | "Match" | "Loop" | "For" | "While" | "If" | "Else"
        | "Use" | "Mod" | "Move" | "Fn" | "Impl" | "Trait" | "Struct" | "Enum" | "Const"
        | "Static" | "Crate" | "Super" | "As" | "In" | "Where" | "Pub" | "Ref" | "Mut"
        | "Unsafe" | "Extern" | "Return" | "Break" | "Continue" | "Box" | "Do" | "Final"
        | "Macro" | "Override" | "Priv" | "Try" | "Yield" | "Abstract" | "Become" | "Unsized"
        | "Virtual" | "Await" | "Dyn" | "Union" | "None" | "Unknown" => format!("{name}Value"),
        _ => name,
    }
}

fn extract_symbol_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let start = line.find(marker)? + marker.len();
    let tail = &line[start..];
    let end = tail
        .find(|c: char| c == ':' || c == '(' || c.is_whitespace())
        .unwrap_or(tail.len());
    Some(&tail[..end])
}

fn common_token_prefix_len(values: &[String]) -> usize {
    if values.is_empty() {
        return 0;
    }

    let tokenized: Vec<Vec<&str>> = values
        .iter()
        .map(|value| value.split('_').filter(|p| !p.is_empty()).collect())
        .collect();

    let min_len = tokenized.iter().map(|parts| parts.len()).min().unwrap_or(0);
    let mut prefix_len = 0;

    for index in 0..min_len {
        let candidate = tokenized[0][index];
        if tokenized.iter().all(|parts| parts[index] == candidate) {
            prefix_len += 1;
        } else {
            break;
        }
    }

    prefix_len
}

fn trim_token_prefix(value: &str, token_prefix_len: usize) -> String {
    let mut parts = value.split('_').filter(|p| !p.is_empty());
    for _ in 0..token_prefix_len {
        let _ = parts.next();
    }

    let remainder: Vec<&str> = parts.collect();
    if remainder.is_empty() {
        value.to_string()
    } else {
        remainder.join("_")
    }
}

fn generate_enum_wrappers(bindings_rs: &str) -> String {
    let mut type_aliases: Vec<(String, String)> = Vec::new();
    let mut strings_fns: Vec<String> = Vec::new();

    for line in bindings_rs.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("pub type ")
            && let Some((left, right)) = trimmed
                .strip_prefix("pub type ")
                .and_then(|rest| rest.split_once('='))
        {
            let alias = left.trim().to_string();
            let raw = right.trim().trim_end_matches(';').trim().to_string();
            type_aliases.push((alias, raw));
        }

        if trimmed.starts_with("pub use self::") && trimmed.contains(" as ") {
            let rest = trimmed.trim_start_matches("pub use self::");
            if let Some((source, alias)) = rest.split_once(" as ") {
                let source = source.trim();
                let alias = alias.trim().trim_end_matches(';').trim();
                if !alias.is_empty() && !source.is_empty() {
                    type_aliases.push((alias.to_string(), source.to_string()));
                }
            }
        }

        if trimmed.starts_with("pub fn ")
            && trimmed.contains("Strings(")
            && let Some(name) = extract_symbol_after(trimmed, "pub fn ")
        {
            strings_fns.push(name.to_string());
        }
    }

    let mut out = String::new();
    out.push_str("use std::ffi::CStr;\n");
    out.push_str("use std::fmt::{Display, Formatter};\n\n");
    out.push_str("macro_rules! generated_sdk_enum_wrapper {\n");
    out.push_str("    (\n");
    out.push_str("        $name:ident,\n");
    out.push_str("        $raw:ty,\n");
    out.push_str("        $strings_fn:ident,\n");
    out.push_str("        {\n");
    out.push_str("            $( $variant:ident = $raw_const:ident, )+\n");
    out.push_str("        }\n");
    out.push_str("    ) => {\n");
    out.push_str("        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
    out.push_str("        pub enum $name {\n");
    out.push_str("            $( $variant, )+\n");
    out.push_str("            Unknown($raw),\n");
    out.push_str("        }\n\n");
    out.push_str("        impl $name {\n");
    out.push_str("            pub fn from_raw(raw: $raw) -> Self { raw.into() }\n");
    out.push_str("            pub fn as_raw(self) -> $raw { self.into() }\n");
    out.push_str("        }\n\n");
    out.push_str("        impl From<$raw> for $name {\n");
    out.push_str("            fn from(value: $raw) -> Self {\n");
    out.push_str("                match value {\n");
    out.push_str("                    $( $raw_const => Self::$variant, )+\n");
    out.push_str("                    _ => Self::Unknown(value),\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("        }\n\n");
    out.push_str("        impl From<$name> for $raw {\n");
    out.push_str("            fn from(value: $name) -> Self {\n");
    out.push_str("                match value {\n");
    out.push_str("                    $( $name::$variant => $raw_const, )+\n");
    out.push_str("                    $name::Unknown(raw) => raw,\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("        }\n\n");
    out.push_str("        impl Display for $name {\n");
    out.push_str("            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {\n");
    out.push_str("                let raw: $raw = (*self).into();\n");
    out.push_str("                let value = unsafe { $strings_fn(raw) };\n");
    out.push_str("                if value.is_null() {\n");
    out.push_str("                    write!(f, \"Unknown({raw})\")\n");
    out.push_str("                } else {\n");
    out.push_str("                    let value = unsafe { CStr::from_ptr(value) };\n");
    out.push_str("                    write!(f, \"{}\", value.to_string_lossy())\n");
    out.push_str("                }\n");
    out.push_str("            }\n");
    out.push_str("        }\n");
    out.push_str("    };\n");
    out.push_str("}\n\n");

    for (alias, _raw) in &type_aliases {
        let strings_fn = format!("{alias}Strings");
        if !strings_fns.iter().any(|f| f == &strings_fn) {
            continue;
        }

        let const_prefix = format!("{alias}_TAG_");
        let mut raw_constants: Vec<(String, String)> = Vec::new();
        for line in bindings_rs.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("pub const ") || !trimmed.contains(&const_prefix) {
                continue;
            }

            if let Some(const_name) = extract_symbol_after(trimmed, "pub const ") {
                let suffix = const_name.trim_start_matches(&const_prefix);
                raw_constants.push((suffix.to_string(), const_name.to_string()));
            }
        }

        if raw_constants.is_empty() {
            continue;
        }

        let suffixes: Vec<String> = raw_constants
            .iter()
            .map(|(suffix, _)| suffix.clone())
            .collect();
        let common_prefix_len = common_token_prefix_len(&suffixes);
        let mut used_variant_names: HashMap<String, usize> = HashMap::new();
        let mut constants = Vec::new();

        for (suffix, constant) in raw_constants {
            let short_suffix = trim_token_prefix(&suffix, common_prefix_len);
            let mut variant = sanitize_variant(to_pascal_case(&short_suffix));

            if variant.is_empty() {
                variant = "Value".to_string();
            }

            let counter = used_variant_names.entry(variant.clone()).or_insert(0);
            if *counter > 0 {
                variant = format!("{variant}{}", *counter + 1);
            }
            *counter += 1;

            constants.push((variant, constant));
        }

        let wrapper_name = to_pascal_case(alias);
        out.push_str("generated_sdk_enum_wrapper!(\n");
        out.push_str(&format!("    {wrapper_name},\n"));
        out.push_str(&format!("    {alias},\n"));
        out.push_str(&format!("    {strings_fn},\n"));
        out.push_str("    {\n");
        for (variant, constant) in constants {
            out.push_str(&format!("        {variant} = {constant},\n"));
        }
        out.push_str("    }\n");
        out.push_str(");\n\n");
    }

    out
}

fn update_submodules(modules: &[&str], dir: &str) {
    let mut args = vec![
        "submodule",
        "update",
        "--init",
        "--depth",
        "1",
        "--recommend-shallow",
    ];

    args.extend_from_slice(modules);

    let output = std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("Failed to update submodules");

    if !output.status.success() {
        panic!("Update submodules failed with status {output:?}");
    }
}

fn generate_struct_wrappers(bindings_rs: &str) -> String {
    let mut type_aliases: Vec<(String, String)> = Vec::new();
    let mut struct_names: HashSet<String> = HashSet::new();

    for line in bindings_rs.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("pub struct ")
            && let Some(name) = extract_symbol_after(trimmed, "pub struct ")
        {
            struct_names.insert(name.to_string());
        }

        if trimmed.starts_with("pub type ")
            && let Some((left, right)) = trimmed
                .strip_prefix("pub type ")
                .and_then(|rest| rest.split_once('='))
        {
            let alias = left.trim().to_string();
            let raw = right.trim().trim_end_matches(';').trim().to_string();
            type_aliases.push((alias, raw));
        }

        if trimmed.starts_with("pub use self::") && trimmed.contains(" as ") {
            let rest = trimmed.trim_start_matches("pub use self::");
            if let Some((source, alias)) = rest.split_once(" as ") {
                let source = source.trim();
                let alias = alias.trim().trim_end_matches(';').trim();
                if !alias.is_empty() && !source.is_empty() {
                    type_aliases.push((alias.to_string(), source.to_string()));
                }
            }
        }
    }

    let mut out = String::new();
    let mut emitted: HashSet<String> = HashSet::new();

    for (alias, raw) in type_aliases {
        if !alias.starts_with("IOTHUB_") {
            continue;
        }

        if !struct_names.contains(&raw) {
            continue;
        }

        let wrapper_name = to_pascal_case(&alias);
        if wrapper_name.is_empty() || !emitted.insert(wrapper_name.clone()) {
            continue;
        }

        out.push_str("#[repr(transparent)]\n");
        out.push_str(&format!("pub struct {wrapper_name} {{\n"));
        out.push_str(&format!("    inner: {alias},\n"));
        out.push_str("}\n\n");

        out.push_str(&format!("impl {wrapper_name} {{\n"));
        out.push_str(&format!(
            "    pub fn from_raw(inner: {alias}) -> Self {{\n        Self {{ inner }}\n    }}\n\n"
        ));
        out.push_str(&format!(
            "    pub fn as_raw(&self) -> &{alias} {{\n        &self.inner\n    }}\n\n"
        ));
        out.push_str(&format!(
            "    pub fn as_raw_mut(&mut self) -> &mut {alias} {{\n        &mut self.inner\n    }}\n\n"
        ));
        out.push_str(&format!(
            "    pub fn into_raw(self) -> {alias} {{\n        self.inner\n    }}\n"
        ));
        out.push_str("}\n\n");
    }

    out
}

fn main() {
    let mut config = Config::new("azure-iot-sdk-c");
    config
        .define("use_edge_modules", "ON")
        .define("skip_samples", "ON")
        .define(
            "CMAKE_C_FLAGS",
            "-Wno-array-parameter -Wno-deprecated-declarations -Wno-discarded-qualifiers",
        );

    // Builds the azure iot sdk, installing it
    // into $OUT_DIR
    use cmake::Config;

    let mut modules = vec![
        "c-utility",
        "deps/umock-c",
        "deps/parson",
        "deps/azure-macro-utils-c",
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
    let bindings_path = out_path.join("bindings.rs");
    bindings
        .write_to_file(&bindings_path)
        .expect("Couldn't write bindings!");

    let bindings_source =
        fs::read_to_string(&bindings_path).expect("Couldn't read generated bindings source");
    let wrappers = generate_enum_wrappers(&bindings_source);
    fs::write(out_path.join("enum_wrappers.rs"), wrappers)
        .expect("Couldn't write generated enum wrappers");

    let struct_wrappers = generate_struct_wrappers(&bindings_source);
    fs::write(out_path.join("struct_wrappers.rs"), struct_wrappers)
        .expect("Couldn't write generated struct wrappers");
}
