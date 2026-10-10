use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_diagnostics::DiagnosticLlvmToolRole;
use bray_target::NativeTarget;
use sha2::{Digest as _, Sha256};

use crate::native_archive::{NativeCompilation, configure_c_toolchain, target_environment};

pub(crate) const PROVIDER_IDENTITY_ENVIRONMENT: &str = "BRAY_NATIVE_TOOLCHAIN_IDENTITY";

pub(crate) fn digest(root: &Path, targets: &[NativeTarget]) -> Result<String, String> {
    // The running producer contains the statically linked LLVM backend on Unix hosts.
    let producer = env::current_exe()
        .map_err(|error| format!("could not resolve preparation producer: {error}"))?;

    let mut paths = vec![producer.clone()];

    #[cfg(windows)]
    {
        let search = env::var_os("PATH").unwrap_or_default();

        paths.push(llvm_library_path(&producer, &search)?);
    }

    paths.push(
        bray_tooling::llvm_tool_path(DiagnosticLlvmToolRole::Archiver)
            .map_err(|error| error.to_string())?,
    );

    // Provider publication inspects native exports. Host optimization also summarizes bitcode.
    for target in targets {
        let inspector = if target.object_format() == bray_target::ObjectFormat::Coff {
            "llvm-readobj"
        } else {
            "llvm-nm"
        };

        paths.push(bray_llvm_toolchain::tool_path(root, inspector));
        paths.extend(provider_paths(root, *target, NativeCompilation::Object)?);

        if NativeTarget::current() == Some(*target) {
            paths.extend(provider_paths(root, *target, NativeCompilation::ThinLto)?);

            for name in ["clang", "opt", "llvm-ar"] {
                paths.push(bray_llvm_toolchain::tool_path(root, name));
            }
        }
    }

    digest_paths(root, &paths)
}

#[cfg(windows)]
fn llvm_library_path(producer: &Path, search: &OsStr) -> Result<PathBuf, String> {
    let sibling = producer.with_file_name("LLVM-C.dll");

    if sibling.try_exists().map_err(|error| {
        format!(
            "could not inspect preparation library {}: {error}",
            sibling.display()
        )
    })? {
        return Ok(sibling);
    }

    // Cargo puts dependency DLL directories on PATH when running build scripts.
    resolve_in_path(OsStr::new("LLVM-C.dll"), search).ok_or_else(|| {
        format!(
            "preparation library LLVM-C.dll is absent beside {} and from PATH",
            producer.display()
        )
    })
}

pub(crate) fn provider_digest(
    root: &Path,
    target: NativeTarget,
    compilation: NativeCompilation,
) -> Result<String, String> {
    let tools = digest_paths(root, &provider_paths(root, target, compilation)?)?;
    let mut identity = Sha256::new();

    crate::input_identity::hash_build_environment(&mut identity);
    identity.update(target.as_str());

    identity.update([match compilation {
        NativeCompilation::Object => 0,
        NativeCompilation::ThinLto => 1,
    }]);

    identity.update(tools);

    Ok(bray_base::lowercase_hex(&identity.finalize()))
}

