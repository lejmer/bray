use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use bray_base::NonEmptySharedStr;
use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};

pub(crate) struct RustStaticLibrary {
    archive: PathBuf,
    native_links: Vec<NativeLinkRequirement>,
}

impl RustStaticLibrary {
    pub(crate) fn archive(&self) -> &Path {
        &self.archive
    }

    pub(crate) fn native_links(&self) -> &[NativeLinkRequirement] {
        &self.native_links
    }
}

pub(crate) fn build_rust_static_library(
    root: &Path,
    target: NativeTarget,
    package: &str,
    profile: &str,
    features: &[&str],
) -> Result<RustStaticLibrary, BuildError> {
    build_rust_static_library_with_options(root, target, package, profile, features, false)
}

pub(crate) fn build_no_std_rust_static_library(
    root: &Path,
    target: NativeTarget,
    package: &str,
    profile: &str,
    features: &[&str],
) -> Result<RustStaticLibrary, BuildError> {
    build_rust_static_library_with_options(root, target, package, profile, features, true)
}

pub(crate) fn build_thin_lto_rust_static_library(
    root: &Path,
    target: NativeTarget,
    package: &str,
    profile: &str,
    features: &[&str],
    abort_on_panic: bool,
) -> Result<RustStaticLibrary, BuildError> {
    build_rust_static_library_with_configuration(
        root,
        target,
        package,
        profile,
        features,
        abort_on_panic,
        NativeCompilation::ThinLto,
    )
}

fn build_rust_static_library_with_options(
    root: &Path,
    target: NativeTarget,
    package: &str,
    profile: &str,
    features: &[&str],
    abort_on_panic: bool,
) -> Result<RustStaticLibrary, BuildError> {
    build_rust_static_library_with_configuration(
        root,
        target,
        package,
        profile,
        features,
        abort_on_panic,
        NativeCompilation::Object,
    )
}

#[derive(Clone, Copy)]
enum NativeCompilation {
    Object,
    ThinLto,
}

#[derive(Debug, Eq, PartialEq)]
struct NativeTools {
    compiler: &'static str,
    cpp_compiler: &'static str,
    archiver: &'static str,
}

fn build_rust_static_library_with_configuration(
    root: &Path,
    target: NativeTarget,
    package: &str,
    profile: &str,
    features: &[&str],
    abort_on_panic: bool,
    native_compilation: NativeCompilation,
) -> Result<RustStaticLibrary, BuildError> {
    let target_directory = match native_compilation {
        NativeCompilation::Object => crate::workspace::cargo_target(root),
        NativeCompilation::ThinLto => crate::workspace::cargo_target(root).join("thin-lto-native"),
    };

    let mut command = Command::new("cargo");

    command.current_dir(root).args([
        "rustc",
        "--color",
        "never",
        "--package",
        package,
        "--target",
        target.as_str(),
        "--profile",
        profile,
        "--no-default-features",
        "--target-dir",
    ]);

    command.arg(&target_directory);

    if !features.is_empty() {
        command.arg("--features").arg(features.join(","));
    }

    command.arg("--");

    if abort_on_panic {
        command.args(["-C", "panic=abort"]);
    }

    command.args(["--print", "native-static-libs"]);
    configure_c_toolchain(&mut command, root, target, native_compilation);

    let output = command.output().map_err(BuildError::Cargo)?;

    if !output.status.success() {
        let standard_output = String::from_utf8_lossy(&output.stdout);
        let standard_error = String::from_utf8_lossy(&output.stderr);

        let detail = [standard_output.trim(), standard_error.trim()]
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        return Err(BuildError::BuildFailed {
            package: package.to_owned(),
            detail,
        });
    }

    let native_links = native_link_requirements(&output)?;

    let archive = target_directory
        .join(target.as_str())
        .join(profile_directory(profile))
        .join(rust_static_library_file_name(target, package));

    if !archive.is_file() {
        return Err(BuildError::MissingArchive(archive));
    }

    Ok(RustStaticLibrary {
        archive,
        native_links,
    })
}

fn rust_static_library_file_name(target: NativeTarget, package: &str) -> String {
    let crate_name = package.replace('-', "_");

    let name =
        TargetOutputName::for_native(target.object_format(), TargetOutputKind::StaticLibrary)
            .file_name(&crate_name);

    let Some(name) = name else {
        panic!("Cargo package identities must form valid native output names")
    };

    name
}

