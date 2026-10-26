use bray_symbols::DefaultValueProvider;
use bray_symbols::{DependencySubjectRoot, SymbolOrdinal, TypeId};

use super::{
    ConstructionTarget, SelectedArgument, SelectedConstructionInput, SelectedConversion,
    SelectedOperation, SelectedReceiver, SemanticSelection,
};
use crate::{BoundExecutionSite, BoundExpressionId, BoundReferenceTarget};

/// One evaluated invocation input, borrowing its existing checked selection.
#[derive(Clone, Copy, Debug)]
pub enum SelectedInput<'a> {
    /// An implicit instance receiver.
    Receiver(&'a SelectedReceiver),
    /// An ordinary callable parameter.
    Argument(&'a SelectedArgument),
    /// A construction field, payload, or type-form parameter.
    Construction(&'a SelectedConstructionInput),
}

impl<'a> SelectedInput<'a> {
    /// Returns the formal dependency root supplied by this input.
    pub fn root(self) -> DependencySubjectRoot {
        match self {
            Self::Receiver(_) => DependencySubjectRoot::Receiver,
            Self::Argument(
                SelectedArgument::Explicit { ordinal, .. }
                | SelectedArgument::Default { ordinal, .. },
            )
            | Self::Construction(
                SelectedConstructionInput::Explicit { ordinal, .. }
                | SelectedConstructionInput::Default { ordinal, .. },
            ) => DependencySubjectRoot::Parameter(SymbolOrdinal::new(*ordinal)),
        }
    }

    /// Returns the evaluated source, or none for a declaration-owned default.
    pub fn expression(self) -> Option<BoundExpressionId> {
        match self {
            Self::Receiver(receiver) => Some(receiver.expression()),
            Self::Argument(SelectedArgument::Explicit { expression, .. })
            | Self::Construction(SelectedConstructionInput::Explicit { expression, .. }) => {
                Some(*expression)
            }
            Self::Argument(SelectedArgument::Default { .. })
            | Self::Construction(SelectedConstructionInput::Default { .. }) => None,
        }
    }

    /// Returns the exact declaration-owned default selected for this input.
    pub fn default_provider(self) -> Option<DefaultValueProvider> {
        match self {
            Self::Argument(SelectedArgument::Default { provider, .. }) => {
                Some(DefaultValueProvider::CallableParameter(*provider))
            }
            Self::Construction(SelectedConstructionInput::Default { provider, .. }) => {
                Some(*provider)
            }
            _ => None,
        }
    }

    /// Returns the selected source adaptation, when the input has one.
    pub fn conversion(self) -> Option<&'a SelectedConversion> {
        match self {
            Self::Argument(SelectedArgument::Explicit { conversion, .. })
            | Self::Construction(SelectedConstructionInput::Explicit { conversion, .. }) => {
                Some(conversion)
            }
            _ => None,
        }
    }

    /// Returns the type passed to the selected operation.
    pub fn ty(self, values: &bray_symbols::SemanticValueStore) -> TypeId {
        match self {
            Self::Receiver(receiver) => receiver
                .input_type(values)
                .expect("checked receiver has its passed type"),
            Self::Argument(SelectedArgument::Explicit { conversion, .. }) => {
                conversion.target_type()
            }
            Self::Argument(SelectedArgument::Default { ty, .. }) => *ty,
            Self::Construction(input) => input.ty(),
        }
    }

    /// Returns the declaration input named by clauses or default evaluation.
    pub fn reference(self) -> Option<BoundReferenceTarget> {
        let symbol = match self {
            Self::Receiver(receiver) => receiver.parameter().into(),
            Self::Argument(SelectedArgument::Explicit { parameter, .. }) => (*parameter)?.into(),
            Self::Argument(SelectedArgument::Default { parameter, .. }) => (*parameter).into(),
            Self::Construction(
                SelectedConstructionInput::Explicit { input, .. }
                | SelectedConstructionInput::Default { input, .. },
            ) => input.symbol(),
        };

        Some(BoundReferenceTarget::Surface(symbol))
    }
}

impl SemanticSelection {
    /// Visits checked receiver and supplied inputs in evaluation order, followed by omitted defaults.
    pub fn inputs(&self) -> impl Iterator<Item = SelectedInput<'_>> {
        let (call, construction) = match self {
            Self::Call(call) => (Some(call), None),
            Self::Operation(SelectedOperation::Construction(construction)) => {
                (None, Some(construction))
            }
            _ => (None, None),
        };

        call.and_then(|call| call.receiver())
            .into_iter()
            .map(SelectedInput::Receiver)
            .chain(
                call.into_iter()
                    .flat_map(|call| call.arguments())
                    .map(SelectedInput::Argument),
            )
            .chain(
                construction
                    .into_iter()
                    .flat_map(|construction| construction.inputs())
                    .map(SelectedInput::Construction),
            )
    }

    /// Whether this invocation evaluates an omitted declaration-owned default.
    pub fn evaluates_defaults(&self) -> bool {
        self.inputs()
            .any(|input| input.default_provider().is_some())
    }

    /// Returns borrowed formal referents in receiver-first order for one actual invocation.
    pub fn invocation_reference_inputs(
        &self,
        site: BoundExecutionSite,
        values: &bray_symbols::SemanticValueStore,
    ) -> Vec<(SymbolOrdinal, DependencySubjectRoot, TypeId)> {
        if let Self::Iteration(iteration) = self {
            let ty = match site {
                BoundExecutionSite::Next(_) => iteration.cursor_type(),
                BoundExecutionSite::Iterate(_)
                    if iteration.mode() == crate::IterationSourceMode::Mutable =>
                {
                    iteration.source_type()
                }
                _ => return Vec::new(),
            };

            return vec![(
                SymbolOrdinal::new(0),
                DependencySubjectRoot::Receiver,
                values.unborrowed_type(ty),
            )];
        }

        if !matches!(site, BoundExecutionSite::Node(_)) {
            return Vec::new();
        }

        let mut inputs = self
            .inputs()
            .filter_map(|input| {
                let ty = values.type_data(input.ty(values));

                let bray_symbols::TypeData::Borrow { target, .. } = ty.as_ref() else {
                    return None;
                };

                Some((
                    self.input_ordinal(input.root())
                        .expect("selected input has its formal ordinal"),
                    input.root(),
                    *target,
                ))
            })
            .collect::<Vec<_>>();

        inputs.sort_unstable_by_key(|(ordinal, _, _)| *ordinal);

        inputs
    }

    /// Returns a formal input's receiver-first ordinal.
    pub fn input_ordinal(&self, root: DependencySubjectRoot) -> Option<SymbolOrdinal> {
        match self {
            Self::Call(call) => call.input_ordinal(root),
            Self::Operation(SelectedOperation::Construction(_)) => match root {
                DependencySubjectRoot::Parameter(ordinal) => Some(ordinal),
                _ => None,
            },
            _ => None,
        }
    }

    /// Returns the input at one receiver-first ordinal.
    pub fn input(&self, ordinal: SymbolOrdinal) -> Option<SelectedInput<'_>> {
        self.inputs()
            .find(|input| self.input_ordinal(input.root()) == Some(ordinal))
    }

    /// Returns the value-producing occurrence supplying a formal input.
    pub fn input_site(
        &self,
        expression: BoundExpressionId,
        root: DependencySubjectRoot,
    ) -> Option<BoundExecutionSite> {
        if let Self::Call(call) = self {
            return call.input_site(expression, root);
        }

        let ordinal = self.input_ordinal(root)?;
        let input = self.input(ordinal)?;

        if self.evaluates_defaults() {
            Some(BoundExecutionSite::CallInput {
                expression,
                ordinal,
            })
        } else {
            input.expression().map(Into::into)
        }
    }

    /// Resolves an operation's formal input to the value already evaluated for it.
    /// Callable defaults see only earlier parameters. Field defaults see no siblings.
    pub fn invocation_input_site(
        &self,
        occurrence: BoundExecutionSite,
        root: DependencySubjectRoot,
    ) -> Option<BoundExecutionSite> {
        match (self, occurrence, root) {
            (
                Self::Iteration(iteration),
                BoundExecutionSite::Iterate(_),
                DependencySubjectRoot::Receiver,
            ) => Some(iteration.source().into()),
            (
                Self::Iteration(_),
                BoundExecutionSite::Next(expression),
                DependencySubjectRoot::Receiver,
            ) => Some(BoundExecutionSite::Iterate(expression)),
            (
                Self::ScopedUse(scoped),
                BoundExecutionSite::ScopedEnter(_),
                DependencySubjectRoot::Receiver,
            ) => Some(scoped.initializer().into()),
            (
                Self::ScopedUse(_),
                BoundExecutionSite::ScopedExit(expression),
                DependencySubjectRoot::Parameter(ordinal),
            ) if ordinal.raw() == 0 => Some(BoundExecutionSite::ScopedEnter(expression)),
            _ => match occurrence {
                BoundExecutionSite::Node(crate::AnyBoundNodeId::Expression(expression)) => {
                    self.input_site(expression, root)
                }
                BoundExecutionSite::CallInput {
                    expression,
                    ordinal,
                } => {
                    let input = self
                        .input(ordinal)
                        .expect("input occurrence retains its selected input");

                    if matches!(
                        input.default_provider(),
                        Some(DefaultValueProvider::CallableParameter(_))
                    ) {
                        let previous = self.input_ordinal(root)?;

                        (previous < ordinal)
                            .then(|| self.input_site(expression, root))
                            .flatten()
                    } else if root == DependencySubjectRoot::Receiver {
                        input.expression().map(Into::into)
                    } else {
                        None
                    }
                }
                _ => None,
            },
        }
    }

    /// Returns the selected callable substitution for callable-owned defaults.
    pub fn input_instance(&self) -> Option<bray_symbols::CallableInstanceData> {
        match self {
            Self::Call(call) => match call.target() {
                crate::BoundCallableTarget::TraitView { member, .. } => Some(member),
                _ => call.resolution().declaration_source(),
            },
            Self::Operation(SelectedOperation::Construction(construction)) => {
                match construction.target() {
                    ConstructionTarget::TypeForm { callable, .. } => Some(callable),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
