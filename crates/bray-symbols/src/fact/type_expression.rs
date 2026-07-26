use std::sync::Arc;

use bray_base::shared_slice;
use bray_declarations::SyntaxAnchor;

use crate::{
    AnySymbolId, BorrowKind, CallableAbi, CallableConstness, CallableDependencyContracts,
    CallableExecution, CallableParameterMode, CallableParameterName, CallablePhaseBehaviors,
    CallablePosition, CallableTrust, GenericArgument, GenericConstParameterSymbolId,
    GenericParameterSymbolId, NamedTypeSymbolId, TraitSymbolId, TraitTypeMemberSymbolId, TypeId,
};

/// Stable identity for one source constant expression embedded in a type expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConstantExpressionOccurrenceKey {
    owner: AnySymbolId,
    syntax: SyntaxAnchor,
}

impl ConstantExpressionOccurrenceKey {
    /// Creates an occurrence owned by one exact declaration surface.
    pub const fn new(owner: AnySymbolId, syntax: SyntaxAnchor) -> Self {
        Self { owner, syntax }
    }

    /// Returns the declaration surface whose lexical context contains the expression.
    pub const fn owner(self) -> AnySymbolId {
        self.owner
    }

    /// Returns the exact expression syntax anchor.
    pub const fn syntax(self) -> SyntaxAnchor {
        self.syntax
    }
}

/// The type expectation applied when an embedded constant expression is checked.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConstantExpressionExpectedType {
    /// A canonical type fixes the expectation directly.
    Resolved(TypeId),
    /// The declared type of the exact const parameter supplies the expectation.
    GenericParameter(GenericConstParameterSymbolId),
}

/// One source constant expression retained for later semantic checking.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConstantExpressionOccurrence {
    key: ConstantExpressionOccurrenceKey,
    expected_type: ConstantExpressionExpectedType,
}

impl ConstantExpressionOccurrence {
    /// Creates an embedded constant-expression occurrence.
    pub const fn new(
        key: ConstantExpressionOccurrenceKey,
        expected_type: ConstantExpressionExpectedType,
    ) -> Self {
        Self { key, expected_type }
    }

    /// Returns the stable source occurrence key.
    pub const fn key(self) -> ConstantExpressionOccurrenceKey {
        self.key
    }

    /// Returns the expected type source used during checking.
    pub const fn expected_type(self) -> ConstantExpressionExpectedType {
        self.expected_type
    }
}

/// One generic argument in an unresolved type-expression template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GenericArgumentTemplate {
    /// A type argument retains its recursively bound type template.
    Type(TypeExpressionTemplate),
    /// A const argument retains its source occurrence and expected type.
    Constant(ConstantExpressionOccurrence),
}

impl GenericArgumentTemplate {
    /// Returns the canonical generic argument if no checking remains.
    pub const fn resolved_argument(&self) -> Option<GenericArgument> {
        match self {
            Self::Type(ty) => match ty.resolved_type() {
                Some(ty) => Some(GenericArgument::Type(ty)),
                None => None,
            },
            Self::Constant(_) => None,
        }
    }
}

/// An applied trait whose constant-bearing arguments are not checked yet.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TraitApplicationTemplate {
    definition: TraitSymbolId,
    parameters: Arc<[GenericParameterSymbolId]>,
    arguments: Arc<[GenericArgumentTemplate]>,
}

impl TraitApplicationTemplate {
    /// Creates a trait application template in generic parameter order.
    pub fn new(
        definition: TraitSymbolId,
        parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
        arguments: impl IntoIterator<Item = GenericArgumentTemplate>,
    ) -> Self {
        Self {
            definition,
            parameters: shared_slice(parameters),
            arguments: shared_slice(arguments),
        }
    }

    /// Returns the applied trait definition.
    pub const fn definition(&self) -> TraitSymbolId {
        self.definition
    }

    /// Returns generic parameters in declaration order.
    pub fn parameters(&self) -> &[GenericParameterSymbolId] {
        &self.parameters
    }

    /// Returns generic arguments in declaration order.
    pub fn arguments(&self) -> &[GenericArgumentTemplate] {
        &self.arguments
    }