fn configure_c_toolchain(
    command: &mut Command,
    root: &Path,
    target: NativeTarget,
    compilation: NativeCompilation,
) {
    if matches!(compilation, NativeCompilation::ThinLto) {
        let tools = native_tools(target);
        let flags = thin_lto_flags(root);

        command
            .env(
                target_environment("CC", target),
                bray_llvm_toolchain::tool_path(root, tools.compiler),
            )
            .env(
                target_environment("CXX", target),
                bray_llvm_toolchain::tool_path(root, tools.cpp_compiler),
            )
            .env(
                target_environment("AR", target),
                bray_llvm_toolchain::tool_path(root, tools.archiver),
            )
            .env(target_environment("CFLAGS", target), &flags)
            .env(target_environment("CXXFLAGS", target), flags);

        return;
    }

    if cfg!(windows) && target == NativeTarget::X86_64LinuxGnu {
        command
            .env(
                "CC_x86_64_unknown_linux_gnu",
                bray_llvm_toolchain::tool_path(root, "clang"),
            )
            .env(
                "CXX_x86_64_unknown_linux_gnu",
                bray_llvm_toolchain::tool_path(root, "clang++"),
            )
            .env(
                "AR_x86_64_unknown_linux_gnu",
                bray_llvm_toolchain::tool_path(root, "llvm-ar"),
            );
    }
}

