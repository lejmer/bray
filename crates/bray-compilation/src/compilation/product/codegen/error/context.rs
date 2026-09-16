use bray_diagnostics::{
    DiagnosticFailureField, DiagnosticFailureValue, DiagnosticNativeProductFailureDetail,
};

pub(super) fn failure_detail(
    reason: &'static str,
    context: impl Into<Box<[DiagnosticFailureField]>>,
) -> DiagnosticNativeProductFailureDetail {
    DiagnosticNativeProductFailureDetail::new(reason, context)
}

pub(super) fn text_failure_field(
    name: &'static str,
    value: impl Into<String>,
) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Text(value.into()))
}

pub(super) fn path_failure_field(
    name: &'static str,
    value: impl Into<std::path::PathBuf>,
) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Path(value.into()))
}

pub(super) fn identity_failure_detail(
    reason: &'static str,
    name: &'static str,
    identity: &impl std::hash::Hash,
) -> DiagnosticNativeProductFailureDetail {
    failure_detail(
        reason,
        [crate::fact::diagnostic_context::identity_field(
            name, identity,
        )],
    )
}

pub(super) const fn protected_frame_abi_operation(
    operation: bray_runtime_interface::ProtectedFrameAbiOperation,
) -> &'static str {
    use bray_runtime_interface::ProtectedFrameAbiOperation as Operation;

    match operation {
        Operation::Resume => "resume",
        Operation::TaskBroadcast => "task_broadcast",
        Operation::LifecycleResolution => "lifecycle_resolution",
        Operation::CompletionMove => "completion_move",
        Operation::Destruction => "destruction",
    }
}
