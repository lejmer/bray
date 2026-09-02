use bray_diagnostics::{
    DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm, DiagnosticFailureField,
    DiagnosticFailureValue, DiagnosticIoErrorKind, DiagnosticNativeProductFailureKind,
};
use bray_runtime_interface::RuntimeArtifactSelectionError;

use super::context::{failure_detail, text_failure_field};

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
                        protected_frame_operation(*operation),
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
        RuntimeArtifactSelectionError::UnreadableArchive {
            component,
            path,
            kind,
        } => Kind::RuntimeSelectionUnreadableArchive(failure_detail(
            "runtime_selection_unreadable_archive",
            [
                text_failure_field("component", component.as_str()),
                text_failure_field("path", path.to_string_lossy()),
                DiagnosticFailureField::new(
                    "io_error",
                    DiagnosticFailureValue::IoErrorKind(DiagnosticIoErrorKind::from(*kind)),
                ),
            ],
        )),
        RuntimeArtifactSelectionError::InvalidArchive { component, path } => {
            Kind::RuntimeSelectionInvalidArchive(failure_detail(
                "runtime_selection_invalid_archive",
                [
                    text_failure_field("component", component.as_str()),
                    text_failure_field("path", path.to_string_lossy()),
                ],
            ))
        }
        RuntimeArtifactSelectionError::ArchiveDigestMismatch {
            component,
            path,
            expected,
            actual,
        } => Kind::RuntimeSelectionArchiveDigestMismatch(failure_detail(
            "runtime_selection_archive_digest_mismatch",
            [
                text_failure_field("component", component.as_str()),
                text_failure_field("path", path.to_string_lossy()),
                DiagnosticFailureField::new(
                    "expected_digest",
                    DiagnosticFailureValue::ArtifactDigest(DiagnosticArtifactDigest::new(
                        DiagnosticArtifactDigestAlgorithm::Sha256,
                        expected.bytes(),
                    )),
                ),
                DiagnosticFailureField::new(
                    "actual_digest",
                    DiagnosticFailureValue::ArtifactDigest(DiagnosticArtifactDigest::new(
                        DiagnosticArtifactDigestAlgorithm::Sha256,
                        actual.bytes(),
                    )),
                ),
            ],
        )),
    }
}

const fn protected_frame_operation(
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
