pub(crate) const fn format_english_dependency_subject(
    kind: bray_diagnostics::DiagnosticDependencySubjectKind,
) -> &'static str {
    use bray_diagnostics::DiagnosticDependencySubjectKind;

    match kind {
        DiagnosticDependencySubjectKind::Storage => "storage",
        DiagnosticDependencySubjectKind::StorageAccess => "storage access",
        DiagnosticDependencySubjectKind::BorrowCapability => "borrow",
        DiagnosticDependencySubjectKind::ScopedCapability => "scoped capability",
        DiagnosticDependencySubjectKind::SelectedImplementation => "selected implementation",
        DiagnosticDependencySubjectKind::ProductStatic => "product-static storage",
        DiagnosticDependencySubjectKind::ExactThreadStatic => "thread-local static storage",
        DiagnosticDependencySubjectKind::LifecycleObligation => "lifecycle obligation",
        DiagnosticDependencySubjectKind::SuspensionState => "the values and borrows at this point",
    }
}

pub(crate) const fn format_english_dependency_requirement(
    kind: bray_diagnostics::DiagnosticDependencyRequirementKind,
) -> &'static str {
    use bray_diagnostics::DiagnosticDependencyRequirementKind;

    match kind {
        DiagnosticDependencyRequirementKind::StorageAlive => "remain alive",
        DiagnosticDependencyRequirementKind::ValueDependencies => {
            "preserve the dependencies carried by its value"
        }
        DiagnosticDependencyRequirementKind::StorageInitialized => "remain initialized",
        DiagnosticDependencyRequirementKind::SharedBorrowActive => "retain its shared borrow",
        DiagnosticDependencyRequirementKind::MutableBorrowActive => "retain its mutable borrow",
        DiagnosticDependencyRequirementKind::ExclusiveMutationAuthority => {
            "retain exclusive mutation authority"
        }
        DiagnosticDependencyRequirementKind::ScopedCapabilityLive => "remain live",
        DiagnosticDependencyRequirementKind::DestructionAttached => {
            "retain its destruction obligation"
        }
        DiagnosticDependencyRequirementKind::FinalizationAttached => {
            "retain its finalization obligation"
        }
        DiagnosticDependencyRequirementKind::CancellationAttached => {
            "retain its cancellation obligation"
        }
        DiagnosticDependencyRequirementKind::JoiningAttached => "retain its joining obligation",
        DiagnosticDependencyRequirementKind::SuspensionStateAvailable => {
            "be proven valid across suspension"
        }
    }
}
