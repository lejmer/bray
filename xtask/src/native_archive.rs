use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use bray_base::NonEmptySharedStr;
use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
use bray_target::NativeTarget;

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
    archive_file_name: &str,
    features: &[&str],
) -> Result<RustStaticLibrary, BuildError> {
    let target_directory = root.join("target");
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

    command.args(["--", "--print", "native-static-libs"]);
    configure_cross_c_toolchain(&mut command, root, target);

    let output = command.output().map_err(BuildError::Cargo)?;

    if !output.status.success() {
        return Err(BuildError::BuildFailed(package.to_owned()));
    }

    let native_links = native_link_requirements(&output)?;

    let archive = target_directory
        .join(target.as_str())
        .join(profile_directory(profile))
        .join(archive_file_name);

    if !archive.is_file() {
        return Err(BuildError::MissingArchive(archive));
    }

    Ok(RustStaticLibrary {
        archive,
        native_links,
    })
}

fn configure_cross_c_toolchain(command: &mut Command, root: &Path, target: NativeTarget) {
    if !cfg!(windows) || target != NativeTarget::X86_64LinuxGnu {
        return;
    }

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
    BuildFailed(String),
    MissingArchive(PathBuf),
    MissingNativeLinks,
    InvalidNativeLink,
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cargo(error) => write!(formatter, "could not run Cargo: {error}"),
            Self::BuildFailed(package) => {
                write!(formatter, "native archive build failed for {package}")
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

    use super::parse_native_link_arguments;

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