    /// Returns embedded constant expressions in deterministic argument order.
    pub fn constant_expressions(&self) -> Vec<ConstantExpressionOccurrence> {
        let mut occurrences = Vec::new();

        push_argument_constants(&self.arguments, &mut occurrences);

        occurrences
    }
}

/// One callable parameter whose type can contain embedded constant expressions.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableParameterTypeTemplate {
    name: CallableParameterName,
    position: CallablePosition,
    mode: CallableParameterMode,
    ty: TypeExpressionTemplate,
}

impl CallableParameterTypeTemplate {
    /// Creates one callable parameter type template.
    pub const fn new(
        name: CallableParameterName,
        position: CallablePosition,
        mode: CallableParameterMode,
        ty: TypeExpressionTemplate,
    ) -> Self {
        Self {
            name,
            position,
            mode,
            ty,
        }
    }

    /// Returns the caller-visible parameter name.
    pub const fn name(&self) -> &CallableParameterName {
        &self.name
    }

    /// Returns the caller-visible argument position policy.
    pub const fn position(&self) -> CallablePosition {
        self.position
    }

    /// Returns the parameter mutation mode.
    pub const fn mode(&self) -> CallableParameterMode {
        self.mode
    }

    /// Returns the parameter type template.
    pub const fn ty(&self) -> &TypeExpressionTemplate {
        &self.ty
    }

    /// Separates the parameter surface from its type template.
    pub fn into_parts(
        self,
    ) -> (
        CallableParameterName,
        CallablePosition,
        CallableParameterMode,
        TypeExpressionTemplate,
    ) {
        (self.name, self.position, self.mode, self.ty)
    }
}

/// A callable type whose component types can contain embedded constant expressions.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableTypeTemplate {
    parameters: Arc<[CallableParameterTypeTemplate]>,
    result: Arc<TypeExpressionTemplate>,
    constness: CallableConstness,
    trust: CallableTrust,
    abi: CallableAbi,
    phase_behaviors: CallablePhaseBehaviors,
}

impl CallableTypeTemplate {
    /// Creates one callable type template.
    pub fn new(
        parameters: impl IntoIterator<Item = CallableParameterTypeTemplate>,
        result: TypeExpressionTemplate,
        constness: CallableConstness,
        trust: CallableTrust,
        abi: CallableAbi,
        dependencies: CallableDependencyContracts,
    ) -> Self {
        Self {
            parameters: shared_slice(parameters),
            result: Arc::new(result),
            constness,
            trust,
            abi,
            phase_behaviors: CallablePhaseBehaviors::empty(dependencies),
        }
    }

    /// Returns this callable template with complete caller-visible phase behavior.
    pub fn with_phase_behaviors(mut self, phase_behaviors: CallablePhaseBehaviors) -> Self {
        self.phase_behaviors = phase_behaviors;

        self
    }

    /// Returns parameters in declaration order.
    pub fn parameters(&self) -> &[CallableParameterTypeTemplate] {
        &self.parameters
    }

    /// Returns the callable result type template.
    pub fn result(&self) -> &TypeExpressionTemplate {
        &self.result
    }

    /// Returns whether calls are allowed in constant contexts.
    pub const fn constness(&self) -> CallableConstness {
        self.constness
    }

    /// Returns whether calls execute synchronously or asynchronously.
    pub const fn execution(&self) -> CallableExecution {
        self.phase_behaviors.execution()
    }

    /// Returns the callable trust boundary.
    pub const fn trust(&self) -> CallableTrust {
        self.trust
    }

    /// Returns the caller-visible ABI.
    pub const fn abi(&self) -> CallableAbi {
        self.abi
    }

    /// Returns the invocation and deferred-execution dependency contracts.
    pub fn dependencies(&self) -> CallableDependencyContracts {
        self.phase_behaviors.dependency_contracts()
    }

    /// Returns behavior for every callable execution phase.
    pub const fn phase_behaviors(&self) -> &CallablePhaseBehaviors {
        &self.phase_behaviors
    }
}