fn thin_lto_flags(root: &Path) -> String {
    thin_lto_arguments(root)
        .into_iter()
        .map(|argument| {
            if argument.contains(' ') {
                format!("\"{argument}\"")
            } else {
                argument
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn thin_lto_arguments(root: &Path) -> [String; 3] {
    let root = crate::path::slash_separated(root);

    [
        "-flto=thin".to_owned(),
        format!("-ffile-prefix-map={root}=."),
        "-fdebug-compilation-dir=.".to_owned(),
    ]
}

fn native_tools(target: NativeTarget) -> NativeTools {
    if target.object_format() == bray_target::ObjectFormat::Coff {
        NativeTools {
            compiler: "clang-cl",
            cpp_compiler: "clang-cl",
            archiver: "llvm-lib",
        }
    } else {
        NativeTools {
            compiler: "clang",
            cpp_compiler: "clang++",
            archiver: "llvm-ar",
        }
    }
}

fn target_environment(prefix: &str, target: NativeTarget) -> String {
    format!("{prefix}_{}", target.as_str().replace('-', "_"))
}

fn native_link_requirements(output: &Output) -> Result<Vec<NativeLinkRequirement>, BuildError> {
    let standard_output = String::from_utf8_lossy(&output.stdout);
    let standard_error = String::from_utf8_lossy(&output.stderr);

    let Some(arguments) = standard_output
        .lines()
        .chain(standard_error.lines())
        .find_map(|line| line.trim().strip_prefix("note: native-static-libs:"))
    else {
        return Err(BuildError::MissingNativeLinks);
    };

    parse_native_link_arguments(arguments.split_whitespace())
}

fn parse_native_link_arguments<'a>(
    mut arguments: impl Iterator<Item = &'a str>,
) -> Result<Vec<NativeLinkRequirement>, BuildError> {
    let mut requirements = Vec::new();

    while let Some(argument) = arguments.next() {
        let (name, kind) = if argument == "-framework" {
            (
                arguments.next().ok_or(BuildError::InvalidNativeLink)?,
                NativeLinkKind::Framework,
            )
        } else if let Some(name) = argument.strip_prefix("-l") {
            (name, NativeLinkKind::System)
        } else if let Some(name) = argument.strip_prefix("/defaultlib:") {
            (name, NativeLinkKind::System)
        } else if let Some(name) = argument.strip_suffix(".lib") {
            (name, NativeLinkKind::System)
        } else {
            return Err(BuildError::InvalidNativeLink);
        };

        let name = NonEmptySharedStr::try_new(name).ok_or(BuildError::InvalidNativeLink)?;

        requirements.push(NativeLinkRequirement::new(name, kind));
    }

    Ok(requirements)
}

fn profile_directory(profile: &str) -> &str {
    if profile == "dev" { "debug" } else { profile }
}

#[derive(Debug)]
pub(crate) enum BuildError {
    Cargo(std::io::Error),
    BuildFailed { package: String, detail: String },
    MissingArchive(PathBuf),
    MissingNativeLinks,
    InvalidNativeLink,
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cargo(error) => write!(formatter, "could not run Cargo: {error}"),
            Self::BuildFailed { package, detail } => {
                write!(formatter, "native archive build failed for {package}")?;

                if !detail.is_empty() {
                    write!(formatter, "\n{detail}")?;
                }

                Ok(())
            }
            Self::MissingArchive(path) => {
                write!(
                    formatter,
                    "native archive was not produced at {}",
                    path.display()
                )
            }
            Self::MissingNativeLinks => {
                formatter.write_str("rustc did not report native link requirements")
            }
            Self::InvalidNativeLink => {
                formatter.write_str("rustc reported an unsupported native link argument")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::NativeLinkKind;

    use super::{
        NativeTools, native_tools, parse_native_link_arguments, rust_static_library_file_name,
        thin_lto_flags,
    };

    #[test]
    fn optimization_native_tools_follow_the_target_object_format() {
        assert_eq!(
            native_tools(bray_target::NativeTarget::X86_64WindowsMsvc),
            NativeTools {
                compiler: "clang-cl",
                cpp_compiler: "clang-cl",
                archiver: "llvm-lib",
            }
        );

        assert_eq!(
            native_tools(bray_target::NativeTarget::X86_64LinuxGnu),
            NativeTools {
                compiler: "clang",
                cpp_compiler: "clang++",
                archiver: "llvm-ar",
            }
        );

        assert_eq!(
            native_tools(bray_target::NativeTarget::Aarch64MacOs),
            NativeTools {
                compiler: "clang",
                cpp_compiler: "clang++",
                archiver: "llvm-ar",
            }
        );
    }

    #[test]
    fn optimization_native_flags_remove_checkout_identity() {
        assert_eq!(
            thin_lto_flags(std::path::Path::new("C:\\work space\\bray")),
            "-flto=thin \"-ffile-prefix-map=C:/work space/bray=.\" -fdebug-compilation-dir=."
        );
    }

    #[test]
    fn rust_static_library_names_follow_cargo_target_conventions() {
        assert_eq!(
            rust_static_library_file_name(
                bray_target::NativeTarget::X86_64WindowsMsvc,
                "bray-runtime-builtins",
            ),
            "bray_runtime_builtins.lib"
        );

        assert_eq!(
            rust_static_library_file_name(
                bray_target::NativeTarget::X86_64LinuxGnu,
                "bray-runtime-builtins",
            ),
            "libbray_runtime_builtins.a"
        );
    }

    #[test]
    fn rustc_native_link_arguments_preserve_platform_requirements_in_order() {
        let cases = [
            (
                "-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc",
                vec![
                    ("gcc_s", NativeLinkKind::System),
                    ("util", NativeLinkKind::System),
                    ("rt", NativeLinkKind::System),
                    ("pthread", NativeLinkKind::System),
                    ("m", NativeLinkKind::System),
                    ("dl", NativeLinkKind::System),
                    ("c", NativeLinkKind::System),
                ],
            ),
            (
                "kernel32.lib ntdll.lib userenv.lib /defaultlib:msvcrt",
                vec![
                    ("kernel32", NativeLinkKind::System),
                    ("ntdll", NativeLinkKind::System),
                    ("userenv", NativeLinkKind::System),
                    ("msvcrt", NativeLinkKind::System),
                ],
            ),
            (
                "-framework Security -framework CoreFoundation -lSystem",
                vec![
                    ("Security", NativeLinkKind::Framework),
                    ("CoreFoundation", NativeLinkKind::Framework),
                    ("System", NativeLinkKind::System),
                ],
            ),
        ];

        for (arguments, expected) in cases {
            let requirements = parse_native_link_arguments(arguments.split_whitespace())
                .unwrap_or_else(|error| panic!("native links must parse: {error}"));

            assert_eq!(
                requirements
                    .iter()
                    .map(|requirement| (requirement.name(), requirement.kind()))
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
}
