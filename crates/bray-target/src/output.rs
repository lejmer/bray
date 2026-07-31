use std::sync::Arc;

use bray_base::shared_str;

use crate::{NativeTarget, ObjectFormat, TargetIdentity, TargetMachineProperties, TargetProfile};

/// Artifact categories whose external names are selected by target policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetOutputKind {
    /// Human-readable target assembly.
    Assembly,
    /// Human-readable backend low-level IR.
    BackendIr,
    /// Backend-owned binary IR or bitcode.
    BackendBitcode,
    /// Relocatable native object.
    RelocatableObject,
    /// Backend-owned directly executable target module.
    ExecutableModule,
    /// Backend-owned separately stored debug data.
    DebugCompanion,
    /// Compiled package interface.
    PackageInterface,
    /// Compiler-owned dependency metadata.
    DependencyMetadata,
    /// Final executable product.
    Executable,
    /// Final static library product.
    StaticLibrary,
    /// Final shared library product.
    SharedLibrary,
    /// Target-required companion to a linked product.
    LinkedCompanion,
}

/// Target-selected prefix and suffix for one external artifact category.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetOutputName {
    kind: TargetOutputKind,
    prefix: Arc<str>,
    suffix: Arc<str>,
}

impl TargetOutputName {
    /// Returns the filename rule for one native target output.
    pub fn for_native(format: ObjectFormat, kind: TargetOutputKind) -> Self {
        let (prefix, suffix) = native_name_fragments(format, kind);

        Self::try_new(kind, prefix, suffix)
            .unwrap_or_else(|error| panic!("native output name must be valid: {error:?}"))
    }

    /// Creates a naming rule when both fragments are valid filename components.
    pub fn try_new(
        kind: TargetOutputKind,
        prefix: impl Into<Arc<str>>,
        suffix: impl Into<Arc<str>>,
    ) -> Result<Self, TargetOutputNameBuildError> {
        let prefix = shared_str(prefix);
        let suffix = shared_str(suffix);

        if !is_valid_name_fragment(&prefix) {
            return Err(TargetOutputNameBuildError::InvalidPrefix);
        }

        if !is_valid_name_fragment(&suffix) {
            return Err(TargetOutputNameBuildError::InvalidSuffix);
        }

        Ok(Self {
            kind,
            prefix,
            suffix,
        })
    }

    /// Returns the artifact category covered by this naming rule.
    pub const fn kind(&self) -> TargetOutputKind {
        self.kind
    }

    /// Returns the text placed before the product-derived name stem.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Returns the text placed after the product-derived name stem.
    pub fn suffix(&self) -> &str {
        &self.suffix
    }

    /// Applies this target rule when the supplied stem is one filename component.
    pub fn file_name(&self, stem: &str) -> Option<String> {
        is_valid_name_stem(stem).then(|| format!("{}{stem}{}", self.prefix, self.suffix))
    }
}

/// Validated target and external artifact naming facts used by output phases.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetOutputDescription {
    profile: TargetProfile,
    names: Arc<[TargetOutputName]>,
}

impl TargetOutputDescription {
    /// Creates output naming facts for one native toolchain target.
    pub fn for_native(
        target: NativeTarget,
        kinds: impl IntoIterator<Item = TargetOutputKind>,
    ) -> Self {
        let format = target.object_format();

        Self::try_new(
            target.profile(),
            kinds
                .into_iter()
                .map(|kind| TargetOutputName::for_native(format, kind)),
        )
        .unwrap_or_else(|error| panic!("native output description must be valid: {error:?}"))
    }

    /// Creates target output facts with at most one naming rule per artifact category.
    pub fn try_new(
        profile: TargetProfile,
        names: impl IntoIterator<Item = TargetOutputName>,
    ) -> Result<Self, TargetOutputDescriptionBuildError> {
        let mut names: Vec<_> = names.into_iter().collect();

        names.sort_unstable_by_key(TargetOutputName::kind);

        if names.is_empty() {
            return Err(TargetOutputDescriptionBuildError::MissingNames);
        }

        if let Some(pair) = names
            .windows(2)
            .find(|pair| pair[0].kind() == pair[1].kind())
        {
            return Err(TargetOutputDescriptionBuildError::DuplicateKind(
                pair[0].kind(),
            ));
        }

        Ok(Self {
            profile,
            names: names.into(),
        })
    }

    /// Returns the target identity covered by these output facts.
    pub const fn identity(&self) -> &TargetIdentity {
        self.profile.identity()
    }

    /// Returns the machine properties relevant to backend output compatibility.
    pub const fn machine(&self) -> &TargetMachineProperties {
        self.profile.machine()
    }

    /// Returns the target profile covered by these output facts.
    pub const fn profile(&self) -> &TargetProfile {
        &self.profile
    }

    /// Returns naming rules in canonical artifact-category order.
    pub fn names(&self) -> &[TargetOutputName] {
        &self.names
    }

    /// Returns the naming rule for one artifact category.
    pub fn name(&self, kind: TargetOutputKind) -> Option<&TargetOutputName> {
        self.names
            .binary_search_by_key(&kind, TargetOutputName::kind)
            .ok()
            .map(|index| &self.names[index])
    }
}

/// A contract violation in one target output naming rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetOutputNameBuildError {
    /// The filename prefix contains a path separator or null character.
    InvalidPrefix,
    /// The filename suffix contains a path separator or null character.
    InvalidSuffix,
}

/// A contract violation in a complete target output description.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetOutputDescriptionBuildError {
    /// No artifact naming rules were supplied.
    MissingNames,
    /// More than one rule covers the same artifact category.
    DuplicateKind(TargetOutputKind),
}