/// A target-independent bound type expression awaiting embedded constant checking.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TypeExpressionTemplate {
    /// A constant-free type is already a canonical semantic type.
    Resolved(TypeId),
    /// A named type retains generic arguments in parameter order.
    Named {
        /// The exact named type definition.
        definition: NamedTypeSymbolId,
        /// Generic parameters in declaration order.
        parameters: Arc<[GenericParameterSymbolId]>,
        /// Generic arguments in parameter order.
        arguments: Arc<[GenericArgumentTemplate]>,
    },
    /// A type-valued member projection retains its applied trait and exact member.
    TypeValuedMemberProjection {
        subject: Arc<TypeExpressionTemplate>,
        application: TraitApplicationTemplate,
        member: TraitTypeMemberSymbolId,
    },
    /// A tuple retains element templates in source order.
    Tuple(Arc<[TypeExpressionTemplate]>),
    /// A fixed array retains its element template and length expression occurrence.
    Array {
        element: Arc<TypeExpressionTemplate>,
        length: ConstantExpressionOccurrence,
    },
    /// A slice retains its element type template.
    Slice(Arc<TypeExpressionTemplate>),
    /// A nullable type retains its target type template.
    Nullable(Arc<TypeExpressionTemplate>),
    /// A borrow retains its kind and target type template.
    Borrow {
        kind: BorrowKind,
        target: Arc<TypeExpressionTemplate>,
    },
    /// A trait view retains the applied trait template.
    TraitView(TraitApplicationTemplate),
    /// An owned indirection retains storage and target type templates.
    OwnedIndirection {
        storage: Arc<TypeExpressionTemplate>,
        target: Arc<TypeExpressionTemplate>,
    },
    /// A callable retains its complete caller-visible type template.
    Callable(CallableTypeTemplate),
}

impl TypeExpressionTemplate {
    /// Returns the canonical type if this template already stores one.
    pub const fn resolved_type(&self) -> Option<TypeId> {
        match self {
            Self::Resolved(ty) => Some(*ty),
            Self::Named { .. }
            | Self::TypeValuedMemberProjection { .. }
            | Self::Tuple(_)
            | Self::Array { .. }
            | Self::Slice(_)
            | Self::Nullable(_)
            | Self::Borrow { .. }
            | Self::TraitView(_)
            | Self::OwnedIndirection { .. }
            | Self::Callable(_) => None,
        }
    }

    /// Returns embedded constant expressions in deterministic template order.
    pub fn constant_expressions(&self) -> Vec<ConstantExpressionOccurrence> {
        let mut occurrences = Vec::new();

        self.push_constant_expressions(&mut occurrences);

        occurrences
    }

    fn push_constant_expressions(&self, occurrences: &mut Vec<ConstantExpressionOccurrence>) {
        match self {
            Self::Resolved(_) => {}
            Self::Named { arguments, .. } => {
                push_argument_constants(arguments, occurrences);
            }
            Self::TypeValuedMemberProjection {
                subject,
                application,
                ..
            } => {
                subject.push_constant_expressions(occurrences);
                push_argument_constants(application.arguments(), occurrences);
            }
            Self::Tuple(elements) => {
                for element in elements.iter() {
                    element.push_constant_expressions(occurrences);
                }
            }
            Self::Array { element, length } => {
                element.push_constant_expressions(occurrences);
                occurrences.push(*length);
            }
            Self::Slice(element) | Self::Nullable(element) => {
                element.push_constant_expressions(occurrences);
            }
            Self::Borrow { target, .. } => {
                target.push_constant_expressions(occurrences);
            }
            Self::TraitView(application) => {
                push_argument_constants(application.arguments(), occurrences);
            }
            Self::OwnedIndirection { storage, target } => {
                storage.push_constant_expressions(occurrences);
                target.push_constant_expressions(occurrences);
            }
            Self::Callable(callable) => {
                for parameter in callable.parameters() {
                    parameter.ty().push_constant_expressions(occurrences);
                }

                callable.result().push_constant_expressions(occurrences);
            }
        }
    }
}

fn push_argument_constants(
    arguments: &[GenericArgumentTemplate],
    occurrences: &mut Vec<ConstantExpressionOccurrence>,
) {
    for argument in arguments {
        match argument {
            GenericArgumentTemplate::Type(ty) => ty.push_constant_expressions(occurrences),
            GenericArgumentTemplate::Constant(occurrence) => occurrences.push(*occurrence),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConstantExpressionOccurrence, TypeExpressionTemplate};

    #[test]
    fn template_values_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ConstantExpressionOccurrence>();
        assert_send_sync::<TypeExpressionTemplate>();
    }
}
