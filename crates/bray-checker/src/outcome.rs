use bray_bound_tree::{BoundUnitId, BoundUnitKind, ControlCompletion};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

macro_rules! define_unit_conclusions {
    ($(($name:ident, $variant:ident)),+ $(,)?) => {
        $(
            #[doc = concat!("Completed semantic conclusions for one `", stringify!($variant), "` unit.")]
            #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
            pub struct $name {
                flow: WholeUnitFlowConclusions,
            }

            impl $name {
                pub(crate) const fn new(flow: WholeUnitFlowConclusions) -> Self {
                    Self { flow }
                }

                /// Returns the exact bound unit these conclusions describe.
                pub const fn unit(self) -> BoundUnitId {
                    self.flow.unit()
                }

                /// Returns the unit's checked control-completion categories.
                pub const fn completion(self) -> ControlCompletion {
                    self.flow.completion()
                }

                /// Returns whether conservative recovery affected whole-unit checking.
                pub const fn is_recovered(self) -> bool {
                    self.flow.is_recovered()
                }
            }
        )+

        /// Completed semantic conclusions for one exact checked-unit category.
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum UnitCheckConclusions {
            $(
                #[doc = concat!("Conclusions for a `", stringify!($variant), "` unit.")]
                $variant($name),
            )+
        }

        impl UnitCheckConclusions {
            pub(crate) const fn new(
                kind: BoundUnitKind,
                flow: WholeUnitFlowConclusions,
            ) -> Self {
                match kind {
                    $(BoundUnitKind::$variant => Self::$variant($name::new(flow)),)+
                }
            }

            /// Returns the exact bound unit these conclusions describe.
            pub const fn unit(self) -> BoundUnitId {
                match self {
                    $(Self::$variant(conclusions) => conclusions.unit(),)+
                }
            }

            /// Returns the unit's checked control-completion categories.
            pub const fn completion(self) -> ControlCompletion {
                match self {
                    $(Self::$variant(conclusions) => conclusions.completion(),)+
                }
            }

            /// Returns whether conservative recovery affected whole-unit checking.
            pub const fn is_recovered(self) -> bool {
                match self {
                    $(Self::$variant(conclusions) => conclusions.is_recovered(),)+
                }
            }
        }
    };
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct WholeUnitFlowConclusions {
    unit: BoundUnitId,
    completion: ControlCompletion,
    is_recovered: bool,
}

impl WholeUnitFlowConclusions {
    pub(crate) const fn new(
        unit: BoundUnitId,
        completion: ControlCompletion,
        is_recovered: bool,
    ) -> Self {
        Self {
            unit,
            completion,
            is_recovered,
        }
    }

    pub(crate) const fn unit(self) -> BoundUnitId {
        self.unit
    }

    pub(crate) const fn completion(self) -> ControlCompletion {
        self.completion
    }

    pub(crate) const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

define_unit_conclusions! {
    (CallableBodyCheckConclusions, CallableBody),
    (AnonymousCallableCheckConclusions, AnonymousCallable),
    (RuntimeDefaultCheckConclusions, RuntimeDefault),
    (ConstantTemplateCheckConclusions, ConstantTemplate),
    (PredicateDefinitionCheckConclusions, PredicateDefinition),
    (ConstraintCheckConclusions, Constraint),
    (ContractClauseCheckConclusions, ContractClause),
}

/// The result of one focused checker service operation.
///
/// Completed operations own their structured diagnostics alongside the typed
/// value. Cancellation carries neither diagnostics nor partial conclusions, so
/// orchestration cannot accidentally publish abandoned checker work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckerOutcome<T> {
    /// The operation completed with a typed value and its owned diagnostics.
    Complete(DiagnosticResult<T>),
    /// Cancellation was observed before the operation could complete.
    Cancelled,
}

impl<T> CheckerOutcome<T> {
    /// Creates a completed checker outcome.
    pub const fn complete(value: T, diagnostics: DiagnosticBag) -> Self {
        Self::Complete(DiagnosticResult::new(value, diagnostics))
    }

    /// Creates a completed checker outcome without diagnostics.
    pub fn without_diagnostics(value: T) -> Self {
        Self::Complete(DiagnosticResult::without_diagnostics(value))
    }

    /// Returns the completed result, or `None` after cancellation.
    pub const fn result(&self) -> Option<&DiagnosticResult<T>> {
        match self {
            Self::Complete(result) => Some(result),
            Self::Cancelled => None,
        }
    }

    /// Consumes the outcome into a completed result, or `None` after cancellation.
    pub fn into_result(self) -> Option<DiagnosticResult<T>> {
        match self {
            Self::Complete(result) => Some(result),
            Self::Cancelled => None,
        }
    }

    /// Returns whether cancellation prevented completion.
    pub const fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundUnitId, BoundUnitKind, ControlCompletion, ControlCompletionKind};
    use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

    use super::{
        CallableBodyCheckConclusions, CheckerOutcome, UnitCheckConclusions,
        WholeUnitFlowConclusions,
    };

    #[test]
    fn category_conclusions_keep_unit_completion_and_recovery_together() {
        let unit = BoundUnitId::new(4);
        let completion = ControlCompletion::from_kinds([ControlCompletionKind::Return]);
        let flow = WholeUnitFlowConclusions::new(unit, completion, true);
        let conclusions =
            UnitCheckConclusions::CallableBody(CallableBodyCheckConclusions::new(flow));

        assert_eq!(conclusions.unit(), unit);
        assert_eq!(conclusions.completion(), completion);
        assert!(conclusions.is_recovered());
    }

    #[test]
    fn each_bound_unit_kind_produces_its_exact_conclusion_category() {
        let flow =
            WholeUnitFlowConclusions::new(BoundUnitId::new(7), ControlCompletion::default(), false);

        let cases = [
            (
                BoundUnitKind::CallableBody,
                UnitCheckConclusions::CallableBody(CallableBodyCheckConclusions::new(flow)),
            ),
            (
                BoundUnitKind::AnonymousCallable,
                UnitCheckConclusions::AnonymousCallable(
                    super::AnonymousCallableCheckConclusions::new(flow),
                ),
            ),
            (
                BoundUnitKind::RuntimeDefault,
                UnitCheckConclusions::RuntimeDefault(super::RuntimeDefaultCheckConclusions::new(
                    flow,
                )),
            ),
            (
                BoundUnitKind::ConstantTemplate,
                UnitCheckConclusions::ConstantTemplate(
                    super::ConstantTemplateCheckConclusions::new(flow),
                ),
            ),
            (
                BoundUnitKind::PredicateDefinition,
                UnitCheckConclusions::PredicateDefinition(
                    super::PredicateDefinitionCheckConclusions::new(flow),
                ),
            ),
            (
                BoundUnitKind::Constraint,
                UnitCheckConclusions::Constraint(super::ConstraintCheckConclusions::new(flow)),
            ),
            (
                BoundUnitKind::ContractClause,
                UnitCheckConclusions::ContractClause(super::ContractClauseCheckConclusions::new(
                    flow,
                )),
            ),
        ];

        for (kind, expected) in cases {
            assert_eq!(UnitCheckConclusions::new(kind, flow), expected);
        }
    }

    #[test]
    fn completed_outcomes_keep_typed_values_with_owned_diagnostics() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(3),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        );

        let unit = BoundUnitId::new(4);
        let flow = WholeUnitFlowConclusions::new(unit, ControlCompletion::default(), false);
        let conclusions =
            UnitCheckConclusions::CallableBody(CallableBodyCheckConclusions::new(flow));

        let outcome =
            CheckerOutcome::complete(conclusions, DiagnosticBag::single(diagnostic.clone()));

        let Some(result) = outcome.result() else {
            panic!("completed checker outcomes retain their result");
        };

        assert_eq!(result.value(), &conclusions);
        assert_eq!(result.diagnostics().diagnostics(), &[diagnostic]);
    }

    #[test]
    fn cancelled_outcomes_expose_no_partial_result() {
        let outcome = CheckerOutcome::<UnitCheckConclusions>::Cancelled;

        assert!(outcome.is_cancelled());
        assert_eq!(outcome.into_result(), None);
    }

    #[test]
    fn outcomes_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckerOutcome<UnitCheckConclusions>>();
    }
}