fn provider_paths(
    root: &Path,
    target: NativeTarget,
    compilation: NativeCompilation,
) -> Result<Vec<PathBuf>, String> {
    let host = NativeTarget::current()
        .ok_or_else(|| "native provider compiler host is unsupported".to_owned())?;

    let mut command = Command::new("cargo");

    configure_c_toolchain(&mut command, root, target, compilation);

    // Use the same cc resolver as both C++ provider build scripts, including target overrides.
    let mut build = cc::Build::new();

    build
        .target(target.as_str())
        .host(host.as_str())
        .cpp(true)
        .opt_level(3)
        .cargo_metadata(false)
        .out_dir(env::temp_dir());

    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            build.env(key, value);
        }
    }

    // cc's env setter affects child processes, not its CXX/AR selection. A raw-target
    // environment variable has precedence over the normalized override in Cargo too.
    for prefix in ["CXX", "AR"] {
        if env::var_os(format!("{prefix}_{}", target.as_str())).is_some() {
            continue;
        }

        let name = target_environment(prefix, target);

        if let Some((_, Some(value))) = command
            .get_envs()
            .find(|(key, _)| *key == OsStr::new(&name))
        {
            if prefix == "CXX" {
                build.compiler(value);
            } else {
                build.archiver(value);
            }
        }
    }

    let compiler = build.try_get_compiler().map_err(|error| {
        format!(
            "could not resolve {} provider compiler: {error}",
            target.as_str()
        )
    })?;

    let archiver = build.try_get_archiver().map_err(|error| {
        format!(
            "could not resolve {} provider archiver: {error}",
            target.as_str()
        )
    })?;

    let mut paths = vec![
        compiler.path().to_path_buf(),
        PathBuf::from(compiler.to_command().get_program()),
        PathBuf::from(archiver.get_program()),
    ];

    // Cargo invokes this wrapper for the Rust provider, and cc also uses compatible
    // wrappers for C++. Explicit compiler selection bypasses cc's wrapper fallback.
    if let Some(wrapper) = env::var_os("RUSTC_WRAPPER").filter(|wrapper| !wrapper.is_empty()) {
        paths.push(PathBuf::from(wrapper));
    }

    if compiler.is_like_gnu() {
        for name in ["cc1plus", "as"] {
            let output = compiler
                .to_command()
                .arg(format!("-print-prog-name={name}"))
                .current_dir(root)
                .output()
                .map_err(|error| format!("could not resolve provider {name}: {error}"))?;

            if !output.status.success() {
                return Err(format!(
                    "provider compiler could not resolve {name}: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }

            let path = String::from_utf8(output.stdout)
                .map_err(|error| format!("provider {name} path is not UTF-8: {error}"))?;

            paths.push(PathBuf::from(path.trim()));
        }
    }

    Ok(paths)
}

fn digest_paths(root: &Path, paths: &[PathBuf]) -> Result<String, String> {
    let mut resolved = BTreeSet::new();

    for path in paths {
        let path = resolve_program(root, path)?;

        let path = path.canonicalize().map_err(|error| {
            format!(
                "could not resolve preparation tool {}: {error}",
                path.display()
            )
        })?;

        resolved.insert(path);
    }

    let mut digest = Sha256::new();

    for path in resolved {
        let identity = path.to_string_lossy();

        digest.update(identity.len().to_le_bytes());
        digest.update(identity.as_bytes());

        let content = bray_base::sha256_file(&path).map_err(|error| {
            format!(
                "could not fingerprint preparation tool {}: {error}",
                path.display()
            )
        })?;

        digest.update(content);
    }

    Ok(bray_base::lowercase_hex(&digest.finalize()))
}

fn resolve_program(root: &Path, path: &Path) -> Result<PathBuf, String> {
    if path.components().count() > 1 || path.is_absolute() {
        return Ok(root.join(path));
    }

    let search = env::var_os("PATH").unwrap_or_default();

    resolve_in_path(path.as_os_str(), &search)
        .ok_or_else(|| format!("preparation tool {} is absent from PATH", path.display()))
}

fn resolve_in_path(program: &OsStr, search: &OsStr) -> Option<PathBuf> {
    env::split_paths(search).find_map(|directory| {
        let path = directory.join(program);

        if cfg!(windows) && path.extension().is_none() {
            let executable = path.with_extension("exe");

            if executable.is_file() {
                return Some(executable);
            }
        }

        let metadata = path.metadata().ok()?;

        if !metadata.is_file() {
            return None;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;

            if metadata.permissions().mode() & 0o111 == 0 {
                return None;
            }
        }

        Some(path)
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{digest_paths, resolve_in_path};

    #[cfg(windows)]
    #[test]
    fn cargo_build_script_library_is_fingerprinted_from_dependency_path() {
        let directory = tempfile::tempdir().expect("Cargo layout");

        let producer = directory
            .path()
            .join("release/build/bray-runtime-id/build-script-build.exe");

        let dependencies = directory.path().join("release/deps");
        let library = dependencies.join("LLVM-C.dll");

        fs::create_dir_all(producer.parent().expect("producer directory"))
            .expect("build directory");

        fs::create_dir_all(&dependencies).expect("dependency directory");
        fs::write(&producer, b"build script").expect("producer");
        fs::write(&library, b"first version").expect("LLVM library");

        let search = std::env::join_paths([&dependencies]).expect("Cargo PATH");
        let selected = super::llvm_library_path(&producer, &search).expect("LLVM dependency");

        assert_eq!(selected, library);

        let modified = library
            .metadata()
            .expect("library metadata")
            .modified()
            .expect("mtime");

        let paths = [producer, selected];
        let before = digest_paths(directory.path(), &paths).expect("producer identity");

        fs::write(&library, b"other version").expect("replacement library");

        fs::File::options()
            .write(true)
            .open(&library)
            .expect("library file")
            .set_modified(modified)
            .expect("restore mtime");

        assert_ne!(
            before,
            digest_paths(directory.path(), &paths).expect("replacement identity")
        );
    }

    #[cfg(windows)]
    #[test]
    fn producer_library_takes_precedence_over_path_library() {
        let directory = tempfile::tempdir().expect("producer libraries");
        let producer = directory.path().join("xtask.exe");
        let sibling = directory.path().join("LLVM-C.dll");
        let dependencies = directory.path().join("deps");

        fs::create_dir(&dependencies).expect("dependency directory");
        fs::write(&sibling, b"sibling library").expect("sibling");
        fs::write(dependencies.join("LLVM-C.dll"), b"PATH library").expect("PATH library");

        let search = std::env::join_paths([&dependencies]).expect("dependency PATH");

        assert_eq!(super::llvm_library_path(&producer, &search), Ok(sibling));
    }

    #[cfg(windows)]
    #[test]
    fn missing_producer_library_preserves_search_context() {
        let directory = tempfile::tempdir().expect("missing library");
        let producer = directory.path().join("build-script-build.exe");
        let search = std::env::join_paths([directory.path()]).expect("empty directory PATH");

        assert_eq!(
            super::llvm_library_path(&producer, &search),
            Err(format!(
                "preparation library LLVM-C.dll is absent beside {} and from PATH",
                producer.display()
            ))
        );
    }

    #[cfg(unix)]
    #[test]
    fn configured_provider_tools_override_ambient_selection() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("provider tools");
        let bin = directory.path().join("bin");

        fs::create_dir(&bin).expect("tool directory");

        for name in [
            "clang++",
            "clang-cl",
            "alternative-clang++",
            "llvm-ar",
            "llvm-lib",
            "sccache",
        ] {
            let path = bin.join(name);

            fs::write(&path, b"#!/bin/sh\necho __clang__\n# version-a\n").expect("selected tool");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("executable");
        }

        for raw_overrides in [false, true] {
            let mut command =
                std::process::Command::new(std::env::current_exe().expect("test executable"));

            command
                .args([
                    "--exact",
                    "preparation_tools::tests::configured_provider_tools_child",
                    "--ignored",
                ])
                .env("BRAY_LLVM_PREFIX", directory.path())
                .env("RUSTC_WRAPPER", bin.join("sccache"))
                .env("CXX", directory.path().join("unrelated-compiler"))
                .env("AR", directory.path().join("unrelated-archiver"));

            for target in [
                bray_target::NativeTarget::current().expect("host"),
                bray_target::NativeTarget::X86_64WindowsMsvc,
            ] {
                for prefix in ["CXX", "AR"] {
                    let raw = format!("{prefix}_{}", target.as_str());

                    command.env_remove(crate::native_archive::target_environment(prefix, target));

                    if raw_overrides {
                        let name = if prefix == "CXX" {
                            "alternative-clang++"
                        } else {
                            "llvm-ar"
                        };

                        command.env(raw, bin.join(name));
                    } else {
                        command.env_remove(raw);
                    }
                }
            }

            let output = command.output().expect("resolver process");

            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "invoked in a subprocess with isolated tool-selection environment"]
    fn configured_provider_tools_child() {
        use crate::native_archive::NativeCompilation;

        let root =
            std::path::PathBuf::from(std::env::var_os("BRAY_LLVM_PREFIX").expect("selected tools"));

        let bin = root.join("bin");

        for (target, compilation, compiler, archiver) in [
            (
                bray_target::NativeTarget::current().expect("host"),
                NativeCompilation::ThinLto,
                "clang++",
                "llvm-ar",
            ),
            (
                bray_target::NativeTarget::X86_64WindowsMsvc,
                NativeCompilation::Object,
                "clang-cl",
                "llvm-lib",
            ),
        ] {
            let compiler = std::env::var_os(format!("CXX_{}", target.as_str()))
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| bin.join(compiler));

            let archiver = std::env::var_os(format!("AR_{}", target.as_str()))
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| bin.join(archiver));

            let paths =
                super::provider_paths(&root, target, compilation).expect("configured resolver");

            assert!(paths.contains(&compiler), "selected compiler: {paths:?}");
            assert!(paths.contains(&archiver), "selected archiver: {paths:?}");

            assert!(
                paths.contains(&bin.join("sccache")),
                "selected wrapper: {paths:?}"
            );

            for tool in [&compiler, &bin.join("sccache")] {
                let modified = tool
                    .metadata()
                    .expect("tool metadata")
                    .modified()
                    .expect("mtime");

                let before =
                    super::provider_digest(&root, target, compilation).expect("provider identity");

                let content = fs::read_to_string(tool).expect("tool");

                let replacement = if content.contains("version-a") {
                    content.replace("version-a", "version-b")
                } else {
                    content.replace("version-b", "version-a")
                };

                fs::write(tool, replacement).expect("replacement");

                fs::File::options()
                    .write(true)
                    .open(tool)
                    .expect("tool file")
                    .set_modified(modified)
                    .expect("restore mtime");

                assert_ne!(
                    before,
                    super::provider_digest(&root, target, compilation)
                        .expect("replacement identity")
                );
            }
        }
    }

    #[test]
    fn replacement_at_same_path_invalidates_even_with_identical_size_and_mtime() {
        let directory = tempfile::tempdir().expect("temporary tools");
        let path = directory.path().join("compiler");

        fs::write(&path, b"first version").expect("first compiler");

        let modified = path
            .metadata()
            .expect("compiler metadata")
            .modified()
            .expect("mtime");

        let before = digest_paths(directory.path(), std::slice::from_ref(&path)).expect("identity");

        fs::write(&path, b"other version").expect("replacement compiler");

        fs::File::options()
            .write(true)
            .open(&path)
            .expect("compiler file")
            .set_modified(modified)
            .expect("restore mtime");

        let after = digest_paths(directory.path(), std::slice::from_ref(&path)).expect("identity");

        assert_ne!(before, after);
    }

    #[test]
    fn tool_order_and_duplicate_roles_do_not_change_identity() {
        let directory = tempfile::tempdir().expect("temporary tools");
        let compiler = directory.path().join("compiler");
        let archiver = directory.path().join("archiver");

        fs::write(&compiler, b"compiler").expect("compiler");
        fs::write(&archiver, b"archiver").expect("archiver");

        assert_eq!(
            digest_paths(directory.path(), &[compiler.clone(), archiver.clone()])
                .expect("identity"),
            digest_paths(directory.path(), &[archiver, compiler.clone(), compiler])
                .expect("identity")
        );
    }

    #[test]
    fn path_resolution_tracks_selected_directory() {
        let directory = tempfile::tempdir().expect("temporary search path");
        let first = directory.path().join("first");
        let second = directory.path().join("second");

        let name = if cfg!(windows) {
            "compiler.exe"
        } else {
            "compiler"
        };

        fs::create_dir(&first).expect("first directory");
        fs::create_dir(&second).expect("second directory");
        fs::write(first.join(name), b"first").expect("first compiler");
        fs::write(second.join(name), b"second").expect("second compiler");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;

            for directory in [&first, &second] {
                fs::set_permissions(directory.join(name), fs::Permissions::from_mode(0o700))
                    .expect("executable permissions");
            }
        }

        let search = std::env::join_paths([&first, &second]).expect("search path");

        assert_eq!(
            resolve_in_path(name.as_ref(), &search),
            Some(first.join(name))
        );
    }
}
