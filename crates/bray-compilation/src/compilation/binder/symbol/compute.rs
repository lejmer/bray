use bray_binder::{BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider};
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableContractTypeFact, CallableSignatureFact, CallableSymbolId,
    ConstantDeclaredTypeFact, ExactSymbolId, GenericConstParameterDeclaredTypeFact,
    GenericConstParameterSymbolId, ImplementationCoherenceFact, ImplementationCoherenceKey,
    ImplementationSubjectFact, ImplementationSubjectTemplate, ImplementationSymbolId,
    ImplementedTraitApplicationFact, InherentTypeMemberValueFact, StructFieldTypeFact,
    SymbolFactContract, SymbolFactRequest, SymbolFactResult, SymbolOrigin,
    TraitConstantFulfillmentDeclaredTypeFact, TraitConstantMemberDeclaredTypeFact,
    TraitTypeFulfillmentValueFact, UnionPayloadFieldTypeFact,
};
use bray_syntax::{
    CallableContractDeclarationSyntax, ConstantDeclarationSyntax, GenericConstParameterSyntax,
    ImplementationSubjectSyntax, ImplementationTypeMemberBindingSyntax,
    StructFieldDeclarationSyntax, SyntaxKind, SyntaxWalkControl, TraitApplicationSyntax,
    TraitConstantMemberDeclarationSyntax, TypeExpressionSyntax, UnionPayloadFieldSyntax,
    walk_direct_child_nodes,
};

use super::super::context::CompilationBinderFacts;
use super::binding::CompilationSymbolFactBinding;
use super::cache::CompilationSymbolFacts;
use super::environment::type_binder;
use super::surface::{declaration_callable_surface, declaration_child, declaration_syntax};
use crate::fact::SymbolFactCache;

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

impl CompilationSymbolFactBinding<ConstantDeclaredTypeFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<ConstantDeclaredTypeFact> {
        &self.constant_declared_types
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<ConstantDeclaredTypeFact>,
    ) -> BinderFactResult<SymbolFactResult<ConstantDeclaredTypeFact>> {
        if let Some(fact) = context
            .compilation()
            .available_compiler_known_symbols()
            .provider()
            .symbol_target_fact(request.owner())
        {
            let ty = context
                .compilation()
                .target_fact_type(fact)
                .map_err(|_| BinderFactError::DependencyUnavailable)?;

            return Ok(DiagnosticResult::without_diagnostics(
                bray_symbols::TypeExpressionTemplate::Resolved(ty),
            ));
        }

        bind_declaration_type::<ConstantDeclarationSyntax>(context, request.symbol(), |syntax| {
            syntax.type_expression()
        })
    }
}

impl CompilationSymbolFactBinding<GenericConstParameterDeclaredTypeFact>
    for CompilationSymbolFacts
{
    fn cache(&self) -> &SymbolFactCache<GenericConstParameterDeclaredTypeFact> {
        &self.generic_const_parameter_declared_types
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<GenericConstParameterDeclaredTypeFact>,
    ) -> BinderFactResult<SymbolFactResult<GenericConstParameterDeclaredTypeFact>> {
        let symbol = request.symbol();

        if let Some(address) = context.imported_fact_address(symbol.into())? {
            return super::imported::imported_declared_type(context, address);
        }

        let parameter = GenericConstParameterSymbolId::try_from_any(symbol)
            .ok_or(BinderFactError::DependencyUnavailable)?;

        let origin = context
            .symbols()
            .generic_const_parameter(parameter)
            .map(|parameter| parameter.origin())
            .ok_or(BinderFactError::DependencyUnavailable)?;

        match origin {
            SymbolOrigin::Source => {
                bind_declaration_type::<GenericConstParameterSyntax>(context, symbol, |syntax| {
                    syntax.typed_identifier().type_expression()
                })
            }
            SymbolOrigin::CompilerKnown | SymbolOrigin::CompilerProvided => {
                let syntax = compiler_known_generic_const_parameter(context, parameter)?;

                type_binder(context, symbol)?
                    .bind_type_expression(&syntax.typed_identifier().type_expression())
            }
            SymbolOrigin::Imported | SymbolOrigin::Synthesized => {
                Err(BinderFactError::DependencyUnavailable)
            }
        }
    }
}

