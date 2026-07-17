use bray_binder::{BinderFactError, BinderFactResult, SymbolFactProvider};
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableContractTypeFact, CallableParameterSymbolId, CallableSignatureFact,
    CallableSymbolId, ConstantDeclaredTypeFact, ImplementationCoherenceFact,
    ImplementationCoherenceKey, ImplementationSubject, ImplementationSubjectFact,
    ImplementationSymbolId, ImplementedTraitApplicationFact, InherentTypeMemberValueFact,
    ReceiverParameterSymbolId, StructFieldTypeFact, SymbolFactContract, SymbolFactRequest,
    SymbolFactResult, SymbolGraph, TraitConstantFulfillmentDeclaredTypeFact,
    TraitConstantMemberDeclaredTypeFact, TraitTypeFulfillmentValueFact, UnionPayloadFieldTypeFact,
};
use bray_syntax::{
    CallableContractDeclarationSyntax, ConstantDeclarationSyntax, ImplementationSubjectSyntax,
    ImplementationTypeMemberBindingSyntax, StructFieldDeclarationSyntax, TraitApplicationSyntax,
    TraitConstantMemberDeclarationSyntax, TypeExpressionSyntax, UnionPayloadFieldSyntax,
};

use super::super::context::CompilationBinderFacts;
use super::cache::CompilationSymbolFacts;
use super::environment::type_binder;
use super::surface::{declaration_callable_surface, declaration_child, declaration_syntax};
use crate::fact::SymbolFactCache;

pub(super) trait CompilationSymbolFactBinding<C>
where
    C: SymbolFactContract,
{
    fn cache(&self) -> &SymbolFactCache<C>;

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<C>,
    ) -> BinderFactResult<SymbolFactResult<C>>;
}

impl CompilationSymbolFactBinding<CallableSignatureFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<CallableSignatureFact> {
        &self.callable_signatures
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<CallableSignatureFact>,
    ) -> BinderFactResult<SymbolFactResult<CallableSignatureFact>> {
        bind_callable_signature(context, request.owner())
    }
}

macro_rules! impl_declared_type_fact {
    ($contract:ty, $cache:ident, $syntax:ty) => {
        impl CompilationSymbolFactBinding<$contract> for CompilationSymbolFacts {
            fn cache(&self) -> &SymbolFactCache<$contract> {
                &self.$cache
            }

            fn bind(
                &self,
                context: &CompilationBinderFacts<'_>,
                request: SymbolFactRequest<$contract>,
            ) -> BinderFactResult<SymbolFactResult<$contract>> {
                bind_declaration_type::<$syntax>(context, request.symbol(), |syntax| {
                    syntax.type_expression()
                })
            }
        }
    };
}

impl_declared_type_fact!(
    ConstantDeclaredTypeFact,
    constant_declared_types,
    ConstantDeclarationSyntax
);

impl_declared_type_fact!(
    TraitConstantMemberDeclaredTypeFact,
    trait_constant_member_declared_types,
    TraitConstantMemberDeclarationSyntax
);

impl_declared_type_fact!(
    TraitConstantFulfillmentDeclaredTypeFact,
    trait_constant_fulfillment_declared_types,
    ConstantDeclarationSyntax
);

impl_declared_type_fact!(
    CallableContractTypeFact,
    callable_contract_types,
    CallableContractDeclarationSyntax
);

impl_declared_type_fact!(
    StructFieldTypeFact,
    struct_field_types,
    StructFieldDeclarationSyntax
);

impl_declared_type_fact!(
    UnionPayloadFieldTypeFact,
    union_payload_field_types,
    UnionPayloadFieldSyntax
);

fn bind_declaration_type<T>(
    context: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
    type_expression: impl FnOnce(T) -> TypeExpressionSyntax,
) -> BinderFactResult<DiagnosticResult<bray_symbols::TypeId>>
where
    T: bray_syntax::SyntaxCast,
{
    let syntax = declaration_syntax::<T>(context, symbol)?;
    let type_expression = type_expression(syntax);

    type_binder(context, symbol)?.bind_type_expression(&type_expression)
}