fn is_valid_name_fragment(fragment: &str) -> bool {
    !fragment.contains(['/', '\\', '\0'])
}

fn is_valid_name_stem(stem: &str) -> bool {
    !stem.is_empty() && stem != "." && stem != ".." && is_valid_name_fragment(stem)
}

const fn native_name_fragments(
    format: ObjectFormat,
    kind: TargetOutputKind,
) -> (&'static str, &'static str) {
    match kind {
        TargetOutputKind::Assembly => ("", ".s"),
        TargetOutputKind::BackendIr => ("", ".ll"),
        TargetOutputKind::BackendBitcode => ("", ".bc"),
        TargetOutputKind::RelocatableObject => match format {
            ObjectFormat::Coff => ("", ".obj"),
            ObjectFormat::Elf | ObjectFormat::MachO => ("", ".o"),
            ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {
                panic!("native target must use COFF, ELF, or Mach-O")
            }
        },
        TargetOutputKind::ExecutableModule => ("", ".wasm"),
        TargetOutputKind::DebugCompanion => match format {
            ObjectFormat::Coff => ("", ".pdb"),
            ObjectFormat::Elf => ("", ".debug"),
            ObjectFormat::MachO => ("", ".dSYM"),
            ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {
                panic!("native target must use COFF, ELF, or Mach-O")
            }
        },
        TargetOutputKind::PackageInterface => ("", ".brayi"),
        TargetOutputKind::DependencyMetadata => ("", ".brayd"),
        TargetOutputKind::Executable => match format {
            ObjectFormat::Coff => ("", ".exe"),
            ObjectFormat::Elf | ObjectFormat::MachO => ("", ""),
            ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {
                panic!("native target must use COFF, ELF, or Mach-O")
            }
        },
        TargetOutputKind::StaticLibrary => match format {
            ObjectFormat::Coff => ("", ".lib"),
            ObjectFormat::Elf | ObjectFormat::MachO => ("lib", ".a"),
            ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {
                panic!("native target must use COFF, ELF, or Mach-O")
            }
        },
        TargetOutputKind::SharedLibrary => match format {
            ObjectFormat::Coff => ("", ".dll"),
            ObjectFormat::Elf => ("lib", ".so"),
            ObjectFormat::MachO => ("lib", ".dylib"),
            ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {
                panic!("native target must use COFF, ELF, or Mach-O")
            }
        },
        TargetOutputKind::LinkedCompanion => ("", ".companion"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TargetOutputDescription, TargetOutputDescriptionBuildError, TargetOutputKind,
        TargetOutputName, TargetOutputNameBuildError,
    };
    use crate::test_support::test_target_profile;
    use crate::{NativeTarget, ObjectFormat};

    #[test]
    fn output_names_reject_path_fragments() {
        assert_eq!(
            TargetOutputName::try_new(TargetOutputKind::Executable, "../", ".exe"),
            Err(TargetOutputNameBuildError::InvalidPrefix)
        );

        assert_eq!(
            TargetOutputName::try_new(TargetOutputKind::Executable, "", "/program"),
            Err(TargetOutputNameBuildError::InvalidSuffix)
        );
    }

    #[test]
    fn output_descriptions_are_canonical_and_unique() {
        let executable = output_name(TargetOutputKind::Executable, "", "");
        let object = output_name(TargetOutputKind::RelocatableObject, "", ".o");

        let Ok(description) = TargetOutputDescription::try_new(
            test_target_profile(),
            [object.clone(), executable.clone()],
        ) else {
            panic!("test target output description must be valid");
        };

        assert_eq!(description.names(), &[object.clone(), executable.clone()]);

        assert_eq!(
            description
                .name(TargetOutputKind::RelocatableObject)
                .and_then(|name| name.file_name("application")),
            Some(String::from("application.o"))
        );

        assert_eq!(
            description
                .name(TargetOutputKind::Executable)
                .and_then(|name| name.file_name("../application")),
            None
        );

        assert_eq!(
            TargetOutputDescription::try_new(test_target_profile(), [object.clone(), object]),
            Err(TargetOutputDescriptionBuildError::DuplicateKind(
                TargetOutputKind::RelocatableObject
            ))
        );
    }

    #[test]
    fn native_output_names_follow_each_object_platform() {
        let cases = [
            (
                ObjectFormat::Coff,
                ["application.obj", "application.exe", "application.lib", "application.dll"],
            ),
            (
                ObjectFormat::Elf,
                ["application.o", "application", "libapplication.a", "libapplication.so"],
            ),
            (
                ObjectFormat::MachO,
                [
                    "application.o",
                    "application",
                    "libapplication.a",
                    "libapplication.dylib",
                ],
            ),
        ];

        for (format, expected) in cases {
            let actual = [
                TargetOutputKind::RelocatableObject,
                TargetOutputKind::Executable,
                TargetOutputKind::StaticLibrary,
                TargetOutputKind::SharedLibrary,
            ]
            .map(|kind| {
                TargetOutputName::for_native(format, kind)
                    .file_name("application")
                    .unwrap_or_else(|| panic!("native output name must accept a valid stem"))
            });

            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn native_output_descriptions_cover_the_supported_matrix() {
        for target in NativeTarget::ALL {
            let description = TargetOutputDescription::for_native(
                target,
                [
                    TargetOutputKind::RelocatableObject,
                    TargetOutputKind::Executable,
                ],
            );

            assert_eq!(description.identity().as_str(), target.as_str());
            assert_eq!(description.machine().object_format(), target.object_format());
        }
    }

    fn output_name(kind: TargetOutputKind, prefix: &str, suffix: &str) -> TargetOutputName {
        let Ok(name) = TargetOutputName::try_new(kind, prefix, suffix) else {
            panic!("test target output name must be valid");
        };

        name
    }
}