fn compiler_known_generic_const_parameter(
    context: &CompilationBinderFacts<'_>,
    symbol: GenericConstParameterSymbolId,
) -> BinderFactResult<GenericConstParameterSyntax> {
    let parameter = context
        .symbols()
        .generic_const_parameter(symbol)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let owner = parameter.owner().symbol();

    let ordinal = usize::try_from(parameter.ordinal())
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    super::surface::with_declaration_root(context, owner, |root| {
        let mut parameter = None;
        let mut current = 0_usize;

        walk_direct_child_nodes(&root, |child| {
            if child.kind() != SyntaxKind::GenericParameterList {
                return SyntaxWalkControl::Continue;
            }

            walk_direct_child_nodes(&child, |candidate| {
                if !matches!(
                    candidate.kind(),
                    SyntaxKind::GenericTypeParameter | SyntaxKind::GenericConstParameter
                ) {
                    return SyntaxWalkControl::Continue;
                }

                if current == ordinal {
                    parameter = candidate.cast::<GenericConstParameterSyntax>();

                    return SyntaxWalkControl::Stop;
                }

                current = current.saturating_add(1);

                SyntaxWalkControl::Continue
            });

            if parameter.is_some() {
                SyntaxWalkControl::Stop
            } else {
                SyntaxWalkControl::Continue
            }
        });

        parameter.ok_or(BinderFactError::DependencyUnavailable)
    })
}

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
) -> BinderFactResult<DiagnosticResult<bray_symbols::TypeExpressionTemplate>>
where
    T: bray_syntax::SyntaxCast,
{
    if let Some(address) = context.imported_fact_address(symbol)? {
        return super::imported::imported_declared_type(context, address);
    }

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
    C: SymbolFactContract<Value = bray_symbols::TypeExpressionTemplate>,
{
    if let Some(address) = context.imported_fact_address(symbol)? {
        return super::imported::imported_declared_type(context, address);
    }

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

        if let Some(address) = context.imported_fact_address(symbol)? {
            let imported = super::imported::imported_implementation(context, address)?;

            return Ok(imported.map(|implementation| {
                ImplementationSubjectTemplate::new(bray_symbols::TypeExpressionTemplate::Resolved(
                    implementation.subject().ty(),
                ))
            }));
        }

        let syntax = declaration_child::<ImplementationSubjectSyntax>(context, symbol)?;
        let result = type_binder(context, symbol)?.bind_implementation_subject(&syntax)?;

        Ok(result.map(ImplementationSubjectTemplate::new))
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

        if let Some(address) = context.imported_fact_address(symbol)? {
            return super::imported::imported_implemented_trait_application(context, address);
        }

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

        if let Some(address) = context.imported_fact_address(owner.into_any())? {
            let imported = super::imported::imported_implementation(context, address)?;

            return Ok(imported.map(|implementation| {
                ImplementationCoherenceKey::new(
                    implementation.subject().ty(),
                    implementation.trait_application(),
                )
            }));
        }

        let subject =
            context.symbol_fact(SymbolFactRequest::<ImplementationSubjectFact>::new(owner))?;

        let trait_application = context.symbol_fact(SymbolFactRequest::<
            ImplementedTraitApplicationFact,
        >::new(owner))?;

        let subject = subject
            .value()
            .ty()
            .resolved_type()
            .ok_or(BinderFactError::DependencyUnavailable)?;

        let trait_application = match trait_application.value() {
            Some(application) => type_binder(context, owner.into_any())?
                .resolve_trait_application_template(application)?
                .ok_or(BinderFactError::DependencyUnavailable)?
                .into(),
            None => None,
        };

        Ok(DiagnosticResult::without_diagnostics(
            ImplementationCoherenceKey::new(subject, trait_application),
        ))
    }
}

