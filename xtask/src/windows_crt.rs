pub(crate) const RUST_DYNAMIC_TARGET_FEATURE: &str = "target-feature=-crt-static";
pub(crate) const CLANG_DYNAMIC_RUNTIME: &str = "-fms-runtime-lib=dll";
pub(crate) const CLANG_CL_DYNAMIC_RUNTIME: &str = "/MD";

pub(crate) fn is_dynamic_library(name: &str) -> bool {
    let name = file_name(name).to_ascii_lowercase();

    if !name.ends_with(".dll") {
        return false;
    }

    name.starts_with("api-ms-win-crt-")
        || name.starts_with("concrt")
        || name.starts_with("msvcp")
        || name.starts_with("msvcr")
        || name.starts_with("vcruntime")
        || name == "ucrtbase.dll"
}

pub(crate) fn is_static_library(name: &str) -> bool {
    let name = file_name(name).to_ascii_lowercase();
    let name = name.strip_suffix(".lib").unwrap_or(&name);

    matches!(
        name,
        "libcmt"
            | "libcmtd"
            | "libconcrt"
            | "libconcrtd"
            | "libcpmt"
            | "libcpmtd"
            | "libucrt"
            | "libucrtd"
            | "libvcruntime"
            | "libvcruntimed"
    )
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_library_classification_distinguishes_imports_from_static_archives() {
        for library in [
            "api-ms-win-crt-runtime-l1-1-0.dll",
            "C:/Windows/System32/ucrtbase.dll",
            "CONCRT140.dll",
            "VCRUNTIME140.dll",
            "msvcp140.dll",
        ] {
            assert!(super::is_dynamic_library(library), "{library}");
            assert!(!super::is_static_library(library), "{library}");
        }

        for library in [
            "libcmt.lib",
            "C:/toolchain/libucrt.lib",
            "libconcrt.lib",
            "libvcruntime.lib",
            "libcpmt.lib",
        ] {
            assert!(super::is_static_library(library), "{library}");
            assert!(!super::is_dynamic_library(library), "{library}");
        }

        for library in ["kernel32.dll", "ucrt.lib", "msvcrt.lib", "system.dll"] {
            assert!(!super::is_static_library(library), "{library}");
        }
    }
}
