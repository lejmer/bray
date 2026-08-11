use serde::Serialize;

use super::super::{DiagnosticInterfaceSymbolIdentityJson, DiagnosticTypeJson};

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticImplementationOverloadProblemJson {
    HeaderSubjectKind {
        subject: DiagnosticInterfaceSymbolIdentityJson,
        actual: &'static str,
    },
    HeaderTraitKind {
        trait_definition: DiagnosticInterfaceSymbolIdentityJson,
        actual: &'static str,
    },
    ArmSymbolKind {
        implementation: DiagnosticInterfaceSymbolIdentityJson,
        actual: &'static str,
    },
    DuplicateArm {
        implementation: DiagnosticInterfaceSymbolIdentityJson,
    },
    ArmSubjectNotFamilyCompatible {
        implementation: DiagnosticInterfaceSymbolIdentityJson,
    },
    FamilyMismatch {
        implementation: DiagnosticInterfaceSymbolIdentityJson,
        required: DiagnosticImplementationFamilyJson,
        provided: DiagnosticImplementationFamilyJson,
    },
}

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticCallableOverloadProblemJson {
    ArmSymbolKind {
        symbol: DiagnosticInterfaceSymbolIdentityJson,
        actual: &'static str,
    },
    ContextMismatch {
        arm: DiagnosticCallableOverloadArmJson,
        required: DiagnosticCallableOverloadContextJson,
        provided: DiagnosticCallableOverloadContextJson,
    },
    DuplicateArm {
        arm: DiagnosticCallableOverloadArmJson,
    },
    ConflictingFamilies {
        arm: DiagnosticCallableOverloadArmJson,
        first: DiagnosticInterfaceSymbolIdentityJson,
        second: DiagnosticInterfaceSymbolIdentityJson,
    },
    ConflictingSignatures {
        arm: DiagnosticCallableOverloadArmJson,
        conflicting: DiagnosticCallableOverloadArmJson,
    },
}

