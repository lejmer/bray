use std::sync::Arc;

use bray_base::shared_slice;
use bray_declarations::SyntaxAnchor;

use crate::{
    AnySymbolId, CallableParameterName, ImplementationSubjectTemplate, ImplementationSymbolId,
    PredicateDefinitionSymbolId, PredicateParameterSymbolId, TraitApplicationTemplate,
    TypeExpressionTemplate,
};

use super::GenericDeclarationTemplate;

/// One predicate parameter and its declared type template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PredicateParameterTemplate {
    parameter: PredicateParameterSymbolId,
    name: CallableParameterName,
    ty: TypeExpressionTemplate,
}

impl PredicateParameterTemplate {
    /// Creates one predicate parameter template.
    pub const fn new(
        parameter: PredicateParameterSymbolId,
        name: CallableParameterName,
        ty: TypeExpressionTemplate,
    ) -> Self {
        Self {
            parameter,
            name,
            ty,
        }
    }

    /// Returns the exact parameter symbol.
    pub const fn parameter(&self) -> PredicateParameterSymbolId {
        self.parameter
    }

    /// Returns the parameter name used for named argument matching.
    pub const fn name(&self) -> &CallableParameterName {
        &self.name
    }

    /// Returns the declared parameter type template.
    pub const fn ty(&self) -> &TypeExpressionTemplate {
        &self.ty
    }
}

/// The declaration signature template of one predicate.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PredicateSignatureTemplate {
    owner: PredicateDefinitionSymbolId,
    parameters: Arc<[PredicateParameterTemplate]>,
    is_trusted: bool,
}

impl PredicateSignatureTemplate {
    /// Creates one predicate signature in declaration order.
    pub fn new(
        owner: PredicateDefinitionSymbolId,
        parameters: impl IntoIterator<Item = PredicateParameterTemplate>,
        is_trusted: bool,
    ) -> Self {
        Self {
            owner,
            parameters: shared_slice(parameters),
            is_trusted,
        }
    }

    /// Returns the predicate declaration that owns this signature.
    pub const fn owner(&self) -> PredicateDefinitionSymbolId {
        self.owner
    }

    /// Returns predicate parameters in declaration order.
    pub fn parameters(&self) -> &[PredicateParameterTemplate] {
        &self.parameters
    }

    /// Returns whether the predicate crosses a trusted boundary.
    pub const fn is_trusted(&self) -> bool {
        self.is_trusted
    }
}

/// The complete unevaluated header of one implementation declaration.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationHeadTemplate {
    implementation: ImplementationSymbolId,
    generic: GenericDeclarationTemplate,
    subject: ImplementationSubjectTemplate,
    trait_application: Option<TraitApplicationTemplate>,
}

impl ImplementationHeadTemplate {
    /// Creates one implementation header template.
    pub const fn new(
        implementation: ImplementationSymbolId,
        generic: GenericDeclarationTemplate,
        subject: ImplementationSubjectTemplate,
        trait_application: Option<TraitApplicationTemplate>,
    ) -> Self {
        Self {
            implementation,
            generic,
            subject,
            trait_application,
        }
    }

    /// Returns the implementation declaration.
    pub const fn implementation(&self) -> ImplementationSymbolId {
        self.implementation
    }

    /// Returns generic parameters and unevaluated constraints.
    pub const fn generic(&self) -> &GenericDeclarationTemplate {
        &self.generic
    }

    /// Returns the implementation subject template.
    pub const fn subject(&self) -> &ImplementationSubjectTemplate {
        &self.subject
    }

    /// Returns the implemented trait application when this is a trait implementation.
    pub const fn trait_application(&self) -> Option<&TraitApplicationTemplate> {
        self.trait_application.as_ref()
    }
}

/// One overload arm retained without resolving source path syntax prematurely.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OverloadArmTemplate {
    /// A source arm awaiting ordinary path resolution.
    Source(SyntaxAnchor),
    /// An imported or compiler-provided arm with resolved symbol identity.
    Resolved(AnySymbolId),
}

/// The ordered arm surface of one overload declaration.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OverloadSignatureTemplate {
    owner: AnySymbolId,
    arms: Arc<[OverloadArmTemplate]>,
}

impl OverloadSignatureTemplate {
    /// Creates one overload signature in declaration order.
    pub fn new(owner: AnySymbolId, arms: impl IntoIterator<Item = OverloadArmTemplate>) -> Self {
        Self {
            owner,
            arms: shared_slice(arms),
        }
    }

    /// Returns the overload declaration.
    pub const fn owner(&self) -> AnySymbolId {
        self.owner
    }

    /// Returns overload arms in declaration order.
    pub fn arms(&self) -> &[OverloadArmTemplate] {
        &self.arms
    }
}
