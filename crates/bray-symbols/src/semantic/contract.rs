use std::marker::PhantomData;

use bray_diagnostics::DiagnosticResult;

use crate::{
    AnySymbolId, CallableContractSymbolId, CallableParameterSymbolId, CallableSymbolId,
    ConstantSymbolId, GenericConstParameterSymbolId, GenericOwnerId, ImplementationSymbolId,
    InherentTypeMemberSymbolId, ModuleSymbolId, PredicateSymbolId, StructFieldSymbolId,
    TraitConstantFulfillmentSymbolId, TraitConstantMemberSymbolId,
    TraitPredicateFulfillmentSymbolId, TraitPredicateMemberSymbolId, TraitTypeFulfillmentSymbolId,
    UnionPayloadFieldSymbolId,
};

use super::{
    CallableContractSet, CallableContractTemplate, CallableSignatureTemplate,
    CheckedCallableParameterDefault, CheckedStructFieldDefault, CheckedUnionPayloadDefault,
    ConstantDefinitionState, DirectiveSurface, GenericConstraintSet, GenericDeclarationTemplate,
    ImplementationCandidateSet, ImplementationCoherenceDomainKey, ImplementationCoherenceKey,
    ImplementationHeadTemplate, ImplementationParticipationSet, ImplementationRequirementKey,
    ImplementationSelection, ImplementationSubjectTemplate, ModuleSurface,
    OverloadSignatureTemplate, PredicateDefinition, PredicateDefinitionState,
    PredicateSignatureTemplate, SymbolFactKind, TraitApplicationTemplate, TypeExpressionTemplate,
    UnevaluatedDefaultTemplate,
};

mod sealed {
    pub trait Sealed {}
}

/// Maps one typed symbol owner to its immutable semantic query value.
///
/// This sealed contract keeps the owner, value, and erased query category coupled at compile time.
pub trait SymbolFactContract: sealed::Sealed + Copy + Send + Sync + 'static {
    /// The exact symbol family that can own this query.
    type Owner: Copy + Ord + Send + Sync + 'static;
    /// The immutable semantic value supplied by the query.
    type Value: std::hash::Hash + Send + Sync + 'static;

    /// The category used by erased query coordination and completion.
    const KIND: SymbolFactKind;

    /// Erases the typed owner only for query coordination.
    fn erase_owner(owner: Self::Owner) -> AnySymbolId;
}

/// The diagnostic-bearing result of one typed symbol query contract.
pub type SymbolFactResult<C> = DiagnosticResult<<C as SymbolFactContract>::Value>;

/// Couples an instance-specific semantic query key to its immutable value.
///
/// These facts retain semantic inputs beyond one symbol owner.
pub trait SemanticFactContract: sealed::Sealed + Copy + Send + Sync + 'static {
    /// The complete symbol-domain inputs excluding compilation snapshot and target context.
    type Key: Send + Sync + 'static;
    /// The immutable semantic value supplied by the query.
    type Value: Send + Sync + 'static;
}

/// The diagnostic-bearing result of an instance-specific semantic query contract.
pub type SemanticFactResult<C> = DiagnosticResult<<C as SemanticFactContract>::Value>;

/// A typed request for one symbol-owned semantic query.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolFactRequest<C: SymbolFactContract> {
    owner: C::Owner,
    marker: PhantomData<fn() -> C>,
}

impl<C: SymbolFactContract> SymbolFactRequest<C> {
    /// Creates a request for the query owned by the exact symbol.
    pub const fn new(owner: C::Owner) -> Self {
        Self {
            owner,
            marker: PhantomData,
        }
    }

    /// Returns the typed symbol owner.
    pub const fn owner(self) -> C::Owner {
        self.owner
    }

    /// Returns the erased owner used by compilation query coordination.
    pub fn symbol(self) -> AnySymbolId {
        C::erase_owner(self.owner)
    }

