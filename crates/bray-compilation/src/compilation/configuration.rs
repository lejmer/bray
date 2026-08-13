use bray_codegen::{
    CodegenOptions, DebugInformationMode, OptimizationLevel, RuntimeObservationMode,
    SizePreference,
};

impl super::Compilation {
    pub(super) fn package_implementation_configuration(
        &self,
        selected_runtime: Option<bray_runtime_interface::RuntimeIdentity>,
    ) -> Result<
        bray_package_interface::PackageImplementationConfiguration,
        bray_codegen::CodegenTargetBuildError,
    > {
        let selected_target = self.selected_target().target();
        let codegen_target = selected_target.codegen_target()?;

        // The target profile and panic ABI are immutable identity values owned by the bundle.
        let mir_target = bray_ir::MirTargetFacts::new(
            selected_target.profile().clone(),
            selected_target.runtime_abi(),
        );

        Ok(
            bray_package_interface::PackageImplementationConfiguration::for_mir_target(
                &mir_target,
                selected_runtime,
                codegen_target.panic_abi().clone(),
            ),
        )
    }
}

/// Coherent generation and linking policy for one native product build.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BuildConfiguration {
    /// Favor fast generation and source-level debugging.
    #[default]
    Development,
    /// Favor generated-code performance and omit debug information.
    Release,
    /// Preserve release behavior while observing generated memory work.
    ObservedRelease,
    /// Preserve release behavior while observing the generated root interval.
    TimedRelease,
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
                bray_codegen::ReproducibilityLevel::ByteForByte,
                RuntimeObservationMode::None,
            ),
            Self::Release => CodegenOptions::new(
                OptimizationLevel::Full,
                SizePreference::None,
                DebugInformationMode::None,
                bray_codegen::ReproducibilityLevel::ByteForByte,
                RuntimeObservationMode::None,
            ),
            Self::ObservedRelease => CodegenOptions::new(
                OptimizationLevel::Full,
                SizePreference::None,
                DebugInformationMode::None,
                bray_codegen::ReproducibilityLevel::ByteForByte,
                RuntimeObservationMode::Memory,
            ),
            Self::TimedRelease => CodegenOptions::new(
                OptimizationLevel::Full,
                SizePreference::None,
                DebugInformationMode::None,
                bray_codegen::ReproducibilityLevel::ByteForByte,
                RuntimeObservationMode::PerformanceInterval,
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
        let observed = BuildConfiguration::ObservedRelease.codegen_options();
        let timed = BuildConfiguration::TimedRelease.codegen_options();

        assert_eq!(development.optimization(), OptimizationLevel::Basic);

        assert_eq!(
            development.debug_information(),
            DebugInformationMode::LineTables
        );

        assert_eq!(release.optimization(), OptimizationLevel::Full);
        assert_eq!(release.debug_information(), DebugInformationMode::None);

        assert_eq!(
            observed.runtime_observations(),
            bray_codegen::RuntimeObservationMode::Memory
        );

        assert_eq!(
            timed.runtime_observations(),
            bray_codegen::RuntimeObservationMode::PerformanceInterval
        );
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

        assert!(
            !BuildConfiguration::ObservedRelease
                .requires_linked_debug_companion(bray_target::ObjectFormat::Coff)
        );

        assert!(
            !BuildConfiguration::TimedRelease
                .requires_linked_debug_companion(bray_target::ObjectFormat::Coff)
        );
    }
}
