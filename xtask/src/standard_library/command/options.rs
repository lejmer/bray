use std::path::PathBuf;

use bray_compilation::{CompilationProfileConfiguration, CompilationProfileMode};

use super::core::build_selected_targets;
use super::error::BuildError;
use crate::bundle::{NativeBuildOptions, NativeBuildOptionsBuilder};
use crate::workspace;

pub(super) struct BuildOptions {
    native: NativeBuildOptions,
    source: PathBuf,
    profile: Option<BuildProfileOptions>,
}

pub(super) struct BuildProfileOptions {
    pub(super) configuration: CompilationProfileConfiguration,
    pub(super) output: PathBuf,
}

impl BuildOptions {
    pub(super) fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self, BuildError> {
        let mut source = None;
        let mut selected_profile_mode = None;
        let mut profile_output = None;
        let mut native = NativeBuildOptionsBuilder::default();

        while let Some(argument) = arguments.next() {
            if native
                .parse_option(&argument, &mut arguments)
                .map_err(BuildError::BuildOptions)?
            {
                continue;
            }

            match argument.as_str() {
                "--source" if source.is_none() => {
                    source = Some(path_argument(&mut arguments, "--source")?);
                }
                "--profile" if selected_profile_mode.is_none() => {
                    let value = required_argument(&mut arguments, "--profile")?;

                    selected_profile_mode = Some(profile_mode(&value)?);
                }
                "--profile-output" if profile_output.is_none() => {
                    profile_output = Some(path_argument(&mut arguments, "--profile-output")?);
                }
                _ => return Err(BuildError::UnexpectedArgument(argument)),
            }
        }

        let native = native.finish().map_err(BuildError::BuildOptions)?;

        let source = match source {
            Some(source) => source,
            None => workspace::root()
                .map_err(BuildError::Workspace)?
                .join("standard-library"),
        };

        let profile = match (selected_profile_mode, profile_output) {
            (Some(mode), Some(output)) => Some(BuildProfileOptions {
                configuration: CompilationProfileConfiguration::new(mode),
                output,
            }),
            (Some(_), None) => {
                return Err(BuildError::MissingRequiredOption {
                    option: "--profile-output",
                    required_by: "--profile",
                });
            }
            (None, Some(_)) => {
                return Err(BuildError::MissingRequiredOption {
                    option: "--profile",
                    required_by: "--profile-output",
                });
            }
            (None, None) => None,
        };

        Ok(Self {
            native,
            source,
            profile,
        })
    }

    pub(super) fn build(self) -> Result<PathBuf, BuildError> {
        build_selected_targets(
            &self.source,
            self.native.output(),
            self.native.target(),
            self.profile.as_ref(),
        )
    }
}

fn profile_mode(value: &str) -> Result<CompilationProfileMode, BuildError> {
    match value {
        "summary" => Ok(CompilationProfileMode::Summary),
        "trace" => Ok(CompilationProfileMode::Trace),
        _ => Err(BuildError::InvalidProfileMode(value.to_owned())),
    }
}

pub(super) fn path_argument(
    arguments: &mut impl Iterator<Item = String>,
    option: &'static str,
) -> Result<PathBuf, BuildError> {
    required_argument(arguments, option).map(PathBuf::from)
}

pub(super) fn required_argument(
    arguments: &mut impl Iterator<Item = String>,
    option: &'static str,
) -> Result<String, BuildError> {
    arguments.next().ok_or(BuildError::MissingValue(option))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bray_target::NativeTarget;

    use super::BuildOptions;
    use crate::standard_library::command::BuildError;

    #[test]
    fn build_options_require_an_output_and_reject_unknown_arguments() {
        assert!(matches!(
            BuildOptions::parse(std::iter::empty()),
            Err(BuildError::BuildOptions(
                crate::bundle::NativeBuildOptionsError::MissingOutput
            ))
        ));

        assert!(matches!(
            BuildOptions::parse(["--unknown".to_owned()].into_iter()),
            Err(BuildError::UnexpectedArgument(argument)) if argument == "--unknown"
        ));
    }

    #[test]
    fn build_options_accept_explicit_source_and_output_roots() {
        let options = BuildOptions::parse(
            [
                "--source".to_owned(),
                "source".to_owned(),
                "--output".to_owned(),
                "output".to_owned(),
            ]
            .into_iter(),
        )
        .unwrap_or_else(|error| panic!("build options must parse: {error}"));

        assert_eq!(options.source, PathBuf::from("source"));
        assert_eq!(options.native.output(), PathBuf::from("output"));
        assert_eq!(options.native.target(), None);
    }

    #[test]
    fn build_options_accept_one_exact_native_target() {
        let options = BuildOptions::parse(
            [
                "--output".to_owned(),
                "output".to_owned(),
                "--target".to_owned(),
                "x86_64-pc-windows-msvc".to_owned(),
            ]
            .into_iter(),
        )
        .unwrap_or_else(|error| panic!("build options must parse: {error}"));

        assert_eq!(
            options.native.target(),
            Some(NativeTarget::X86_64WindowsMsvc)
        );
    }

    #[test]
    fn build_options_accept_explicit_compiler_profiling() {
        let options = BuildOptions::parse(
            [
                "--output".to_owned(),
                "output".to_owned(),
                "--profile".to_owned(),
                "trace".to_owned(),
                "--profile-output".to_owned(),
                "profiles/standard-library".to_owned(),
            ]
            .into_iter(),
        )
        .unwrap_or_else(|error| panic!("profiled build options must parse: {error}"));

        let profile = options
            .profile
            .unwrap_or_else(|| panic!("compiler profiling must be retained"));

        assert_eq!(
            profile.configuration.mode(),
            bray_compilation::CompilationProfileMode::Trace
        );

        assert_eq!(profile.output, PathBuf::from("profiles/standard-library"));
    }

    #[test]
    fn build_options_require_complete_compiler_profiling_options() {
        assert!(matches!(
            BuildOptions::parse(
                [
                    "--output".to_owned(),
                    "output".to_owned(),
                    "--profile".to_owned(),
                    "summary".to_owned(),
                ]
                .into_iter()
            ),
            Err(BuildError::MissingRequiredOption {
                option: "--profile-output",
                required_by: "--profile",
            })
        ));

        assert!(matches!(
            BuildOptions::parse(
                [
                    "--output".to_owned(),
                    "output".to_owned(),
                    "--profile-output".to_owned(),
                    "profiles/standard-library".to_owned(),
                ]
                .into_iter()
            ),
            Err(BuildError::MissingRequiredOption {
                option: "--profile",
                required_by: "--profile-output",
            })
        ));

        assert!(matches!(
            BuildOptions::parse(
                [
                    "--output".to_owned(),
                    "output".to_owned(),
                    "--profile".to_owned(),
                    "everything".to_owned(),
                    "--profile-output".to_owned(),
                    "profiles/standard-library".to_owned(),
                ]
                .into_iter()
            ),
            Err(BuildError::InvalidProfileMode(mode)) if mode == "everything"
        ));
    }
}
