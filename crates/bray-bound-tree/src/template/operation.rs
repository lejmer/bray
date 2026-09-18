use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{
    BorrowKind, ConstantBinaryOperation, ConstantField, ConstantTermId, ConstantUnaryOperation,
    GenericSubstitutionId, SymbolKey, TypeId,
};

use super::{
    CheckedTemplateConstantUsage, CheckedTemplateInputId, CheckedTemplateNodeId,
    CheckedTemplateShortCircuitKind, CheckedTemplateTemporaryId,
};

/// The exact witness source selected for a custom indexing operation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedTemplateIndexDispatch<
    Declaration = SymbolKey,
    Substitution = GenericSubstitutionId,
    Implementation = SymbolKey,
    Type = TypeId,
> {
    /// A concrete implementation and its checked generic application.
    Implementation(Implementation, Substitution),
    /// A surrounding declaration constraint in declaration order.
    Constraint {
        /// The generic declaration supplying the witness.
        owner: Declaration,
        /// The selected constraint's ordinal.
        ordinal: bray_symbols::SymbolOrdinal,
    },
    /// The witness realizing the selected trait member's default body.
    TraitDefault {
        /// The reached subject requiring the default witness.
        subject: Type,
        /// The trait-owned substitution of the exact requirement.
        substitution: Substitution,
    },
}

/// The checked callable and access capability selected for custom indexing.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedTemplateIndexCall<
    Declaration = SymbolKey,
    Substitution = GenericSubstitutionId,
    Implementation = SymbolKey,
    Type = TypeId,
> {
    /// The selected concrete fulfillment or constrained trait member.
    pub callable: Declaration,
    /// The exact checked generic application.
    pub substitution: Substitution,
    /// The implicit receiver borrow and result-place capability.
    pub borrow_kind: BorrowKind,
    /// The exact checked witness source.
    pub dispatch: CheckedTemplateIndexDispatch<Declaration, Substitution, Implementation, Type>,
}

impl<D, S, I, T> CheckedTemplateIndexCall<D, S, I, T> {
    fn try_map_references<O, P, Q, Y, E>(
        &self,
        ty: &mut impl FnMut(&T) -> Result<Y, E>,
        declaration: &mut impl FnMut(&D) -> Result<O, E>,
        substitution: &mut impl FnMut(&S) -> Result<P, E>,
        implementation: &mut impl FnMut(&I) -> Result<Q, E>,
    ) -> Result<CheckedTemplateIndexCall<O, P, Q, Y>, E> {
        Ok(CheckedTemplateIndexCall {
            callable: declaration(&self.callable)?,
            substitution: substitution(&self.substitution)?,
            borrow_kind: self.borrow_kind,
            dispatch: match &self.dispatch {
                CheckedTemplateIndexDispatch::Implementation(witness, application) => {
                    CheckedTemplateIndexDispatch::Implementation(
                        implementation(witness)?,
                        substitution(application)?,
                    )
                }
                CheckedTemplateIndexDispatch::Constraint { owner, ordinal } => {
                    CheckedTemplateIndexDispatch::Constraint {
                        owner: declaration(owner)?,
                        ordinal: *ordinal,
                    }
                }
                CheckedTemplateIndexDispatch::TraitDefault {
                    subject,
                    substitution: application,
                } => CheckedTemplateIndexDispatch::TraitDefault {
                    subject: ty(subject)?,
                    substitution: substitution(application)?,
                },
            },
        })
    }
}

/// The closed normalized operation vocabulary of a checked template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedTemplateOperation<
    Term = ConstantTermId,
    Type = TypeId,
    Declaration = SymbolKey,
    Substitution = GenericSubstitutionId,
    Implementation = SymbolKey,