macro_rules! impl_type_member_value_fact {
    ($contract:ty, $cache:ident) => {
        impl CompilationSymbolFactBinding<$contract> for CompilationSymbolFacts {
            fn cache(&self) -> &SymbolFactCache<$contract> {
                &self.$cache
            }

            fn bind(
                &self,
                context: &CompilationBinderFacts<'_>,
                request: SymbolFactRequest<$contract>,
            ) -> BinderFactResult<SymbolFactResult<$contract>> {
                bind_type_member_value::<$contract>(context, request.symbol())
            }
        }
    };
}

impl_type_member_value_fact!(InherentTypeMemberValueFact, inherent_type_member_values);

impl_type_member_value_fact!(TraitTypeFulfillmentValueFact, trait_type_fulfillment_values);

fn bind_type_member_value<C>(
    context: &CompilationBinderFacts<'_>,
    symbol: AnySymbolId,
) -> BinderFactResult<SymbolFactResult<C>>
where
    C: SymbolFactContract<Value = bray_symbols::TypeId>,
{
    let syntax = declaration_syntax::<ImplementationTypeMemberBindingSyntax>(context, symbol)?;

    type_binder(context, symbol)?.bind_type_expression(&syntax.type_expression())
}

impl CompilationSymbolFactBinding<ImplementationSubjectFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<ImplementationSubjectFact> {
        &self.implementation_subjects
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<ImplementationSubjectFact>,
    ) -> BinderFactResult<SymbolFactResult<ImplementationSubjectFact>> {
        let symbol = request.symbol();
        let syntax = declaration_child::<ImplementationSubjectSyntax>(context, symbol)?;
        let result = type_binder(context, symbol)?.bind_implementation_subject(&syntax)?;

        Ok(result.map(ImplementationSubject::new))
    }
}

impl CompilationSymbolFactBinding<ImplementedTraitApplicationFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<ImplementedTraitApplicationFact> {
        &self.implemented_traits
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<ImplementedTraitApplicationFact>,
    ) -> BinderFactResult<SymbolFactResult<ImplementedTraitApplicationFact>> {
        let symbol = request.symbol();

        if let ImplementationSymbolId::Inherent(id) = request.owner() {
            context
                .symbols
                .inherent_implementation(id)
                .ok_or(BinderFactError::DependencyUnavailable)?;

            return Ok(DiagnosticResult::without_diagnostics(None));
        }

        let syntax = declaration_child::<TraitApplicationSyntax>(context, symbol)?;
        let result = type_binder(context, symbol)?.bind_trait_application(&syntax)?;

        Ok(result.map(Some))
    }
}

impl CompilationSymbolFactBinding<ImplementationCoherenceFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<ImplementationCoherenceFact> {
        &self.implementation_coherence
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<ImplementationCoherenceFact>,
    ) -> BinderFactResult<SymbolFactResult<ImplementationCoherenceFact>> {
        let owner = request.owner();

        let subject =
            context.symbol_fact(SymbolFactRequest::<ImplementationSubjectFact>::new(owner))?;

        let trait_application = context.symbol_fact(SymbolFactRequest::<
            ImplementedTraitApplicationFact,
        >::new(owner))?;

        Ok(DiagnosticResult::without_diagnostics(
            ImplementationCoherenceKey::new(subject.value().ty(), *trait_application.value()),
        ))
    }
}

fn bind_callable_signature(
    context: &CompilationBinderFacts<'_>,
    callable: CallableSymbolId,
) -> BinderFactResult<SymbolFactResult<CallableSignatureFact>> {
    let symbol = callable.into_any();
    let (parameters, receiver) = callable_relationships(context.symbols, callable)?;
    let surface = declaration_callable_surface(context, symbol)?;

    type_binder(context, symbol)?.bind_callable_signature(
        callable,
        parameters,
        receiver,
        &surface.parameters,
        surface.result.as_ref(),
        surface.qualifiers,
    )
}

