use serde::Serialize;

use super::candidate::DiagnosticSelectionCandidateJson;
use super::contract_mismatch::receiver_mode_key;
use super::super::DiagnosticTypeJson;

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticSelectionCandidatesJson {
    candidates: Vec<DiagnosticSelectionCandidateJson>,
    omitted_count: u64,
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticSelectionRejectionsJson {
    rejections: Vec<DiagnosticSelectionRejectionJson>,
    omitted_count: u64,
}

#[derive(Serialize)]
struct DiagnosticSelectionRejectionJson {
    candidate: DiagnosticSelectionCandidateJson,
    mismatch: DiagnosticSelectionRejectionReasonJson,
}

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
enum DiagnosticSelectionRejectionReasonJson {
    GenericArgumentCount { provided: u64, maximum: u64 },
    ReceiverPresence { provided: bool, required: bool },
    ReceiverType {
        provided: DiagnosticTypeJson,
        required: DiagnosticTypeJson,
    },
    ReceiverCapability {
        provided: &'static str,
        required: &'static str,
    },
    CallableArgument {
        mismatch: DiagnosticCallableArgumentRejectionJson,
    },
    OperandTypes { provided: Vec<DiagnosticTypeJson> },
    ConstructionInput {
        mismatch: DiagnosticConstructionInputRejectionJson,
    },
    ExpressionForm,
    RequiredImplementation,
    RequiredLanguageOperation,
}

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
enum DiagnosticCallableArgumentRejectionJson {
    PositionalAfterNamed { ordinal: u64 },
    UnknownName { provided: String, accepted: Vec<String> },
    PositionalUnavailable { ordinal: u64 },
    Duplicate { name: Option<String>, ordinal: u64 },
    Type {
        name: Option<String>,
        ordinal: u64,
        expected: DiagnosticTypeJson,
        actual: DiagnosticTypeJson,
    },
    Missing {
        name: String,
        ordinal: u64,
        expected: DiagnosticTypeJson,
    },
}

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
enum DiagnosticConstructionInputRejectionJson {
    PositionalAfterNamed,
    UnknownName { provided: String, accepted: Vec<String> },
    PositionalUnavailable { ordinal: u64 },
    Duplicate { name: Option<String>, ordinal: u64 },
    Type {
        name: Option<String>,
        ordinal: u64,
        expected: DiagnosticTypeJson,
        actual: DiagnosticTypeJson,
    },
    Missing {
        name: String,
        ordinal: u64,
        expected: DiagnosticTypeJson,
    },
}

impl DiagnosticSelectionCandidatesJson {
    pub(in crate::output::diagnostic::json) fn from_candidates(
        candidates: &bray_diagnostics::DiagnosticSelectionCandidates,
    ) -> Self {
        Self {
            candidates: candidates
                .candidates()
                .iter()
                .map(DiagnosticSelectionCandidateJson::from_candidate)
                .collect(),
            omitted_count: candidates.omitted_count(),
        }
    }
}

impl DiagnosticSelectionRejectionsJson {
    pub(in crate::output::diagnostic::json) fn from_rejections(
        rejections: &bray_diagnostics::DiagnosticSelectionRejections,
    ) -> Self {
        Self {
            rejections: rejections
                .rejections()
                .iter()
                .map(DiagnosticSelectionRejectionJson::from_rejection)
                .collect(),
            omitted_count: rejections.omitted_count(),
        }
    }
}

impl DiagnosticSelectionRejectionJson {
    fn from_rejection(
        rejection: &bray_diagnostics::DiagnosticRejectedSelectionCandidate,
    ) -> Self {
        Self {
            candidate: DiagnosticSelectionCandidateJson::from_candidate(rejection.candidate()),
            mismatch: DiagnosticSelectionRejectionReasonJson::from_reason(rejection.reason()),
        }
    }
}

impl DiagnosticSelectionRejectionReasonJson {
    fn from_reason(reason: &bray_diagnostics::DiagnosticSelectionRejectionReason) -> Self {
        use bray_diagnostics::DiagnosticSelectionRejectionReason as Reason;

        match reason {
            Reason::GenericArgumentCount { provided, maximum } => Self::GenericArgumentCount {
                provided: *provided,
                maximum: *maximum,
            },
            Reason::ReceiverPresence { provided, required } => Self::ReceiverPresence {
                provided: *provided,
                required: *required,
            },
            Reason::ReceiverType { provided, required } => Self::ReceiverType {
                provided: DiagnosticTypeJson::from_type(provided),
                required: DiagnosticTypeJson::from_type(required),
            },
            Reason::ReceiverCapability { provided, required } => Self::ReceiverCapability {
                provided: match provided {
                    bray_diagnostics::DiagnosticReceiverCapability::Shared => "shared",
                    bray_diagnostics::DiagnosticReceiverCapability::Mutable => "mutable",
                    bray_diagnostics::DiagnosticReceiverCapability::Owned => "owned",
                    bray_diagnostics::DiagnosticReceiverCapability::OwnedMutable => {
                        "owned_mutable"
                    }
                },
                required: receiver_mode_key(*required),
            },
            Reason::CallableArgument(reason) => Self::CallableArgument {
                mismatch: DiagnosticCallableArgumentRejectionJson::from_reason(reason),
            },
            Reason::OperandTypes { provided } => Self::OperandTypes {
                provided: provided.iter().map(DiagnosticTypeJson::from_type).collect(),
            },
            Reason::ConstructionInput(reason) => Self::ConstructionInput {
                mismatch: DiagnosticConstructionInputRejectionJson::from_reason(reason),
            },
            Reason::ExpressionForm => Self::ExpressionForm,
            Reason::RequiredImplementation => Self::RequiredImplementation,
            Reason::RequiredLanguageOperation => Self::RequiredLanguageOperation,
        }
    }
}

impl DiagnosticCallableArgumentRejectionJson {
    fn from_reason(reason: &bray_diagnostics::DiagnosticCallableArgumentRejection) -> Self {
        use bray_diagnostics::DiagnosticCallableArgumentRejection as Reason;

        match reason {
            Reason::PositionalAfterNamed { ordinal } => {
                Self::PositionalAfterNamed { ordinal: *ordinal }
            }
            Reason::UnknownName { provided, accepted } => Self::UnknownName {
                provided: provided.clone(),
                accepted: accepted.to_vec(),
            },
            Reason::PositionalUnavailable { ordinal } => {
                Self::PositionalUnavailable { ordinal: *ordinal }
            }
            Reason::Duplicate { name, ordinal } => Self::Duplicate {
                name: name.clone(),
                ordinal: *ordinal,
            },
            Reason::Type { name, ordinal, expected, actual } => Self::Type {
                name: name.clone(),
                ordinal: *ordinal,
                expected: DiagnosticTypeJson::from_type(expected),
                actual: DiagnosticTypeJson::from_type(actual),
            },
            Reason::Missing { name, ordinal, expected } => Self::Missing {
                name: name.clone(),
                ordinal: *ordinal,
                expected: DiagnosticTypeJson::from_type(expected),
            },
        }
    }
}

impl DiagnosticConstructionInputRejectionJson {
    fn from_reason(reason: &bray_diagnostics::DiagnosticConstructionInputRejection) -> Self {
        use bray_diagnostics::DiagnosticConstructionInputRejection as Reason;

        match reason {
            Reason::PositionalAfterNamed => Self::PositionalAfterNamed,
            Reason::UnknownName { provided, accepted } => Self::UnknownName {
                provided: provided.clone(),
                accepted: accepted.to_vec(),
            },
            Reason::PositionalUnavailable { ordinal } => {
                Self::PositionalUnavailable { ordinal: *ordinal }
            }
            Reason::Duplicate { name, ordinal } => Self::Duplicate {
                name: name.clone(),
                ordinal: *ordinal,
            },
            Reason::Type { name, ordinal, expected, actual } => Self::Type {
                name: name.clone(),
                ordinal: *ordinal,
                expected: DiagnosticTypeJson::from_type(expected),
                actual: DiagnosticTypeJson::from_type(actual),
            },
            Reason::Missing { name, ordinal, expected } => Self::Missing {
                name: name.clone(),
                ordinal: *ordinal,
                expected: DiagnosticTypeJson::from_type(expected),
            },
        }
    }
}