> {
    /// Reads one explicitly declared contextual or generic input.
    Input(CheckedTemplateInputId),
    /// Materializes an already checked open or closed constant term.
    Constant {
        /// The checked open or closed constant term.
        term: Term,
        /// Materialization work no longer recoverable from a closed value.
        usage: CheckedTemplateConstantUsage,
    },
    /// Applies a selected unary constant operation.
    Unary {
        /// Exact checked operation.
        operation: ConstantUnaryOperation,
        /// Operand evaluated before the operation.
        operand: CheckedTemplateNodeId,
    },
    /// Applies a selected binary constant operation.
    Binary {
        /// Exact checked operation.
        operation: ConstantBinaryOperation,
        /// Left operand evaluated first.
        left: CheckedTemplateNodeId,
        /// Right operand evaluated second unless the operation short-circuits.
        right: CheckedTemplateNodeId,
    },
    /// Borrows one evaluated place with its checked capability.
    Borrow {
        /// Exact borrow capability.
        kind: BorrowKind,
        /// Place evaluated before creating the borrow.
        operand: CheckedTemplateNodeId,
    },
    /// Reads a declaration-owned value through stable semantic identity.
    Declaration {
        /// Selected declaration.
        declaration: Declaration,
        /// Exact closed generic application when the declaration is selected explicitly.
        substitution: Option<Substitution>,
    },
    /// Applies one selected callable or predicate with deterministic argument order.
    Call {
        /// The selected callable or predicate declaration.
        callable: Declaration,
        /// Ordered generic arguments applied to the callable declaration.
        substitution: Substitution,
        /// Arguments in exact evaluation and parameter order.
        arguments: Arc<[CheckedTemplateNodeId]>,
        /// The selected implementation witness when dispatch requires one.
        implementation: Option<(Implementation, Substitution)>,
    },
    /// Applies an already checked semantic conversion.
    Convert {
        /// The converted value.
        value: CheckedTemplateNodeId,
        /// The checked destination type.
        target: Type,
    },
    /// Constructs a tuple from values in element order.
    Tuple(Arc<[CheckedTemplateNodeId]>),
    /// Constructs an array from values in element order.
    Array(Arc<[CheckedTemplateNodeId]>),
    /// Constructs a product while evaluating fields in source order.
    Product(Arc<[ConstantField<Declaration, CheckedTemplateNodeId>]>),
    /// Projects a selected declaration-owned member from a value.
    Project {
        /// The projected subject.
        subject: CheckedTemplateNodeId,
        /// The selected field, payload, or associated declaration.
        member: Declaration,
    },
    /// Projects one element using the checked indexing operation.
    Index {
        /// Selected custom access. Absence denotes the built-in operation.
        call:
            Option<Arc<CheckedTemplateIndexCall<Declaration, Substitution, Implementation, Type>>>,
        /// The indexed subject, evaluated first.
        subject: CheckedTemplateNodeId,
        /// The checked element index.
        index: CheckedTemplateNodeId,
    },
    /// Projects a half-open range using the checked indexing operation.
    Slice {
        /// Selected custom access. Absence denotes the built-in operation.
        call:
            Option<Arc<CheckedTemplateIndexCall<Declaration, Substitution, Implementation, Type>>>,
        /// The sliced subject, evaluated first.
        subject: CheckedTemplateNodeId,
        /// Inclusive lower bound. Absence means the start of the subject.
        lower: Option<CheckedTemplateNodeId>,
        /// Exclusive upper bound. Absence means the end of the subject.
        upper: Option<CheckedTemplateNodeId>,
    },
    /// Evaluates a condition once and then exactly one selected branch.
    Conditional {
        /// The condition evaluated before either branch.
        condition: CheckedTemplateNodeId,
        /// The result evaluated only when the condition is true.
        when_true: CheckedTemplateNodeId,
        /// The result evaluated only when the condition is false.
        when_false: CheckedTemplateNodeId,
    },
    /// Evaluates the left operand and evaluates the right operand only when required.
    ShortCircuit {
        /// The exact conjunction or disjunction evaluation rule.
        kind: CheckedTemplateShortCircuitKind,
        /// The operand evaluated first.
        left: CheckedTemplateNodeId,
        /// The operand evaluated conditionally.
        right: CheckedTemplateNodeId,
    },
    /// Reads one explicitly materialized template-local temporary.
    Temporary(CheckedTemplateTemporaryId),
}