fn bind_callable_signature(
    context: &CompilationBinderFacts<'_>,
    callable: CallableSymbolId,
) -> BinderFactResult<SymbolFactResult<CallableSignatureFact>> {
    let symbol = callable.into_any();

    if let Some(address) = context.imported_fact_address(symbol)? {
        return super::imported::imported_callable_signature(context, address);
    }

    let (parameters, receiver) = context
        .symbols
        .callable_parameters_and_receiver(callable)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let surface = declaration_callable_surface(context, symbol)?;

    let result = type_binder(context, symbol)?.bind_callable_signature(
        callable,
        parameters,
        receiver,
        &surface.parameters,
        surface.result.as_ref(),
        surface.qualifiers,
    )?;

    Ok(result.map(|signature| signature.with_body(surface.has_body)))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::{BinderFactError, SymbolFactProvider};
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{
        AnySymbolId, CallableAbi, CallableContractTypeFact, CallableExecution,
        CallableSignatureFact, CallableSymbolId, ConstantDeclaredTypeFact,
        ConstantExpressionExpectedType, ConstantExpressionOccurrence, GenericArgument,
        GenericArgumentTemplate, GenericConstParameterDeclaredTypeFact, ImplementationSubjectFact,
        ImplementationSymbolId, ImplementedTraitApplicationFact, InherentTypeMemberValueFact,
        NamedTypeSymbolId, ReceiverMode, StructSymbolId, SymbolFactRequest, SymbolOrigin,
        TraitConstantFulfillmentDeclaredTypeFact, TraitConstantMemberDeclaredTypeFact,
        TraitTypeFulfillmentValueFact, TypeData, TypeExpressionTemplate, UnionPayloadFieldTypeFact,
    };
    use bray_syntax::SyntaxKind;

    use crate::compilation::binder::symbol::test_support::{
        binder_facts, published_fact, resolved_type, source_id, symbol_graph, type_data,
    };
    use crate::fact::CancellationToken;
    use crate::test_support::compilation;

    const SOURCE_FACTS: &str = r#"module app;

const enabled: bool = true;

callable Transform =
    @abi(system) func(value: bool) -> bool;

struct Fixed<const count: usize>
{
}

struct Holder
{
    value: bool;
    default_box: box bool;
    explicit_box: box[Heap] bool;
    grouped: (bool);
    tuple: (bool,);
    slice: [bool];
    array: [bool; 4];
    trait_view: view Provides;
    fixed: Fixed<4>;

    construct(value: bool) -> Self
    {
    }

    async finalize()
    {
    }

    destruct()
    {
    }

    consume mut enter() -> bool
    {
        return true;
    }

    exit(pos lease: bool)
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
    async finalize();
    destruct();
    consume enter() -> bool;
    exit(pos lease: bool);
}

impl Holder(Provides)
{
    const enabled: bool = true;
    type Item = bool;

    func get() -> bool
    {
        return true;
    }

    consume enter() -> bool
    {
        return true;
    }

    exit(pos lease: bool)
    {
    }
}

impl Holder
{
    type Local = bool;
}

@abi(c)
func identity<T>(value: T) -> T
{
    return value;
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

        let callable_type = type_data(&compilation, first_signature.value().callable_type());

        let TypeData::Callable(callable) = callable_type.as_ref() else {
            panic!("source function must retain its callable type");
        };

        let [callable_parameter] = callable.parameters() else {
            panic!("generic source function must retain one callable parameter type");
        };

        assert!(matches!(
            type_data(&compilation, callable_parameter.ty()).as_ref(),
            TypeData::TypeParameter(_)
        ));

        assert_eq!(callable_parameter.ty(), callable.result());

        let Some(function_symbol) = symbols.function(function) else {
            panic!("source function must remain in the symbol graph");
        };

        assert_eq!(*parameter, function_symbol.parameters()[0]);

        assert!(matches!(
            callable_type.as_ref(),
            TypeData::Callable(callable) if callable.abi() == CallableAbi::C
        ));

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
            type_data(&compilation, callable_contract_type.value()).as_ref(),
            TypeData::Callable(callable) if callable.abi() == CallableAbi::System
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
            type_data(&compilation, constant_type.value()).as_ref(),
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
            type_data(&compilation, field_type.value()).as_ref(),
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
            type_data(&compilation, payload_type.value()).as_ref(),
            TypeData::Named { .. }
        ));
    }

    #[test]
    fn implementation_subjects_bind_inferred_type_parameter_ids() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Buffer<T>\n",
            "{\n",
            "}\n",
            "impl Buffer<T>\n",
            "{\n",
            "}\n",
        ));

        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let facts = binder_facts(&compilation, &cancellation);

        let implementation = source_id(
            symbols.inherent_implementations(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let Some(implementation_symbol) = symbols.inherent_implementation(implementation) else {
            panic!("implementation ID should resolve");
        };

        let [parameter] = implementation_symbol.generic_type_parameters() else {
            panic!("implementation should publish one inferred type parameter");
        };

        let subject = published_fact(
            &facts,
            SymbolFactRequest::<ImplementationSubjectFact>::new(ImplementationSymbolId::from(
                implementation,
            )),
        );

        let subject_type = type_data(&compilation, subject.value().ty());

        let TypeData::Named { substitution, .. } = subject_type.as_ref() else {
            panic!("implementation subject should resolve its generic application");
        };

        let values = match compilation.semantic_value_store() {
            Ok(values) => values,
            Err(error) => panic!("semantic value store should be available: {error:?}"),
        };

        let substitution = match values.generic_substitution_data(*substitution) {
            Ok(substitution) => substitution,
            Err(error) => panic!("subject substitution should resolve: {error:?}"),
        };

        let [binding] = substitution.bindings() else {
            panic!("implementation subject should bind one generic argument");
        };

        let GenericArgument::Type(argument) = binding.argument() else {
            panic!("implementation subject argument should be a type");
        };

        assert_eq!(
            type_data(&compilation, argument).as_ref(),
            &TypeData::TypeParameter(*parameter)
        );

        assert!(subject.diagnostics().is_empty());
    }

    #[test]
    fn source_member_and_implementation_facts_use_their_declared_surfaces() {
        let compilation = compilation(SOURCE_FACTS);

        assert!(
            compilation.syntax_tree_result().diagnostics().is_empty(),
            "test source must parse without recovery: {:?}",
            compilation.syntax_tree_result().diagnostics()
        );

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

        let inherent_subject_type = type_data(&compilation, inherent_subject.value().ty());

        assert!(
            matches!(inherent_subject_type.as_ref(), TypeData::Named { .. }),
            "unexpected inherent implementation subject: {inherent_subject_type:?}, diagnostics: {:?}",
            inherent_subject.diagnostics()
        );

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
        assert!(constructor_signature.value().receiver().is_none());

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

        assert_eq!(
            finalizer_signature
                .value()
                .receiver()
                .map(|receiver| receiver.mode()),
            Some(ReceiverMode::Mutable)
        );

        let destructor = source_id(
            symbols.destructors(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let destructor_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(destructor)),
        );

        assert_eq!(
            destructor_signature
                .value()
                .receiver()
                .map(|receiver| receiver.mode()),
            Some(ReceiverMode::ConsumingMutable)
        );

        let scope_enter = source_id(
            symbols.scope_enters(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let scope_enter_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(scope_enter)),
        );

        assert_eq!(
            scope_enter_signature
                .value()
                .receiver()
                .map(|receiver| receiver.mode()),
            Some(ReceiverMode::ConsumingMutable)
        );

        let scope_exit = source_id(
            symbols.scope_exits(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let scope_exit_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(scope_exit)),
        );

        assert!(scope_exit_signature.value().receiver().is_none());

        let trait_finalizer = source_id(
            symbols.trait_finalizer_requirements(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let trait_finalizer_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(
                trait_finalizer,
            )),
        );

        assert_eq!(
            trait_finalizer_signature
                .value()
                .receiver()
                .map(|receiver| receiver.mode()),
            Some(ReceiverMode::Mutable)
        );

        let trait_destructor = source_id(
            symbols.trait_destructor_requirements(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let trait_destructor_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(
                trait_destructor,
            )),
        );

        assert_eq!(
            trait_destructor_signature
                .value()
                .receiver()
                .map(|receiver| receiver.mode()),
            Some(ReceiverMode::ConsumingMutable)
        );

        let trait_scope_enter = source_id(
            symbols.trait_scope_enter_requirements(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let trait_scope_enter_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(
                trait_scope_enter,
            )),
        );

        assert_eq!(
            trait_scope_enter_signature
                .value()
                .receiver()
                .map(|receiver| receiver.mode()),
            Some(ReceiverMode::Consuming)
        );

        let trait_scope_exit = source_id(
            symbols.trait_scope_exit_requirements(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let trait_scope_exit_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(
                trait_scope_exit,
            )),
        );

        assert!(trait_scope_exit_signature.value().receiver().is_none());

        let trait_scope_enter_fulfillment = source_id(
            symbols.trait_scope_enter_fulfillments(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let trait_scope_enter_fulfillment_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(
                trait_scope_enter_fulfillment,
            )),
        );

        assert_eq!(
            trait_scope_enter_fulfillment_signature
                .value()
                .receiver()
                .map(|receiver| receiver.mode()),
            Some(ReceiverMode::Consuming)
        );

        let trait_scope_exit_fulfillment = source_id(
            symbols.trait_scope_exit_fulfillments(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let trait_scope_exit_fulfillment_signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(
                trait_scope_exit_fulfillment,
            )),
        );

        assert!(
            trait_scope_exit_fulfillment_signature
                .value()
                .receiver()
                .is_none()
        );
    }

    #[test]
    fn source_type_forms_retain_structural_type_facts() {
        let compilation = compilation(SOURCE_FACTS);
        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let facts = binder_facts(&compilation, &cancellation);

        let fields = symbols
            .struct_fields()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [
            value,
            default_box,
            explicit_box,
            grouped,
            tuple,
            slice,
            array,
            trait_view,
            fixed,
        ] = fields.as_slice()
        else {
            panic!("test source must contain all structural type-form fields: {fields:?}");
        };

        let field_type = |field: &bray_symbols::StructFieldSymbol| {
            published_fact(
                &facts,
                SymbolFactRequest::<bray_symbols::StructFieldTypeFact>::new(field.id()),
            )
            .value()
            .clone()
        };

        let value = field_type(value);
        let default_box = field_type(default_box);
        let explicit_box = field_type(explicit_box);
        let grouped = field_type(grouped);
        let tuple = field_type(tuple);
        let slice = field_type(slice);
        let array = field_type(array);
        let trait_view = field_type(trait_view);
        let fixed = field_type(fixed);

        assert_eq!(default_box, explicit_box);
        assert_eq!(grouped, value);

        let value = resolved_type(&value);

        assert!(matches!(
            type_data(&compilation, &default_box).as_ref(),
            TypeData::OwnedIndirection { storage, target }
                if *target == value
                    && matches!(type_data(&compilation, *storage).as_ref(), TypeData::Named { .. })
        ));

        assert!(matches!(
            type_data(&compilation, &tuple).as_ref(),
            TypeData::Tuple(elements) if elements.as_ref() == [value]
        ));

        assert!(matches!(
            type_data(&compilation, &slice).as_ref(),
            TypeData::Slice(element) if *element == value
        ));

        let TypeExpressionTemplate::Array { element, length } = array else {
            panic!("array type must retain its source length occurrence");
        };

        assert_eq!(resolved_type(&element), value);

        assert!(matches!(
            length.expected_type(),
            ConstantExpressionExpectedType::Resolved(_)
        ));

        assert!(matches!(
            type_data(&compilation, &trait_view).as_ref(),
            TypeData::TraitView(_)
        ));

        let TypeExpressionTemplate::Named { arguments, .. } = fixed else {
            panic!("constant generic application must remain a named type template");
        };

        assert!(matches!(
            arguments.as_ref(),
            [GenericArgumentTemplate::Constant(_)]
        ));
    }

    #[test]
    fn source_constant_type_forms_preserve_stable_expression_occurrences() {
        let compilation = compilation(
            r#"module app;

struct Fixed<const count: usize>
{
}

struct Values<const count: usize>
{
    symbolic_array: [bool; count];
    symbolic_application: Fixed<count>;
    target_sensitive_application: Fixed<4>;
    compound_application: Fixed<count + 1>;
    direct_array: [bool; 4];
}
"#,
        );

        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let facts = binder_facts(&compilation, &cancellation);

        let fields = symbols
            .struct_fields()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [
            symbolic_array,
            symbolic_application,
            target_sensitive_application,
            compound_application,
            direct_array,
        ] = fields.as_slice()
        else {
            panic!("test source must contain five source fields: {fields:?}");
        };

        let field_type = |field: &bray_symbols::StructFieldSymbol| {
            published_fact(
                &facts,
                SymbolFactRequest::<bray_symbols::StructFieldTypeFact>::new(field.id()),
            )
            .value()
            .clone()
        };

        let symbolic_length = array_length(&field_type(symbolic_array));
        let symbolic_argument = constant_argument(&field_type(symbolic_application));
        let literal_argument = constant_argument(&field_type(target_sensitive_application));
        let compound_argument = constant_argument(&field_type(compound_application));
        let direct_length = array_length(&field_type(direct_array));

        assert_constant_occurrence(
            &compilation,
            symbolic_length,
            symbolic_array.id().into(),
            SyntaxKind::Expression,
            "count",
        );

        assert_constant_occurrence(
            &compilation,
            symbolic_argument,
            symbolic_application.id().into(),
            SyntaxKind::TypeExpression,
            "count",
        );

        assert_constant_occurrence(
            &compilation,
            literal_argument,
            target_sensitive_application.id().into(),
            SyntaxKind::Expression,
            "4",
        );

        assert_constant_occurrence(
            &compilation,
            compound_argument,
            compound_application.id().into(),
            SyntaxKind::Expression,
            "count + 1",
        );

        assert_constant_occurrence(
            &compilation,
            direct_length,
            direct_array.id().into(),
            SyntaxKind::Expression,
            "4",
        );

        assert_eq!(
            symbolic_argument.expected_type(),
            literal_argument.expected_type()
        );

        assert_eq!(
            symbolic_argument.expected_type(),
            compound_argument.expected_type()
        );

        assert!(matches!(
            symbolic_argument.expected_type(),
            ConstantExpressionExpectedType::GenericParameter(_)
        ));

        assert!(matches!(
            symbolic_length.expected_type(),
            ConstantExpressionExpectedType::Resolved(_)
        ));

        assert_eq!(
            symbolic_length.expected_type(),
            direct_length.expected_type()
        );

        assert_ne!(symbolic_length.key(), direct_length.key());

        let parameter = source_id(
            symbols.generic_const_parameters(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let declared_type = published_fact(
            &facts,
            SymbolFactRequest::<GenericConstParameterDeclaredTypeFact>::new(parameter),
        );

        assert!(declared_type.diagnostics().is_empty());

        let usize_symbol = symbols
            .compiler_known_provider()
            .role_registry()
            .representation_symbol::<StructSymbolId>(
                bray_compiler_known::RepresentationRole::ScalarUsize,
            )
            .unwrap_or_else(|| panic!("compiler-known usize must be available"));

        assert!(matches!(
            type_data(&compilation, declared_type.value()).as_ref(),
            TypeData::Named {
                definition: NamedTypeSymbolId::Struct(definition),
                ..
            } if *definition == usize_symbol
        ));
    }

    #[test]
    fn source_type_valued_member_projection_retains_subject_trait_and_member_identity() {
        let compilation = compilation(
            r#"module app;

trait Provides
{
    type Item;
}

struct Subject
{
}

struct Uses
{
    projected: Subject(Provides).Item;
}

impl Subject(Provides)
{
    type Item = bool;
}
"#,
        );

        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let facts = binder_facts(&compilation, &cancellation);

        let field = source_id(
            symbols.struct_fields(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let result = published_fact(
            &facts,
            SymbolFactRequest::<bray_symbols::StructFieldTypeFact>::new(field),
        );

        assert!(result.diagnostics().is_empty());

        let projection = type_data(&compilation, result.value());

        let TypeData::TypeValuedMemberProjection {
            subject,
            application,
            member,
        } = projection.as_ref()
        else {
            panic!("qualified type-valued member must retain projection identity");
        };

        assert!(matches!(
            type_data(&compilation, *subject).as_ref(),
            TypeData::Named { .. }
        ));

        assert!(
            compilation
                .semantic_value_store()
                .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"))
                .trait_application_data(*application)
                .is_ok()
        );

        assert_eq!(
            symbols.trait_type_member(*member).map(|value| value.id()),
            Some(*member)
        );
    }

    #[test]
    fn array_length_validity_is_deferred_without_losing_source_identity() {
        let compilation = compilation(
            r#"module app;

struct Broken<const flag: bool>
{
    zero: [bool; 0];
    non_integer: [bool; flag];
}
"#,
        );

        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let facts = binder_facts(&compilation, &cancellation);

        let fields = symbols
            .struct_fields()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [zero, non_integer] = fields.as_slice() else {
            panic!("test source must contain two source fields: {fields:?}");
        };

        let zero_field = zero.id();
        let non_integer_field = non_integer.id();

        let zero = published_fact(
            &facts,
            SymbolFactRequest::<bray_symbols::StructFieldTypeFact>::new(zero_field),
        );

        let non_integer = published_fact(
            &facts,
            SymbolFactRequest::<bray_symbols::StructFieldTypeFact>::new(non_integer_field),
        );

        assert!(zero.diagnostics().is_empty());
        assert!(non_integer.diagnostics().is_empty());

        let zero_length = array_length(zero.value());
        let non_integer_length = array_length(non_integer.value());

        assert_constant_occurrence(
            &compilation,
            zero_length,
            zero_field.into(),
            SyntaxKind::Expression,
            "0",
        );

        assert_constant_occurrence(
            &compilation,
            non_integer_length,
            non_integer_field.into(),
            SyntaxKind::Expression,
            "flag",
        );

        assert_eq!(
            zero_length.expected_type(),
            non_integer_length.expected_type()
        );
    }

    #[test]
    fn constant_argument_validity_is_deferred_while_abi_diagnostics_remain_owned() {
        let compilation = compilation(
            r#"module app;

struct Fixed<const count: usize>
{
}

struct Broken
{
    value: Fixed<true>;
}

@abi(unknown)
@abi(c)
func invalid()
{
}
"#,
        );

        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let facts = binder_facts(&compilation, &cancellation);

        let field = source_id(
            symbols.struct_fields(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let field_type = published_fact(
            &facts,
            SymbolFactRequest::<bray_symbols::StructFieldTypeFact>::new(field),
        );

        let argument = constant_argument(field_type.value());

        assert_constant_occurrence(
            &compilation,
            argument,
            field.into(),
            SyntaxKind::Expression,
            "true",
        );

        assert!(matches!(
            argument.expected_type(),
            ConstantExpressionExpectedType::GenericParameter(_)
        ));

        assert!(field_type.diagnostics().is_empty());

        let function = source_id(
            symbols.functions(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let signature = published_fact(
            &facts,
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(function)),
        );

        assert!(matches!(
            type_data(&compilation, signature.value().callable_type()).as_ref(),
            TypeData::Callable(callable) if callable.abi() == CallableAbi::Bray
        ));

        bray_testing::assert_goal_state_diagnostic_kind(
            signature.diagnostics(),
            DiagnosticKind::BindingInvalidCallableAbi,
        );

        assert_eq!(
            signature
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [DiagnosticKind::BindingInvalidCallableAbi]
        );
    }

    #[test]
    fn recovered_source_types_publish_error_type_facts() {
        let compilation = compilation(
            r#"module app;

struct Broken
{
    value: ;
}
"#,
        );

        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let facts = binder_facts(&compilation, &cancellation);

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
            type_data(&compilation, field_type.value()).as_ref(),
            TypeData::Error
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

    fn array_length(template: &TypeExpressionTemplate) -> ConstantExpressionOccurrence {
        let TypeExpressionTemplate::Array { length, .. } = template else {
            panic!("array type must retain its source length occurrence");
        };

        *length
    }

    fn constant_argument(template: &TypeExpressionTemplate) -> ConstantExpressionOccurrence {
        let TypeExpressionTemplate::Named { arguments, .. } = template else {
            panic!("generic constant application must retain a named type template");
        };

        let [GenericArgumentTemplate::Constant(occurrence)] = arguments.as_ref() else {
            panic!("test generic application must retain one constant occurrence");
        };

        *occurrence
    }

    fn assert_constant_occurrence(
        compilation: &crate::Compilation,
        occurrence: ConstantExpressionOccurrence,
        owner: AnySymbolId,
        syntax_kind: SyntaxKind,
        expected_text: &str,
    ) {
        let key = occurrence.key();
        let syntax = key.syntax();

        let Some(source) = compilation.source(syntax.source_id()) else {
            panic!("constant expression source must remain loaded");
        };

        let Some(actual_text) = source.text_slice(syntax.full_range()) else {
            panic!("constant expression range must remain valid");
        };

        assert_eq!(key.owner(), owner);
        assert_eq!(syntax.syntax_kind(), syntax_kind);
        assert_eq!(actual_text, expected_text);
    }
}
