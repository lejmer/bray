use crate::compilation::binder::BindingQueryResult;
use std::sync::Arc;

use bray_binder::{SymbolQueryErrorProvider, SymbolQueryProvider};
use bray_symbols::{
    CallableContractTemplateQuery, CallableContractTypeQuery, CallableContractsQuery,
    CallableOverloadTemplateQuery, CallableParameterDefaultQuery,
    CallableParameterDefaultTemplateQuery, CallableResultDependenciesQuery, CallableSignatureQuery,
    ConstantDeclaredTypeQuery, ConstantDefinitionQuery, DeclarationDirectivesQuery,
    GenericConstParameterDeclaredTypeQuery, GenericConstraintsQuery,
    GenericDeclarationTemplateQuery, ImplementationCoherenceQuery, ImplementationHeadTemplateQuery,
    ImplementationOverloadTemplateQuery, ImplementationSubjectQuery,
    ImplementedTraitApplicationQuery, InherentTypeMemberValueQuery, ModuleSurfaceQuery,
    PredicateDefinitionQuery, PredicateSignatureTemplateQuery, StaticDeclaredTypeQuery,
    StaticInstanceTemplateQuery, StructFieldDefaultQuery, StructFieldDefaultTemplateQuery,
    StructFieldTypeQuery, SymbolQueryContract, SymbolQueryRequest,
    TraitConstantFulfillmentDeclaredTypeQuery, TraitConstantFulfillmentDefinitionQuery,
    TraitConstantMemberDeclaredTypeQuery, TraitConstantMemberDefinitionQuery,
    TraitPredicateFulfillmentDefinitionQuery, TraitPredicateMemberDefinitionQuery,
    TraitTypeFulfillmentValueQuery, UnionPayloadFieldDefaultQuery,
    UnionPayloadFieldDefaultTemplateQuery, UnionPayloadFieldTypeQuery,
};

use super::super::binding_query_error;
use super::super::context::CompilationBindingContext;
use super::binding::{CompilationSymbolQueryEvaluator, binder_error};
use crate::fact::{CompilationFactKey, FactQueryError, SymbolQueryCache};

macro_rules! define_compilation_symbol_semantics {
    ($($field:ident: $contract:ty),+ $(,)?) => {
        pub(in crate::compilation) struct CompilationSymbolSemantics {
            $(pub(super) $field: SymbolQueryCache<$contract>,)+
        }

        impl CompilationSymbolSemantics {
            pub(in crate::compilation) fn new() -> Self {
                Self {
                    $($field: SymbolQueryCache::new(),)+
                }
            }

            pub(in crate::compilation) fn updated(
                &self,
                reusable: &std::collections::BTreeSet<CompilationFactKey>,
            ) -> Self {
                Self {
                    $($field: self.$field.updated(reusable),)+
                }
            }
        }
    };
}

define_compilation_symbol_semantics! {
    module_surfaces: ModuleSurfaceQuery,
    declaration_directives: DeclarationDirectivesQuery,
    generic_constraints: GenericConstraintsQuery,
    generic_declaration_templates: GenericDeclarationTemplateQuery,
    callable_signatures: CallableSignatureQuery,
    callable_result_dependencies: CallableResultDependenciesQuery,
    callable_contracts: CallableContractsQuery,
    callable_contract_templates: CallableContractTemplateQuery,
    predicate_signature_templates: PredicateSignatureTemplateQuery,
    callable_contract_types: CallableContractTypeQuery,
    constant_declared_types: ConstantDeclaredTypeQuery,
    constant_definitions: ConstantDefinitionQuery,
    static_declared_types: StaticDeclaredTypeQuery,
    static_instance_templates: StaticInstanceTemplateQuery,
    generic_const_parameter_declared_types: GenericConstParameterDeclaredTypeQuery,
    trait_constant_member_declared_types: TraitConstantMemberDeclaredTypeQuery,
    trait_constant_member_definitions: TraitConstantMemberDefinitionQuery,
    trait_constant_fulfillment_declared_types: TraitConstantFulfillmentDeclaredTypeQuery,
    trait_constant_fulfillment_definitions: TraitConstantFulfillmentDefinitionQuery,
    struct_field_types: StructFieldTypeQuery,
    union_payload_field_types: UnionPayloadFieldTypeQuery,
    inherent_type_member_values: InherentTypeMemberValueQuery,
    trait_type_fulfillment_values: TraitTypeFulfillmentValueQuery,
    implementation_subjects: ImplementationSubjectQuery,
    implemented_traits: ImplementedTraitApplicationQuery,
    implementation_coherence: ImplementationCoherenceQuery,
    implementation_head_templates: ImplementationHeadTemplateQuery,
    callable_parameter_default_templates: CallableParameterDefaultTemplateQuery,
    callable_parameter_defaults: CallableParameterDefaultQuery,
    struct_field_default_templates: StructFieldDefaultTemplateQuery,
    struct_field_defaults: StructFieldDefaultQuery,
    union_payload_field_default_templates: UnionPayloadFieldDefaultTemplateQuery,
    union_payload_field_defaults: UnionPayloadFieldDefaultQuery,
    predicate_definitions: PredicateDefinitionQuery,
    trait_predicate_member_definitions: TraitPredicateMemberDefinitionQuery,
    trait_predicate_fulfillment_definitions: TraitPredicateFulfillmentDefinitionQuery,
    callable_overload_templates: CallableOverloadTemplateQuery,
    implementation_overload_templates: ImplementationOverloadTemplateQuery,
}

impl SymbolQueryErrorProvider for CompilationBindingContext<'_> {
    type UpstreamError = FactQueryError;
}

impl<C> SymbolQueryProvider<C> for CompilationBindingContext<'_>
where
    C: SymbolQueryContract,
    CompilationSymbolSemantics: CompilationSymbolQueryEvaluator<C>,
{
    fn resolve_symbol_query(
        &self,
        request: SymbolQueryRequest<C>,
    ) -> BindingQueryResult<
        Arc<bray_diagnostics::DiagnosticResult<<C as bray_symbols::SymbolQueryContract>::Value>>,
    > {
        let semantics = self.symbol_semantics;
        let cache = semantics.cache();

        cache
            .get_or_compute(
                &self.compilation.state.fact_runtime,
                self.cancellation,
                request,
                || semantics.bind(self, request).map_err(binding_query_error),
            )
            .map_err(binder_error)
    }
}