impl<Term, Type, Declaration, Substitution, Implementation>
    CheckedTemplateOperation<Term, Type, Declaration, Substitution, Implementation>
{
    /// Creates a selected callable or predicate application with stable argument order.
    pub fn call(
        callable: Declaration,
        substitution: Substitution,
        arguments: impl IntoIterator<Item = CheckedTemplateNodeId>,
        implementation: Option<(Implementation, Substitution)>,
    ) -> Self {
        Self::Call {
            callable,
            substitution,
            arguments: shared_slice(arguments),
            implementation,
        }
    }

    /// Creates an ordered tuple construction.
    pub fn tuple(elements: impl IntoIterator<Item = CheckedTemplateNodeId>) -> Self {
        Self::Tuple(shared_slice(elements))
    }

    /// Creates an ordered array construction.
    pub fn array(elements: impl IntoIterator<Item = CheckedTemplateNodeId>) -> Self {
        Self::Array(shared_slice(elements))
    }

    /// Creates a field-identified product construction in evaluation order.
    pub fn product(
        fields: impl IntoIterator<Item = ConstantField<Declaration, CheckedTemplateNodeId>>,
    ) -> Self {
        Self::Product(shared_slice(fields))
    }

    /// Maps semantic references while preserving node identities and sharing operand storage.
    pub fn try_map_references<T, Y, D, S, I, E>(
        &self,
        mut term: impl FnMut(&Term) -> Result<T, E>,
        mut ty: impl FnMut(&Type) -> Result<Y, E>,
        mut declaration: impl FnMut(&Declaration) -> Result<D, E>,
        mut substitution: impl FnMut(&Substitution) -> Result<S, E>,
        mut implementation: impl FnMut(&Implementation) -> Result<I, E>,
    ) -> Result<CheckedTemplateOperation<T, Y, D, S, I>, E> {
        Ok(match self {
            Self::Input(input) => CheckedTemplateOperation::Input(*input),
            Self::Constant { term: value, usage } => CheckedTemplateOperation::Constant {
                term: term(value)?,
                usage: *usage,
            },
            Self::Unary { operation, operand } => CheckedTemplateOperation::Unary {
                operation: *operation,
                operand: *operand,
            },
            Self::Binary {
                operation,
                left,
                right,
            } => CheckedTemplateOperation::Binary {
                operation: *operation,
                left: *left,
                right: *right,
            },
            Self::Borrow { kind, operand } => CheckedTemplateOperation::Borrow {
                kind: *kind,
                operand: *operand,
            },
            Self::Declaration {
                declaration: value,
                substitution: application,
            } => CheckedTemplateOperation::Declaration {
                declaration: declaration(value)?,
                substitution: application.as_ref().map(&mut substitution).transpose()?,
            },
            Self::Call {
                callable,
                substitution: application,
                arguments,
                implementation: witness,
            } => CheckedTemplateOperation::Call {
                callable: declaration(callable)?,
                substitution: substitution(application)?,
                arguments: Arc::clone(arguments),
                implementation: witness
                    .as_ref()
                    .map(|(witness, application)| {
                        Ok((implementation(witness)?, substitution(application)?))
                    })
                    .transpose()?,
            },
            Self::Convert { value, target } => CheckedTemplateOperation::Convert {
                value: *value,
                target: ty(target)?,
            },
            Self::Tuple(elements) => CheckedTemplateOperation::Tuple(Arc::clone(elements)),
            Self::Array(elements) => CheckedTemplateOperation::Array(Arc::clone(elements)),
            Self::Product(fields) => CheckedTemplateOperation::product(
                fields
                    .iter()
                    .map(|field| {
                        Ok(ConstantField::new(
                            declaration(field.field())?,
                            *field.value(),
                        ))
                    })
                    .collect::<Result<Vec<_>, E>>()?,
            ),
            Self::Project { subject, member } => CheckedTemplateOperation::Project {
                subject: *subject,
                member: declaration(member)?,
            },
            Self::Index {
                subject,
                index,
                call,
            } => CheckedTemplateOperation::Index {
                call: call
                    .as_ref()
                    .map(|call| {
                        call.try_map_references(
                            &mut ty,
                            &mut declaration,
                            &mut substitution,
                            &mut implementation,
                        )
                        .map(Arc::new)
                    })
                    .transpose()?,
                subject: *subject,
                index: *index,
            },
            Self::Slice {
                subject,
                lower,
                upper,
                call,
            } => CheckedTemplateOperation::Slice {
                call: call
                    .as_ref()
                    .map(|call| {
                        call.try_map_references(
                            &mut ty,
                            &mut declaration,
                            &mut substitution,
                            &mut implementation,
                        )
                        .map(Arc::new)
                    })
                    .transpose()?,
                subject: *subject,
                lower: *lower,
                upper: *upper,
            },
            Self::Conditional {
                condition,
                when_true,
                when_false,
            } => CheckedTemplateOperation::Conditional {
                condition: *condition,
                when_true: *when_true,
                when_false: *when_false,
            },
            Self::ShortCircuit { kind, left, right } => CheckedTemplateOperation::ShortCircuit {
                kind: *kind,
                left: *left,
                right: *right,
            },
            Self::Temporary(temporary) => CheckedTemplateOperation::Temporary(*temporary),
        })
    }

    /// Visits direct node dependencies in evaluation order, including conditional branches.
    pub fn try_for_each_node_reference<E>(
        &self,
        mut visit: impl FnMut(CheckedTemplateNodeId) -> Result<(), E>,
    ) -> Result<(), E> {
        match self {
            Self::Call { arguments, .. } | Self::Tuple(arguments) | Self::Array(arguments) => {
                for argument in arguments.iter() {
                    visit(*argument)?;
                }
            }
            Self::Product(fields) => {
                for field in fields.iter() {
                    visit(*field.value())?;
                }
            }
            Self::Convert { value, .. } => visit(*value)?,
            Self::Unary { operand, .. } | Self::Borrow { operand, .. } => visit(*operand)?,
            Self::Binary { left, right, .. } => {
                visit(*left)?;
                visit(*right)?;
            }
            Self::Project { subject, .. } => visit(*subject)?,
            Self::Index { subject, index, .. } => {
                visit(*subject)?;
                visit(*index)?;
            }
            Self::Slice {
                subject,
                lower,
                upper,
                ..
            } => {
                visit(*subject)?;

                for bound in [lower, upper].into_iter().flatten() {
                    visit(*bound)?;
                }
            }
            Self::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                visit(*condition)?;
                visit(*when_true)?;
                visit(*when_false)?;
            }
            Self::ShortCircuit { left, right, .. } => {
                visit(*left)?;
                visit(*right)?;
            }
            Self::Input(_)
            | Self::Constant { .. }
            | Self::Declaration { .. }
            | Self::Temporary(_) => {}
        }

        Ok(())
    }
}