fn callable_relationships(
    symbols: &SymbolGraph,
    callable: CallableSymbolId,
) -> BinderFactResult<(
    &[CallableParameterSymbolId],
    Option<ReceiverParameterSymbolId>,
)> {
    match callable {
        CallableSymbolId::Function(id) => symbols
            .function(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::TypeMember(id) => symbols
            .type_callable_member(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::TraitMember(id) => symbols
            .trait_callable_member(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::TraitFulfillment(id) => symbols
            .trait_callable_fulfillment(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::Constructor(id) => symbols
            .constructor(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::Finalizer(id) => symbols
            .finalizer(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::Destructor(id) => symbols
            .destructor(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::ScopeEnter(id) => symbols
            .scope_enter(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::ScopeExit(id) => symbols
            .scope_exit(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::TraitFinalizer(id) => symbols
            .trait_finalizer_requirement(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::TraitDestructor(id) => symbols
            .trait_destructor_requirement(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::TraitScopeEnter(id) => symbols
            .trait_scope_enter_requirement(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::TraitScopeExit(id) => symbols
            .trait_scope_exit_requirement(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::TraitScopeEnterFulfillment(id) => symbols
            .trait_scope_enter_fulfillment(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
        CallableSymbolId::TraitScopeExitFulfillment(id) => symbols
            .trait_scope_exit_fulfillment(id)
            .map(|symbol| (symbol.parameters(), symbol.receiver())),
    }
    .ok_or(BinderFactError::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::{BinderFactError, SymbolFactProvider};
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{
        CallableContractTypeFact, CallableExecution, CallableSignatureFact, CallableSymbolId,
        ConstantDeclaredTypeFact, ImplementationSubjectFact, ImplementationSymbolId,
        ImplementedTraitApplicationFact, InherentTypeMemberValueFact, SymbolFactContract,
        SymbolFactRequest, SymbolFactResult, SymbolOrigin,
        TraitConstantFulfillmentDeclaredTypeFact, TraitConstantMemberDeclaredTypeFact,
        TraitTypeFulfillmentValueFact, TypeData, UnionPayloadFieldTypeFact,
    };

    use super::CompilationBinderFacts;
    use crate::fact::CancellationToken;
    use crate::test_support::compilation;

    const SOURCE_FACTS: &str = r#"module app;

const enabled: bool = true;

callable Transform =
    func(value: bool) -> bool;

struct Holder
{
    value: bool;

    construct(value: bool) -> Self
    {
    }

    async finalize()
    {
    }
}

union Choice
{
    Value(value: bool);
}

trait Provides
{
    const enabled: bool;
    type Item;
    func get() -> bool;
}

impl Holder(Provides)
{
    const enabled: bool = true;
    type Item = bool;

    func get() -> bool
    {
        true
    }
}

impl Holder
{
    type Local = bool;
}

func identity<T>(value: T) -> T
{
    value
}
"#;

    #[test]
    fn source_declaration_type_and_signature_facts_bind_and_publish_once() {
        let compilation = compilation(SOURCE_FACTS);
        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let facts = binder_facts(&compilation, &cancellation);

        let function = source_id(
            symbols.functions(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let signature_request =
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(function));

        let first_signature = published_fact(&facts, signature_request);
        let second_signature = published_fact(&facts, signature_request);

        assert!(Arc::ptr_eq(&first_signature, &second_signature));
        assert!(first_signature.diagnostics().is_empty());

        let [parameter] = first_signature.value().parameters() else {
            panic!("generic source function must retain one parameter");
        };

        assert!(matches!(
            type_data(&compilation, parameter.ty()).as_ref(),
            TypeData::TypeParameter(_)
        ));

        assert_eq!(parameter.ty(), first_signature.value().result());

        let callable_contract = source_id(
            symbols.callable_contracts(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let callable_contract_type = published_fact(
            &facts,
            SymbolFactRequest::<CallableContractTypeFact>::new(callable_contract),
        );

        assert!(matches!(
            type_data(&compilation, *callable_contract_type.value()).as_ref(),
            TypeData::Callable(_)
        ));

        let constant = source_id(
            symbols.constants(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let constant_type = published_fact(
            &facts,
            SymbolFactRequest::<ConstantDeclaredTypeFact>::new(constant),
        );

        assert!(matches!(
            type_data(&compilation, *constant_type.value()).as_ref(),
            TypeData::Named { .. }
        ));

        let field = source_id(
            symbols.struct_fields(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let field_type = published_fact(
            &facts,
            SymbolFactRequest::<bray_symbols::StructFieldTypeFact>::new(field),
        );

        assert!(matches!(
            type_data(&compilation, *field_type.value()).as_ref(),
            TypeData::Named { .. }
        ));

        let payload = source_id(
            symbols.union_payload_fields(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let payload_type = published_fact(
            &facts,
            SymbolFactRequest::<UnionPayloadFieldTypeFact>::new(payload),
        );

        assert!(matches!(
            type_data(&compilation, *payload_type.value()).as_ref(),
            TypeData::Named { .. }
        ));
    }

    #[test]
    fn source_member_and_implementation_facts_use_their_declared_surfaces() {
        let compilation = compilation(SOURCE_FACTS);
        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let facts = binder_facts(&compilation, &cancellation);

        let member = source_id(
            symbols.trait_constant_members(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let member_type = published_fact(
            &facts,
            SymbolFactRequest::<TraitConstantMemberDeclaredTypeFact>::new(member),
        );

        assert!(member_type.diagnostics().is_empty());

        let fulfillment = source_id(
            symbols.trait_constant_fulfillments(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let fulfillment_type = published_fact(
            &facts,
            SymbolFactRequest::<TraitConstantFulfillmentDeclaredTypeFact>::new(fulfillment),
        );

        assert_eq!(member_type.value(), fulfillment_type.value());

        let type_fulfillment = source_id(
            symbols.trait_type_fulfillments(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let type_value = published_fact(
            &facts,
            SymbolFactRequest::<TraitTypeFulfillmentValueFact>::new(type_fulfillment),
        );

        assert_eq!(member_type.value(), type_value.value());

        let inherent_type_member = source_id(
            symbols.inherent_type_members(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let inherent_type_value = published_fact(
            &facts,
            SymbolFactRequest::<InherentTypeMemberValueFact>::new(inherent_type_member),
        );

        assert_eq!(member_type.value(), inherent_type_value.value());

        let inherent_implementation = source_id(
            symbols.inherent_implementations(),
            |symbol| symbol.origin(),
            |symbol| ImplementationSymbolId::from(symbol.id()),
        );

        let inherent_subject = published_fact(
            &facts,
            SymbolFactRequest::<ImplementationSubjectFact>::new(inherent_implementation),
        );

        let inherent_trait = published_fact(
            &facts,
            SymbolFactRequest::<ImplementedTraitApplicationFact>::new(inherent_implementation),
        );

        assert!(matches!(
            type_data(&compilation, inherent_subject.value().ty()).as_ref(),
            TypeData::Named { .. }
        ));

        assert_eq!(*inherent_trait.value(), None);

        let implementation = source_id(
            symbols.unnamed_trait_implementations(),
            |symbol| symbol.origin(),
            |symbol| ImplementationSymbolId::from(symbol.id()),
        );

        let subject = published_fact(
            &facts,
            SymbolFactRequest::<ImplementationSubjectFact>::new(implementation),
        );

        let implemented_trait = published_fact(
            &facts,
            SymbolFactRequest::<ImplementedTraitApplicationFact>::new(implementation),
        );

        let subject_type = type_data(&compilation, subject.value().ty());

        assert!(
            matches!(subject_type.as_ref(), TypeData::Named { .. }),
            "unexpected implementation subject: {subject_type:?}, diagnostics: {:?}",
            subject.diagnostics()
        );

        assert!(implemented_trait.value().is_some());

        let callable_fulfillment = source_id(
            symbols.trait_callable_fulfillments(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(
                callable_fulfillment,
            )),
        );

        assert!(signature.diagnostics().is_empty());
        assert!(signature.value().receiver().is_some());
    }

    #[test]
    fn source_lifecycle_signatures_share_callable_surface_binding() {
        let compilation = compilation(SOURCE_FACTS);
        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let facts = binder_facts(&compilation, &cancellation);

        let constructor = source_id(
            symbols.constructors(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let constructor_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(constructor)),
        );

        assert_eq!(constructor_signature.value().parameters().len(), 1);
        assert!(matches!(
            type_data(&compilation, constructor_signature.value().result()).as_ref(),
            TypeData::ContextualSelf(_)
        ));

        let finalizer = source_id(
            symbols.finalizers(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let finalizer_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(finalizer)),
        );

        assert!(matches!(
            type_data(&compilation, finalizer_signature.value().callable_type()).as_ref(),
            TypeData::Callable(callable)
                if callable.execution() == CallableExecution::Asynchronous
        ));
    }

    #[test]
    fn source_fact_diagnostics_and_cancellation_remain_fact_owned() {
        let invalid_compilation = compilation(
            r#"module app;

func invalid(value: MissingType)
{
}
"#,
        );

        let symbols = symbol_graph(&invalid_compilation);
        let function = source_id(
            symbols.functions(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let cancellation = CancellationToken::new();
        let facts = binder_facts(&invalid_compilation, &cancellation);
        let request =
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(function));

        let signature = published_fact(&facts, request);
        let diagnostic_kinds = signature
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.kind())
            .collect::<Vec<_>>();

        assert_eq!(diagnostic_kinds, [DiagnosticKind::BindingUnresolvedName]);

        let cancelled_compilation = compilation(SOURCE_FACTS);
        let cancelled_symbols = symbol_graph(&cancelled_compilation);
        let cancelled_function = source_id(
            cancelled_symbols.functions(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let facts = binder_facts(&cancelled_compilation, &cancellation);
        let request = SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(
            cancelled_function,
        ));

        assert_eq!(facts.symbol_fact(request), Err(BinderFactError::Cancelled));
    }

    fn source_id<T, I: Copy>(
        symbols: &[T],
        origin: impl Fn(&T) -> SymbolOrigin,
        id: impl Fn(&T) -> I,
    ) -> I {
        let Some(symbol) = symbols
            .iter()
            .find(|symbol| origin(symbol) == SymbolOrigin::Source)
        else {
            panic!("test source must contain the expected declaration");
        };

        id(symbol)
    }

    fn published_fact<C>(
        facts: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<C>,
    ) -> Arc<SymbolFactResult<C>>
    where
        C: SymbolFactContract,
        for<'facts> CompilationBinderFacts<'facts>: SymbolFactProvider<C>,
    {
        match facts.symbol_fact(request) {
            Ok(result) => result,
            Err(error) => panic!("source symbol fact must bind: {error:?}"),
        }
    }

    fn binder_facts<'compilation>(
        compilation: &'compilation crate::Compilation,
        cancellation: &'compilation CancellationToken,
    ) -> CompilationBinderFacts<'compilation> {
        match compilation.binder_facts(cancellation) {
            Ok(facts) => facts,
            Err(error) => panic!("source binder facts must be available: {error:?}"),
        }
    }

    fn symbol_graph(compilation: &crate::Compilation) -> &bray_symbols::SymbolGraph {
        match compilation.symbol_graph() {
            Ok(symbols) => symbols,
            Err(error) => panic!("source symbol graph must build: {error:?}"),
        }
    }

    fn type_data(compilation: &crate::Compilation, ty: bray_symbols::TypeId) -> Arc<TypeData> {
        match compilation.semantic_value_store() {
            Ok(store) => match store.type_data(ty) {
                Ok(data) => data,
                Err(error) => panic!("source type fact must resolve: {error:?}"),
            },
            Err(error) => panic!("semantic values must be available: {error:?}"),
        }
    }
}