impl DiagnosticCallableOverloadProblemJson {
    pub(in crate::output::diagnostic::json) fn from_problem(
        problem: &bray_diagnostics::DiagnosticCallableOverloadProblem,
    ) -> Self {
        use bray_diagnostics::DiagnosticCallableOverloadProblem as Problem;

        match problem {
            Problem::ArmSymbolKind { symbol, actual } => Self::ArmSymbolKind {
                symbol: DiagnosticInterfaceSymbolIdentityJson::from_identity(symbol),
                actual: actual.as_str(),
            },
            Problem::ContextMismatch {
                arm,
                required,
                provided,
            } => Self::ContextMismatch {
                arm: DiagnosticCallableOverloadArmJson::from_arm(arm),
                required: DiagnosticCallableOverloadContextJson::from_context(required),
                provided: DiagnosticCallableOverloadContextJson::from_context(provided),
            },
            Problem::DuplicateArm { arm } => Self::DuplicateArm {
                arm: DiagnosticCallableOverloadArmJson::from_arm(arm),
            },
            Problem::ConflictingFamilies { arm, first, second } => Self::ConflictingFamilies {
                arm: DiagnosticCallableOverloadArmJson::from_arm(arm),
                first: DiagnosticInterfaceSymbolIdentityJson::from_identity(first),
                second: DiagnosticInterfaceSymbolIdentityJson::from_identity(second),
            },
            Problem::ConflictingSignatures { arm, conflicting } => Self::ConflictingSignatures {
                arm: DiagnosticCallableOverloadArmJson::from_arm(arm),
                conflicting: DiagnosticCallableOverloadArmJson::from_arm(conflicting),
            },
        }
    }
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticCallableOverloadArmJson {
    identity: DiagnosticInterfaceSymbolIdentityJson,
    has_receiver: bool,
    parameter_types: Vec<DiagnosticTypeJson>,
    result_type: DiagnosticTypeJson,
}

impl DiagnosticCallableOverloadArmJson {
    fn from_arm(arm: &bray_diagnostics::DiagnosticCallableOverloadArm) -> Self {
        Self {
            identity: DiagnosticInterfaceSymbolIdentityJson::from_identity(arm.identity()),
            has_receiver: arm.has_receiver(),
            parameter_types: arm
                .parameter_types()
                .iter()
                .map(DiagnosticTypeJson::from_type)
                .collect(),
            result_type: DiagnosticTypeJson::from_type(arm.result_type()),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "identity", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticCallableOverloadContextJson {
    Module(DiagnosticInterfaceSymbolIdentityJson),
    NamedType(DiagnosticInterfaceSymbolIdentityJson),
    Trait(DiagnosticInterfaceSymbolIdentityJson),
    Implementation(DiagnosticInterfaceSymbolIdentityJson),
}

impl DiagnosticCallableOverloadContextJson {
    fn from_context(context: &bray_diagnostics::DiagnosticCallableOverloadContext) -> Self {
        use bray_diagnostics::DiagnosticCallableOverloadContext as Context;

        match context {
            Context::Module(identity) => Self::Module(
                DiagnosticInterfaceSymbolIdentityJson::from_identity(identity),
            ),
            Context::NamedType(identity) => Self::NamedType(
                DiagnosticInterfaceSymbolIdentityJson::from_identity(identity),
            ),
            Context::Trait(identity) => Self::Trait(
                DiagnosticInterfaceSymbolIdentityJson::from_identity(identity),
            ),
            Context::Implementation(identity) => Self::Implementation(
                DiagnosticInterfaceSymbolIdentityJson::from_identity(identity),
            ),
        }
    }
}

impl DiagnosticImplementationOverloadProblemJson {
    pub(in crate::output::diagnostic::json) fn from_problem(
        problem: &bray_diagnostics::DiagnosticImplementationOverloadProblem,
    ) -> Self {
        use bray_diagnostics::DiagnosticImplementationOverloadProblem as Problem;

        match problem {
            Problem::HeaderSubjectKind { subject, actual } => Self::HeaderSubjectKind {
                subject: DiagnosticInterfaceSymbolIdentityJson::from_identity(subject),
                actual: actual.as_str(),
            },
            Problem::HeaderTraitKind {
                trait_definition,
                actual,
            } => Self::HeaderTraitKind {
                trait_definition: DiagnosticInterfaceSymbolIdentityJson::from_identity(
                    trait_definition,
                ),
                actual: actual.as_str(),
            },
            Problem::ArmSymbolKind {
                implementation,
                actual,
            } => Self::ArmSymbolKind {
                implementation: DiagnosticInterfaceSymbolIdentityJson::from_identity(
                    implementation,
                ),
                actual: actual.as_str(),
            },
            Problem::DuplicateArm { implementation } => Self::DuplicateArm {
                implementation: DiagnosticInterfaceSymbolIdentityJson::from_identity(
                    implementation,
                ),
            },
            Problem::ArmSubjectNotFamilyCompatible { implementation } => {
                Self::ArmSubjectNotFamilyCompatible {
                    implementation: DiagnosticInterfaceSymbolIdentityJson::from_identity(
                        implementation,
                    ),
                }
            }
            Problem::FamilyMismatch {
                implementation,
                required,
                provided,
            } => Self::FamilyMismatch {
                implementation: DiagnosticInterfaceSymbolIdentityJson::from_identity(
                    implementation,
                ),
                required: DiagnosticImplementationFamilyJson::from_family(required),
                provided: DiagnosticImplementationFamilyJson::from_family(provided),
            },
        }
    }
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticImplementationFamilyJson {
    subject: DiagnosticImplementationFamilySubjectJson,
    trait_definition: DiagnosticInterfaceSymbolIdentityJson,
}

impl DiagnosticImplementationFamilyJson {
    fn from_family(family: &bray_diagnostics::DiagnosticImplementationFamily) -> Self {
        Self {
            subject: DiagnosticImplementationFamilySubjectJson::from_subject(family.subject()),
            trait_definition: DiagnosticInterfaceSymbolIdentityJson::from_identity(
                family.trait_definition(),
            ),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DiagnosticImplementationFamilySubjectJson {
    Named {
        identity: DiagnosticInterfaceSymbolIdentityJson,
    },
    Borrowed {
        borrow: &'static str,
        identity: DiagnosticInterfaceSymbolIdentityJson,
    },
}

impl DiagnosticImplementationFamilySubjectJson {
    fn from_subject(subject: &bray_diagnostics::DiagnosticImplementationFamilySubject) -> Self {
        use bray_diagnostics::DiagnosticImplementationFamilySubject as Subject;

        match subject {
            Subject::Named(identity) => Self::Named {
                identity: DiagnosticInterfaceSymbolIdentityJson::from_identity(identity),
            },
            Subject::Borrowed { kind, subject } => Self::Borrowed {
                borrow: match kind {
                    bray_diagnostics::DiagnosticImplementationBorrowKind::Shared => "shared",
                    bray_diagnostics::DiagnosticImplementationBorrowKind::Mutable => "mutable",
                },
                identity: DiagnosticInterfaceSymbolIdentityJson::from_identity(subject),
            },
        }
    }
}
