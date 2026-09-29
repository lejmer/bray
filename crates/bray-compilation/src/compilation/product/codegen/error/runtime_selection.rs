use bray_diagnostics::DiagnosticNativeProductFailureKind;
use bray_runtime_interface::RuntimeArtifactSelectionError;

use super::context::{
    failure_detail, protected_frame_abi_operation, text_failure_field,
};

pub(super) fn runtime_selection_failure_kind(
    error: &RuntimeArtifactSelectionError,
) -> DiagnosticNativeProductFailureKind {
    use DiagnosticNativeProductFailureKind as Kind;
    use bray_runtime_interface::RuntimeCompatibilityError as Compatibility;

    match error {
        RuntimeArtifactSelectionError::IncompatibleRuntime(error) => {
            let (reason, context) = match error {
                Compatibility::RuntimeIdentity => {
                    ("runtime_selection_runtime_identity_mismatch", Vec::new())
                }
                Compatibility::RuntimeAbi => ("runtime_selection_runtime_abi_mismatch", Vec::new()),
                Compatibility::Target => ("runtime_selection_target_mismatch", Vec::new()),
                Compatibility::PanicAbi => ("runtime_selection_panic_abi_mismatch", Vec::new()),
                Compatibility::FrameAbi(operation) => (
                    "runtime_selection_frame_abi_mismatch",
                    vec![text_failure_field(
                        "frame_operation",
                        protected_frame_abi_operation(*operation),
                    )],
                ),
                Compatibility::MissingCapability(capability) => (
                    "runtime_selection_missing_capability",
                    vec![text_failure_field("capability", capability.as_str())],
                ),
                Compatibility::MissingRole(role) => (
                    "runtime_selection_missing_role",
                    vec![text_failure_field("role", role.as_str())],
                ),
            };

            Kind::RuntimeSelectionIncompatible(failure_detail(reason, context))
        }
        RuntimeArtifactSelectionError::MissingRoleOwner(role) => {
            Kind::RuntimeSelectionMissingRoleOwner(failure_detail(
                "runtime_selection_missing_role_owner",
                [text_failure_field("role", role.as_str())],
            ))
        }
        RuntimeArtifactSelectionError::MissingCapabilityOwner(capability) => {
            Kind::RuntimeSelectionMissingCapabilityOwner(failure_detail(
                "runtime_selection_missing_capability_owner",
                [text_failure_field("capability", capability.as_str())],
            ))
        }
        RuntimeArtifactSelectionError::NativeResolution(error) => {
            let (reason, symbol) = match error {
                bray_native_artifact::NativeResolutionError::Unresolved(symbol) =>
                    ("runtime_native_symbol_unresolved", symbol),
                bray_native_artifact::NativeResolutionError::DuplicateStrong(symbol) =>
                    ("runtime_native_symbol_duplicate", symbol),
            };

            Kind::RuntimeSelectionIncompatible(failure_detail(
                reason,
                [text_failure_field("symbol", symbol.identity().name()
                    .map(str::to_owned)
                    .unwrap_or_else(|| symbol.identity().ordinal()
                        .expect("native symbol must have a name or ordinal").to_string()))],
            ))
        }
    }
}