    /// Returns the exact query category selected by the contract.
    pub const fn kind(self) -> SymbolFactKind {
        C::KIND
    }
}

macro_rules! define_symbol_fact_contract {
    (
        $(
            $(#[$meta:meta])*
            $contract:ident {
                owner: $owner:ty,
                value: $value:ty,
                kind: $kind:ident,
                erase: $erase:expr,
            }
        )+
    ) => {
        $(
            $(#[$meta])*
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            pub struct $contract;

            impl sealed::Sealed for $contract {}

            impl SymbolFactContract for $contract {
                type Owner = $owner;
                type Value = $value;

                const KIND: SymbolFactKind = SymbolFactKind::$kind;

                fn erase_owner(owner: Self::Owner) -> AnySymbolId {
                    ($erase)(owner)
                }
            }
        )+
    };
}

define_symbol_fact_contract! {
    /// Validated using relationships and re-export edges for one logical module.
    ModuleSurfaceQuery {
        owner: ModuleSymbolId,
        value: ModuleSurface,
        kind: Imports,
        erase: |owner: ModuleSymbolId| owner.into(),
    }
    /// Source-backed directives attached to one declaration symbol.
    DeclarationDirectivesQuery {
        owner: AnySymbolId,
        value: DirectiveSurface,
        kind: Directives,
        erase: |owner: AnySymbolId| owner,
    }
    /// Generic parameter identities and unevaluated constraints for one declaration.
    GenericDeclarationTemplateQuery {
        owner: GenericOwnerId,
        value: GenericDeclarationTemplate,
        kind: GenericDeclarationTemplate,
        erase: |owner: GenericOwnerId| owner.symbol(),
    }
    /// Checked generic constraints for one generic declaration owner.
    GenericConstraintsQuery {
        owner: GenericOwnerId,
        value: GenericConstraintSet,
        kind: GenericConstraints,
        erase: |owner: GenericOwnerId| owner.symbol(),
    }
    /// The declaration signature template of one callable.
    CallableSignatureQuery {
        owner: CallableSymbolId,
        value: CallableSignatureTemplate,
        kind: CallableSignature,
        erase: |owner: CallableSymbolId| owner.into_any(),
    }
    /// Checked contract clauses for one callable.
    CallableContractsQuery {
        owner: CallableSymbolId,
        value: CallableContractSet,
        kind: CallableContracts,
        erase: |owner: CallableSymbolId| owner.into_any(),
    }
    /// Unevaluated contract clauses for one callable.
    CallableContractTemplateQuery {
        owner: CallableSymbolId,
        value: CallableContractTemplate,
        kind: CallableContractTemplate,
        erase: |owner: CallableSymbolId| owner.into_any(),
    }
    /// The declaration signature template of one predicate.
    PredicateSignatureTemplateQuery {
        owner: crate::PredicateDefinitionSymbolId,
        value: PredicateSignatureTemplate,
        kind: PredicateSignatureTemplate,
        erase: |owner: crate::PredicateDefinitionSymbolId| owner.into_any(),
    }
    /// The callable type template named by one callable-contract declaration.
    CallableContractTypeQuery {
        owner: CallableContractSymbolId,
        value: TypeExpressionTemplate,
        kind: CallableContractType,
        erase: |owner: CallableContractSymbolId| owner.into(),
    }
    /// The declared type of one compile-time constant definition.
    ConstantDeclaredTypeQuery {
        owner: ConstantSymbolId,
        value: TypeExpressionTemplate,
        kind: ConstantDeclaredType,
        erase: |owner: ConstantSymbolId| owner.into(),
    }
    /// The declared type of one generic constant parameter.
    GenericConstParameterDeclaredTypeQuery {
        owner: GenericConstParameterSymbolId,
        value: TypeExpressionTemplate,
        kind: ConstantDeclaredType,
        erase: |owner: GenericConstParameterSymbolId| owner.into(),
    }
    /// The declared type of one trait constant member.
    TraitConstantMemberDeclaredTypeQuery {
        owner: TraitConstantMemberSymbolId,
        value: TypeExpressionTemplate,
        kind: ConstantDeclaredType,
        erase: |owner: TraitConstantMemberSymbolId| owner.into(),
    }
    /// The declared type of one trait constant fulfillment.
    TraitConstantFulfillmentDeclaredTypeQuery {
        owner: TraitConstantFulfillmentSymbolId,
        value: TypeExpressionTemplate,
        kind: ConstantDeclaredType,
        erase: |owner: TraitConstantFulfillmentSymbolId| owner.into(),
    }
    /// The checked template of one ordinary compile-time constant definition.
    ConstantDefinitionQuery {
        owner: ConstantSymbolId,
        value: ConstantDefinitionState,
        kind: ConstantDefinition,
        erase: |owner: ConstantSymbolId| owner.into(),
    }
    /// The checked template state of one trait constant member.
    TraitConstantMemberDefinitionQuery {
        owner: TraitConstantMemberSymbolId,
        value: ConstantDefinitionState,
        kind: ConstantDefinition,
        erase: |owner: TraitConstantMemberSymbolId| owner.into(),
    }
    /// The checked template of one trait constant fulfillment.
    TraitConstantFulfillmentDefinitionQuery {
        owner: TraitConstantFulfillmentSymbolId,
        value: ConstantDefinitionState,
        kind: ConstantDefinition,
        erase: |owner: TraitConstantFulfillmentSymbolId| owner.into(),
    }
    /// The checked runtime default of one callable parameter.
    CallableParameterDefaultQuery {
        owner: CallableParameterSymbolId,
        value: CheckedCallableParameterDefault,
        kind: CallableParameterDefault,
        erase: |owner: CallableParameterSymbolId| owner.into(),
    }
    /// The unevaluated default expression of one callable parameter.
    CallableParameterDefaultTemplateQuery {
        owner: CallableParameterSymbolId,
        value: UnevaluatedDefaultTemplate,
        kind: UnevaluatedDefaultTemplate,
        erase: |owner: CallableParameterSymbolId| owner.into(),
    }
    /// The declared type template of one struct field.
    StructFieldTypeQuery {
        owner: StructFieldSymbolId,
        value: TypeExpressionTemplate,
        kind: StructFieldType,
        erase: |owner: StructFieldSymbolId| owner.into(),
    }
    /// The type-value template supplied by one inherent implementation member.
    InherentTypeMemberValueQuery {
        owner: InherentTypeMemberSymbolId,
        value: TypeExpressionTemplate,
        kind: TypeMemberValue,
        erase: |owner: InherentTypeMemberSymbolId| owner.into(),
    }
    /// The type-value template supplied by one trait implementation fulfillment.
    TraitTypeFulfillmentValueQuery {
        owner: TraitTypeFulfillmentSymbolId,
        value: TypeExpressionTemplate,
        kind: TypeMemberValue,
        erase: |owner: TraitTypeFulfillmentSymbolId| owner.into(),
    }
    /// The checked runtime default of one struct field.
    StructFieldDefaultQuery {
        owner: StructFieldSymbolId,
        value: CheckedStructFieldDefault,
        kind: StructFieldDefault,
        erase: |owner: StructFieldSymbolId| owner.into(),
    }
    /// The unevaluated default expression of one struct field.
    StructFieldDefaultTemplateQuery {
        owner: StructFieldSymbolId,
        value: UnevaluatedDefaultTemplate,
        kind: UnevaluatedDefaultTemplate,
        erase: |owner: StructFieldSymbolId| owner.into(),
    }
    /// The declared type template of one union payload field.
    UnionPayloadFieldTypeQuery {
        owner: UnionPayloadFieldSymbolId,
        value: TypeExpressionTemplate,
        kind: UnionPayloadFieldType,
        erase: |owner: UnionPayloadFieldSymbolId| owner.into(),
    }
    /// The checked runtime default of one union payload field.
    UnionPayloadFieldDefaultQuery {
        owner: UnionPayloadFieldSymbolId,
        value: CheckedUnionPayloadDefault,
        kind: UnionPayloadFieldDefault,
        erase: |owner: UnionPayloadFieldSymbolId| owner.into(),
    }
    /// The unevaluated default expression of one union payload field.
    UnionPayloadFieldDefaultTemplateQuery {
        owner: UnionPayloadFieldSymbolId,
        value: UnevaluatedDefaultTemplate,
        kind: UnevaluatedDefaultTemplate,
        erase: |owner: UnionPayloadFieldSymbolId| owner.into(),
    }
    /// The checked semantic definition of one predicate declaration.
    PredicateDefinitionQuery {
        owner: PredicateSymbolId,
        value: PredicateDefinitionState<PredicateDefinition>,
        kind: PredicateDefinition,
        erase: |owner: PredicateSymbolId| owner.into(),
    }
    /// The checked semantic definition state of one trait predicate member.
    TraitPredicateMemberDefinitionQuery {
        owner: TraitPredicateMemberSymbolId,
        value: PredicateDefinitionState<PredicateDefinition>,
        kind: PredicateDefinition,
        erase: |owner: TraitPredicateMemberSymbolId| owner.into(),
    }
    /// The checked semantic definition of one trait predicate fulfillment.
    TraitPredicateFulfillmentDefinitionQuery {
        owner: TraitPredicateFulfillmentSymbolId,
        value: PredicateDefinitionState<PredicateDefinition>,
        kind: PredicateDefinition,
        erase: |owner: TraitPredicateFulfillmentSymbolId| owner.into(),
    }
    /// The subject type template of one implementation declaration.
    ImplementationSubjectQuery {
        owner: ImplementationSymbolId,
        value: ImplementationSubjectTemplate,
        kind: ImplementationSubject,
        erase: |owner: ImplementationSymbolId| owner.into_any(),
    }
    /// The optional trait-application template implemented by one implementation.
    ImplementedTraitApplicationQuery {
        owner: ImplementationSymbolId,
        value: Option<TraitApplicationTemplate>,
        kind: ImplementedTraitApplication,
        erase: |owner: ImplementationSymbolId| owner.into_any(),
    }
    /// The complete unevaluated header of one implementation declaration.
    ImplementationHeadTemplateQuery {
        owner: ImplementationSymbolId,
        value: ImplementationHeadTemplate,
        kind: ImplementationHeadTemplate,
        erase: |owner: ImplementationSymbolId| owner.into_any(),
    }
    /// The ordered arm template of one callable overload declaration.
    CallableOverloadTemplateQuery {
        owner: crate::CallableOverloadSymbolId,
        value: OverloadSignatureTemplate,
        kind: OverloadSignatureTemplate,
        erase: |owner: crate::CallableOverloadSymbolId| owner.into(),
    }
    /// The ordered arm template of one implementation overload declaration.
    ImplementationOverloadTemplateQuery {
        owner: crate::ImplementationOverloadSymbolId,
        value: OverloadSignatureTemplate,
        kind: OverloadSignatureTemplate,
        erase: |owner: crate::ImplementationOverloadSymbolId| owner.into(),
    }
    /// The checked coherence key of one implementation declaration.
    ImplementationCoherenceQuery {
        owner: ImplementationSymbolId,
        value: ImplementationCoherenceKey,
        kind: ImplementationCoherence,
        erase: |owner: ImplementationSymbolId| owner.into_any(),
    }
}

macro_rules! define_semantic_fact_contract {
    (
        $(
            $(#[$meta:meta])*
            $contract:ident {
                key: $key:ty,
                value: $value:ty,
            }
        )+
    ) => {
        $(
            $(#[$meta])*
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            pub struct $contract;

            impl sealed::Sealed for $contract {}

            impl SemanticFactContract for $contract {
                type Key = $key;
                type Value = $value;
            }
        )+
    };
}

define_semantic_fact_contract! {
    /// Finds the implementations participating in one package coherence domain.
    ImplementationParticipationQuery {
        key: ImplementationCoherenceDomainKey,
        value: ImplementationParticipationSet,
    }
    /// Finds uncommitted implementation candidates for an exact subject and trait application.
    ImplementationCandidateSetQuery {
        key: ImplementationRequirementKey,
        value: ImplementationCandidateSet,
    }
    /// Selects one implementation witness for an exact subject and trait application.
    ImplementationSelectionQuery {
        key: ImplementationRequirementKey,
        value: ImplementationSelection,
    }
    /// Checks member fulfillment validity for one trait implementation.
    TraitImplementationConformanceQuery {
        key: ImplementationSymbolId,
        value: crate::TraitImplementationConformance,
    }
    /// Proves every static constraint for one exact generic declaration instance.
    GenericConstraintSatisfactionQuery {
        key: crate::GenericConstraintObligationKey,
        value: crate::ProofOutcome,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CallableSignatureQuery, ConstantDefinitionQuery, ConstantSymbolId, FunctionSymbolId,
        ImplementationCandidateSetQuery, ImplementationCoherenceDomainKey,
        ImplementationParticipationQuery, ImplementationRequirementKey, ImplementationSelectionQuery,
        SemanticFactContract, SymbolFactKind, SymbolFactRequest, SymbolId,
        TraitConstantMemberDefinitionQuery, TraitConstantMemberSymbolId,
    };

    #[test]
    fn typed_requests_couple_owner_value_and_query_category() {
        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(7));
        let request = SymbolFactRequest::<CallableSignatureQuery>::new(function.into());

        assert_eq!(request.symbol(), function.into());
        assert_eq!(request.kind(), SymbolFactKind::CallableSignature);
        assert_eq!(request.owner().symbol_id(), function.symbol_id());
    }

    #[test]
    fn typed_query_contracts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CallableSignatureQuery>();
        assert_send_sync::<SymbolFactRequest<CallableSignatureQuery>>();
    }

    #[test]
    fn constant_query_contracts_preserve_exact_declaration_categories() {
        let raw = SymbolId::new(9);

        let constant = ConstantSymbolId::from_symbol_id(raw);
        let trait_member = TraitConstantMemberSymbolId::from_symbol_id(raw);

        let constant = SymbolFactRequest::<ConstantDefinitionQuery>::new(constant);

        let trait_member =
            SymbolFactRequest::<TraitConstantMemberDefinitionQuery>::new(trait_member);

        assert_eq!(constant.kind(), SymbolFactKind::ConstantDefinition);
        assert_eq!(trait_member.kind(), SymbolFactKind::ConstantDefinition);
        assert_eq!(constant.symbol().symbol_id(), raw);
        assert_eq!(trait_member.symbol().symbol_id(), raw);
        assert_ne!(constant.symbol(), trait_member.symbol());
    }

    #[test]
    fn instance_specific_contracts_retain_typed_keys_and_values() {
        fn assert_candidate_contract<C>()
        where
            C: SemanticFactContract<
                    Key = ImplementationRequirementKey,
                    Value = crate::ImplementationCandidateSet,
                >,
        {
        }

        fn assert_selection_contract<C>()
        where
            C: SemanticFactContract<
                    Key = ImplementationRequirementKey,
                    Value = crate::ImplementationSelection,
                >,
        {
        }

        fn assert_participation_contract<C>()
        where
            C: SemanticFactContract<
                    Key = ImplementationCoherenceDomainKey,
                    Value = crate::ImplementationParticipationSet,
                >,
        {
        }

        assert_candidate_contract::<ImplementationCandidateSetQuery>();
        assert_selection_contract::<ImplementationSelectionQuery>();
        assert_participation_contract::<ImplementationParticipationQuery>();
    }
}
