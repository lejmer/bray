use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use bray_diagnostics::DiagnosticLlvmToolRole;
use bray_target::NativeTarget;
use sha2::{Digest as _, Sha256};

use crate::native_archive::{NativeCompilation, configure_c_toolchain};

pub(crate) const PROVIDER_IDENTITY_ENVIRONMENT: &str = "BRAY_NATIVE_TOOLCHAIN_IDENTITY";

pub(crate) fn digest(root: &Path, targets: &[NativeTarget]) -> Result<String, String> {
    // The running producer contains the statically linked LLVM backend on Unix hosts.
    let producer = env::current_exe()
        .map_err(|error| format!("could not resolve preparation producer: {error}"))?;

    let mut paths = vec![producer.clone()];

    if cfg!(windows) {
        paths.push(producer.with_file_name("LLVM-C.dll"));
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

    let compiler = build.try_get_compiler()
        .map_err(|error| format!("could not resolve {} provider compiler: {error}", target.as_str()))?;

    let archiver = build.try_get_archiver()
        .map_err(|error| format!("could not resolve {} provider archiver: {error}", target.as_str()))?;

    let mut paths = vec![
        compiler.path().to_path_buf(),
        PathBuf::from(compiler.to_command().get_program()),
        PathBuf::from(archiver.get_program()),
    ];

    if compiler.is_like_gnu() {
        for name in ["cc1plus", "as"] {
            let output = compiler.to_command().arg(format!("-print-prog-name={name}"))
                .current_dir(root).output()
                .map_err(|error| format!("could not resolve provider {name}: {error}"))?;

            if !output.status.success() {
                return Err(format!("provider compiler could not resolve {name}: {}", String::from_utf8_lossy(&output.stderr).trim()));
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

        let path = path.canonicalize()
            .map_err(|error| format!("could not resolve preparation tool {}: {error}", path.display()))?;

        resolved.insert(path);
    }

    let mut digest = Sha256::new();

    for path in resolved {
        let identity = path.to_string_lossy();

        digest.update(identity.len().to_le_bytes());
        digest.update(identity.as_bytes());

        let content = bray_base::sha256_file(&path)
            .map_err(|error| format!("could not fingerprint preparation tool {}: {error}", path.display()))?;

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

    #[test]
    fn replacement_at_same_path_invalidates_even_with_identical_size_and_mtime() {
        let directory = tempfile::tempdir().expect("temporary tools");
        let path = directory.path().join("compiler");

        fs::write(&path, b"first version").expect("first compiler");

        let modified = path.metadata().expect("compiler metadata").modified().expect("mtime");
        let before = digest_paths(directory.path(), std::slice::from_ref(&path)).expect("identity");

        fs::write(&path, b"other version").expect("replacement compiler");

        fs::File::options().write(true).open(&path).expect("compiler file")
            .set_modified(modified).expect("restore mtime");

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
            digest_paths(directory.path(), &[compiler.clone(), archiver.clone()]).expect("identity"),
            digest_paths(directory.path(), &[archiver, compiler.clone(), compiler]).expect("identity")
        );
    }

    #[test]
    fn path_resolution_tracks_selected_directory() {
        let directory = tempfile::tempdir().expect("temporary search path");
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        let name = if cfg!(windows) { "compiler.exe" } else { "compiler" };

        fs::create_dir(&first).expect("first directory");
        fs::create_dir(&second).expect("second directory");
        fs::write(first.join(name), b"first").expect("first compiler");
        fs::write(second.join(name), b"second").expect("second compiler");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;

            for directory in [&first, &second] {
                fs::set_permissions(directory.join(name), fs::Permissions::from_mode(0o700)).expect("executable permissions");
            }
        }

        let search = std::env::join_paths([&first, &second]).expect("search path");

        assert_eq!(resolve_in_path(name.as_ref(), &search), Some(first.join(name)));
    }
}
