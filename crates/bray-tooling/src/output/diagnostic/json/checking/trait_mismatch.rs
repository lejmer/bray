use serde::Serialize;

use super::super::DiagnosticTypeJson;
use super::contract_mismatch::{
    DiagnosticCallableContractMismatchJson, DiagnosticGenericConstraintMismatchJson,
      
      generic_parameter_category_key, receiver_mode_key,
};

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticTraitFulfillmentMismatchJson {
    MemberCategory,
    FulfillmentIsNotGeneric,
    GenericParameterCount {
        required: u64,
        provided: u64,
    },
    GenericParameterCategory {
        ordinal: u64,
        required: &'static str,
        provided: &'static str,
    },
    GenericConstantParameterType {
        ordinal: u64,
        required: DiagnosticTypeJson,
        provided: DiagnosticTypeJson,
    },
    GenericConstraints {
        mismatch: DiagnosticGenericConstraintMismatchJson,
    },
    Receiver {
        required: Option<&'static str>,
        provided: Option<&'static str>,
    },
    CallableConstness {
        required: &'static str,
        provided: &'static str,
    },
    CallableExecution {
        required: &'static str,
        provided: &'static str,
    },
    CallableTrust {
        required: &'static str,
        provided: &'static str,
    },
    CallableAbi {
        required: &'static str,
        provided: &'static str,
    },
    CallableParameterCount {
        required: u64,
        provided: u64,
    },
    CallableParameterName {
        ordinal: u64,
        required: String,
        provided: String,
    },
    CallableParameterPosition {
        ordinal: u64,
        required: &'static str,
        provided: &'static str,
    },
    CallableParameterMode {
        ordinal: u64,
        required: &'static str,
        provided: &'static str,
    },
    CallableParameterType {
        ordinal: u64,
        required: DiagnosticTypeJson,
        provided: DiagnosticTypeJson,
    },
    CallableResultType {
        required: DiagnosticTypeJson,
        provided: DiagnosticTypeJson,
    },
    CallableParameterDefault {
        ordinal: u64,
        required: bool,
        provided: bool,
    },
    CallableContract {
        mismatch: DiagnosticCallableContractMismatchJson,
    },
    ConstantType {
        required: DiagnosticTypeJson,
        provided: DiagnosticTypeJson,
    },
    TypeValueUnavailable,
    PredicateTrust {
        required: bool,
        provided: bool,
    },
    PredicateParameterCount {
        required: u64,
        provided: u64,
    },
    PredicateParameterName {
        ordinal: u64,
        required: String,
        provided: String,
    },
    PredicateParameterType {
        ordinal: u64,
        required: DiagnosticTypeJson,
        provided: DiagnosticTypeJson,
    },
}

impl DiagnosticTraitFulfillmentMismatchJson {
    pub(in crate::output::diagnostic::json) fn from_mismatch(
        mismatch: &bray_diagnostics::DiagnosticTraitFulfillmentMismatch,
    ) -> Self {
        use bray_diagnostics::DiagnosticTraitFulfillmentMismatch as Mismatch;

        match mismatch {
            Mismatch::MemberCategory => Self::MemberCategory,
            Mismatch::FulfillmentIsNotGeneric => Self::FulfillmentIsNotGeneric,
            Mismatch::GenericParameterCount { required, provided } => Self::GenericParameterCount {
                required: *required,
                provided: *provided,
            },
            Mismatch::GenericParameterCategory {
                ordinal,
                required,
                provided,
            } => Self::GenericParameterCategory {
                ordinal: *ordinal,
                required: generic_parameter_category_key(*required),
                provided: generic_parameter_category_key(*provided),
            },
            Mismatch::GenericConstantParameterType {
                ordinal,
                required,
                provided,
            } => Self::GenericConstantParameterType {
                ordinal: *ordinal,
                required: DiagnosticTypeJson::from_type(required),
                provided: DiagnosticTypeJson::from_type(provided),
            },
            Mismatch::GenericConstraints(mismatch) => Self::GenericConstraints {
                mismatch: DiagnosticGenericConstraintMismatchJson::from_mismatch(mismatch),
            },
            Mismatch::Receiver { required, provided } => Self::Receiver {
                required: required.map(receiver_mode_key),
                provided: provided.map(receiver_mode_key),
            },
            Mismatch::CallableConstness { required, provided } => Self::CallableConstness {
                required: required.as_str(),
                provided: provided.as_str(),
            },
            Mismatch::CallableExecution { required, provided } => Self::CallableExecution {
                required: required.as_str(),
                provided: provided.as_str(),
            },
            Mismatch::CallableTrust { required, provided } => Self::CallableTrust {
                required: required.as_str(),
                provided: provided.as_str(),
            },
            Mismatch::CallableAbi { required, provided } => Self::CallableAbi {
                required: required.as_str(),
                provided: provided.as_str(),
            },
            Mismatch::CallableParameterCount { required, provided } => {
                Self::CallableParameterCount {
                    required: *required,
                    provided: *provided,
                }
            }
            Mismatch::CallableParameterName {
                ordinal,
                required,
                provided,
            } => Self::CallableParameterName {
                ordinal: *ordinal,
                required: required.clone(),
                provided: provided.clone(),
            },
            Mismatch::CallableParameterPosition {
                ordinal,
                required,
                provided,
            } => Self::CallableParameterPosition {
                ordinal: *ordinal,
                required: required.as_str(),
                provided: provided.as_str(),
            },
            Mismatch::CallableParameterMode {
                ordinal,
                required,
                provided,
            } => Self::CallableParameterMode {
                ordinal: *ordinal,
                required: required.as_str(),
                provided: provided.as_str(),
            },
            Mismatch::CallableParameterType {
                ordinal,
                required,
                provided,
            } => Self::CallableParameterType {
                ordinal: *ordinal,
                required: DiagnosticTypeJson::from_type(required),
                provided: DiagnosticTypeJson::from_type(provided),
            },
            Mismatch::CallableResultType { required, provided } => Self::CallableResultType {
                required: DiagnosticTypeJson::from_type(required),
                provided: DiagnosticTypeJson::from_type(provided),
            },
            Mismatch::CallableParameterDefault {
                ordinal,
                required,
                provided,
            } => Self::CallableParameterDefault {
                ordinal: *ordinal,
                required: *required,
                provided: *provided,
            },
            Mismatch::CallableContract(mismatch) => Self::CallableContract {
                mismatch: DiagnosticCallableContractMismatchJson::from_mismatch(mismatch),
            },
            Mismatch::ConstantType { required, provided } => Self::ConstantType {
                required: DiagnosticTypeJson::from_type(required),
                provided: DiagnosticTypeJson::from_type(provided),
            },
            Mismatch::TypeValueUnavailable => Self::TypeValueUnavailable,
            Mismatch::PredicateTrust { required, provided } => Self::PredicateTrust {
                required: *required,
                provided: *provided,
            },
            Mismatch::PredicateParameterCount { required, provided } => {
                Self::PredicateParameterCount {
                    required: *required,
                    provided: *provided,
                }
            }
            Mismatch::PredicateParameterName {
                ordinal,
                required,
                provided,
            } => Self::PredicateParameterName {
                ordinal: *ordinal,
                required: required.clone(),
                provided: provided.clone(),
            },
            Mismatch::PredicateParameterType {
                ordinal,
                required,
                provided,
            } => Self::PredicateParameterType {
                ordinal: *ordinal,
                required: DiagnosticTypeJson::from_type(required),
                provided: DiagnosticTypeJson::from_type(provided),
            },
        }
    }
}
