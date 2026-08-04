use bray_codegen::{CodegenOptions, DebugInformationMode, OptimizationLevel, SizePreference};

/// Coherent generation and linking policy for one native product build.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BuildConfiguration {
    /// Favor fast generation and source-level debugging.
    #[default]
    Development,
    /// Favor generated-code performance and omit debug information.
    Release,
}

impl BuildConfiguration {
    /// Returns whether linked debug information uses a companion artifact.
    pub const fn requires_linked_debug_companion(
        self,
        object_format: bray_target::ObjectFormat,
    ) -> bool {
        matches!(self, Self::Development)
            && matches!(
                object_format,
                bray_target::ObjectFormat::Coff | bray_target::ObjectFormat::MachO
            )
    }

    pub(super) const fn codegen_options(self) -> CodegenOptions {
        match self {
            Self::Development => CodegenOptions::new(
                OptimizationLevel::Basic,
                SizePreference::None,
                DebugInformationMode::LineTables,
            ),
            Self::Release => CodegenOptions::new(
                OptimizationLevel::Full,
                SizePreference::None,
                DebugInformationMode::None,
            ),
        }
    }

    pub(super) const fn preserves_unused_link_content(self) -> bool {
        matches!(self, Self::Development)
    }
}

#[cfg(test)]
mod tests {
    use bray_codegen::{DebugInformationMode, OptimizationLevel};

    use super::BuildConfiguration;

    #[test]
    fn configurations_select_distinct_codegen_policy() {
        let development = BuildConfiguration::Development.codegen_options();
        let release = BuildConfiguration::Release.codegen_options();

        assert_eq!(development.optimization(), OptimizationLevel::Basic);

        assert_eq!(
            development.debug_information(),
            DebugInformationMode::LineTables
        );

        assert_eq!(release.optimization(), OptimizationLevel::Full);
        assert_eq!(release.debug_information(), DebugInformationMode::None);
    }

    #[test]
    fn development_builds_require_durable_platform_debug_companions() {
        let development = BuildConfiguration::Development;

        assert!(development.requires_linked_debug_companion(bray_target::ObjectFormat::Coff));
        assert!(development.requires_linked_debug_companion(bray_target::ObjectFormat::MachO));
        assert!(!development.requires_linked_debug_companion(bray_target::ObjectFormat::Elf));

        assert!(
            !BuildConfiguration::Release
                .requires_linked_debug_companion(bray_target::ObjectFormat::Coff)
        );
    }
}
