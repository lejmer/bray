use crate::compilation::binder::{
    BindingQueryResult, semantic_contract_binding_error as binding_contract,
};
use bray_binder::{BindingError, BindingQueryError, SymbolQueryProvider};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableContractTemplateQuery, CallableOverloadTemplateQuery,
    CallableParameterDefaultTemplateQuery, CallableParameterSymbolId,
    DeclarationExpressionTemplate, ExactSymbolId, GenericConstraintTemplate,
    GenericDeclarationTemplate, GenericDeclarationTemplateQuery, GenericOwnerId,
    ImplementationHeadTemplate, ImplementationHeadTemplateQuery,
    ImplementationOverloadTemplateQuery, ImplementationSubjectQuery, ImplementationSymbolId,
    ImplementedTraitApplicationQuery, OverloadArmTemplate, OverloadSignatureTemplate,
    PredicateDefinitionSymbolId, PredicateParameterTemplate, PredicateSignatureTemplate,
    PredicateSignatureTemplateQuery, StructFieldDefaultTemplateQuery, SymbolQueryRequest,
    UnevaluatedDefaultTemplate, UnionPayloadFieldDefaultTemplateQuery,
};
use bray_syntax::{
    ExpressionSyntax, PredicateDeclarationSyntax, PredicateParameterListSyntax, SourceSyntaxNode,
    SyntaxNodeView, SyntaxWalkControl, TraitPredicateMemberDeclarationSyntax, WithClauseSyntax,
    walk_direct_child_nodes,
};

use super::super::binding::CompilationSymbolQueryEvaluator;
use super::super::cache::CompilationSymbolSemantics;
use super::super::environment::{generic_parameter_ids, type_binder};
use super::super::surface::{declaration_callable_surface, symbol_ordinal, with_declaration_root};
use super::contract::bind_callable_contract_template;
use crate::compilation::binder::CompilationBindingContext;
use crate::compilation::binder::symbol::imported::{
    imported_callable_parameter_default, imported_generic_declaration, missing_imported_template,
};
use crate::fact::SymbolQueryCache;

impl CompilationSymbolQueryEvaluator<GenericDeclarationTemplateQuery>
    for CompilationSymbolSemantics
{
    fn cache(&self) -> &SymbolQueryCache<GenericDeclarationTemplateQuery> {
        &self.generic_declaration_templates
    }

    fn bind(
        &self,
        context: &CompilationBindingContext<'_>,
        request: SymbolQueryRequest<GenericDeclarationTemplateQuery>,
    ) -> BindingQueryResult<
        DiagnosticResult<
            <GenericDeclarationTemplateQuery as bray_symbols::SymbolQueryContract>::Value,
        >,
    > {
        bind_generic_declaration_template(context, request.owner())
    }
}

impl CompilationSymbolQueryEvaluator<CallableContractTemplateQuery> for CompilationSymbolSemantics {
    fn cache(&self) -> &SymbolQueryCache<CallableContractTemplateQuery> {
        &self.callable_contract_templates
    }

    fn bind(
        &self,
        context: &CompilationBindingContext<'_>,
        request: SymbolQueryRequest<CallableContractTemplateQuery>,
    ) -> BindingQueryResult<
        DiagnosticResult<
            <CallableContractTemplateQuery as bray_symbols::SymbolQueryContract>::Value,
        >,
    > {
        bind_callable_contract_template(context, request.owner())
    }
}

impl CompilationSymbolQueryEvaluator<PredicateSignatureTemplateQuery>
    for CompilationSymbolSemantics
{
    fn cache(&self) -> &SymbolQueryCache<PredicateSignatureTemplateQuery> {
        &self.predicate_signature_templates
    }

    fn bind(
        &self,
        context: &CompilationBindingContext<'_>,
        request: SymbolQueryRequest<PredicateSignatureTemplateQuery>,
    ) -> BindingQueryResult<
        DiagnosticResult<
            <PredicateSignatureTemplateQuery as bray_symbols::SymbolQueryContract>::Value,
        >,
    > {
        bind_predicate_signature_template(context, request.owner())
    }
}

impl CompilationSymbolQueryEvaluator<ImplementationHeadTemplateQuery>
    for CompilationSymbolSemantics
{
    fn cache(&self) -> &SymbolQueryCache<ImplementationHeadTemplateQuery> {
        &self.implementation_head_templates
    }

    fn bind(
        &self,
        context: &CompilationBindingContext<'_>,
        request: SymbolQueryRequest<ImplementationHeadTemplateQuery>,
    ) -> BindingQueryResult<
        DiagnosticResult<
            <ImplementationHeadTemplateQuery as bray_symbols::SymbolQueryContract>::Value,
        >,
    > {
        bind_implementation_head_template(context, request.owner())
    }
}

macro_rules! impl_default_template_binding {
    ($contract:ty, $field:ident) => {
        impl CompilationSymbolQueryEvaluator<$contract> for CompilationSymbolSemantics {
            fn cache(&self) -> &SymbolQueryCache<$contract> {
                &self.$field
            }

            fn bind(
                &self,
                context: &CompilationBindingContext<'_>,
                request: SymbolQueryRequest<$contract>,
            ) -> BindingQueryResult<
                bray_diagnostics::DiagnosticResult<
                    <$contract as bray_symbols::SymbolQueryContract>::Value,
                >,
            > {
                bind_default_template(context, request.symbol())
            }
        }
    };
}

impl_default_template_binding!(
    CallableParameterDefaultTemplateQuery,
    callable_parameter_default_templates
);
impl_default_template_binding!(
    StructFieldDefaultTemplateQuery,
    struct_field_default_templates
);
impl_default_template_binding!(
    UnionPayloadFieldDefaultTemplateQuery,
    union_payload_field_default_templates
);

macro_rules! impl_overload_template_binding {
    ($contract:ty, $cache:ident, $accessor:ident) => {
        impl CompilationSymbolQueryEvaluator<$contract> for CompilationSymbolSemantics {
            fn cache(&self) -> &SymbolQueryCache<$contract> {
                &self.$cache
            }

            fn bind(
                &self,
                context: &CompilationBindingContext<'_>,
                request: SymbolQueryRequest<$contract>,
            ) -> BindingQueryResult<
                bray_diagnostics::DiagnosticResult<
                    <$contract as bray_symbols::SymbolQueryContract>::Value,
                >,
            > {
                let symbol = if let Some(symbol) = context.symbols.$accessor(request.owner()) {
                    symbol
                } else {
                    context
                        .imported_symbols()?
                        .and_then(|symbols| symbols.$accessor(request.owner()))
                        .ok_or(BindingQueryError::Binding(
                            BindingError::SymbolRecordUnavailable(request.symbol()),
                        ))?
                };

                Ok(DiagnosticResult::without_diagnostics(overload_template(
                    request.symbol(),
                    symbol.arm_syntax(),
                    symbol.arms(),
                )))
            }
        }
    };
}

impl_overload_template_binding!(
    CallableOverloadTemplateQuery,
    callable_overload_templates,
    callable_overload
);
impl_overload_template_binding!(
    ImplementationOverloadTemplateQuery,
    implementation_overload_templates,
    implementation_overload
);

fn bind_generic_declaration_template(
    context: &CompilationBindingContext<'_>,
    owner: GenericOwnerId,
) -> BindingQueryResult<DiagnosticResult<GenericDeclarationTemplate>> {
    let symbol = owner.symbol();

    if let Some(address) = context.imported_semantic_address(symbol)? {
        return imported_generic_declaration(context, owner, address);
    }

    let parameters = generic_parameter_ids(context.symbols, symbol)?;

    with_declaration_root(context, symbol, |root| {
        let mut constraints = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        for clause in direct_children::<WithClauseSyntax>(&root)? {
            let unit = SyntaxAnchor::from_node(&clause);

            for expression in clause.expressions() {
                let ordinal = symbol_ordinal(constraints.len())?;

                let constraint = if let Some(satisfaction) =
                    expression.trait_satisfaction_constraint()
                {
                    let subject = type_binder(context, symbol)?
                        .bind_type_expression(satisfaction.subject())?;

                    let application = type_binder(context, symbol)?
                        .bind_trait_application(satisfaction.application())?;

                    diagnostics = diagnostics.merged(subject.diagnostics());
                    diagnostics = diagnostics.merged(application.diagnostics());

                    // The declaration template owns the Arc-backed checked type templates.
                    GenericConstraintTemplate::trait_satisfaction(
                        ordinal,
                        unit,
                        subject.value().clone(),
                        application.value().clone(),
                    )
                } else if let Some(equality) = expression.type_equality_constraint() {
                    match (
                        type_binder(context, symbol)?.bind_static_type_operand(equality.left())?,
                        type_binder(context, symbol)?.bind_static_type_operand(equality.right())?,
                    ) {
                        (Some(left), Some(right)) => {
                            diagnostics = diagnostics.merged(left.diagnostics());
                            diagnostics = diagnostics.merged(right.diagnostics());

                            GenericConstraintTemplate::type_equality(
                                ordinal,
                                unit,
                                left.value().clone(),
                                right.value().clone(),
                            )
                        }
                        _ => source_constraint(symbol, ordinal, unit, &expression),
                    }
                } else {
                    source_constraint(symbol, ordinal, unit, &expression)
                };

                constraints.push(constraint);
            }
        }

        Ok(DiagnosticResult::new(
            GenericDeclarationTemplate::new(owner, parameters, constraints),
            diagnostics,
        ))
    })
}

fn source_constraint(
    owner: AnySymbolId,
    ordinal: bray_symbols::SymbolOrdinal,
    unit: SyntaxAnchor,
    expression: &ExpressionSyntax,
) -> GenericConstraintTemplate {
    GenericConstraintTemplate::new(
        ordinal,
        unit,
        DeclarationExpressionTemplate::new(owner, SyntaxAnchor::from_node(expression)),
    )
}

fn bind_predicate_signature_template(
    context: &CompilationBindingContext<'_>,
    owner: PredicateDefinitionSymbolId,
) -> BindingQueryResult<DiagnosticResult<PredicateSignatureTemplate>> {
    let symbol = owner.into_any();

    let parameters = context
        .symbols
        .predicate_definition_parameters(owner)
        .ok_or(BindingQueryError::Binding(
            BindingError::SymbolRecordUnavailable(symbol),
        ))?;

    with_declaration_root(context, symbol, |root| {
        let list = direct_children::<PredicateParameterListSyntax>(&root)?
            .into_iter()
            .next()
            .ok_or_else(|| syntax_contract(SyntaxAnchor::from_node(&root)))?;

        let syntax = list.predicate_parameters().collect::<Vec<_>>();

        if syntax.len() != parameters.len() {
            return Err(binding_contract(
                crate::compilation::SemanticQueryContext::Symbol(symbol),
                crate::compilation::SemanticQueryViolation::CountMismatch {
                    data: crate::compilation::SemanticDataKind::CallableSignature,
                    expected: parameters.len(),
                    actual: syntax.len(),
                },
            ));
        }

        let types = syntax
            .iter()
            .map(|syntax| syntax.type_expression())
            .collect::<Vec<_>>();

        let result = type_binder(context, symbol)?.bind_type_expressions(types.iter())?;

        let (types, diagnostics) = result.into_parts();

        let entries = parameters
            .iter()
            .copied()
            .zip(syntax)
            .zip(types)
            .map(|((parameter, syntax), ty)| {
                let name = syntax
                    .identifier_token()
                    .text(syntax.source().text())
                    .and_then(bray_symbols::CallableParameterName::try_new)
                    .ok_or_else(|| syntax_contract(SyntaxAnchor::from_node(&syntax)))?;

                Ok(PredicateParameterTemplate::new(parameter, name, ty))
            })
            .collect::<BindingQueryResult<Vec<_>>>()?;

        let is_trusted = predicate_is_trusted(&root, owner)?;

        Ok(DiagnosticResult::new(
            PredicateSignatureTemplate::new(owner, entries, is_trusted),
            diagnostics,
        ))
    })
}

fn predicate_is_trusted(
    root: &SyntaxNodeView<'_>,
    owner: PredicateDefinitionSymbolId,
) -> BindingQueryResult<bool> {
    match owner {
        PredicateDefinitionSymbolId::Predicate(_)
        | PredicateDefinitionSymbolId::TraitFulfillment(_) => root
            .cast::<PredicateDeclarationSyntax>()
            .map(|syntax| syntax.predicate_modifiers().trusted_token().is_some()),
        PredicateDefinitionSymbolId::TraitMember(_) => root
            .cast::<TraitPredicateMemberDeclarationSyntax>()
            .map(|syntax| {
                syntax
                    .trait_predicate_member_modifiers()
                    .trusted_token()
                    .is_some()
            }),
    }
    .ok_or_else(|| syntax_contract(SyntaxAnchor::from_node(root)))
}

fn bind_implementation_head_template(
    context: &CompilationBindingContext<'_>,
    implementation: ImplementationSymbolId,
) -> BindingQueryResult<DiagnosticResult<ImplementationHeadTemplate>> {
    let generic = context.resolve_symbol_query(SymbolQueryRequest::<
        GenericDeclarationTemplateQuery,
    >::new(
        GenericOwnerId::try_new(implementation.into_any()).ok_or(BindingQueryError::Binding(
            BindingError::GenericOwnerUnavailable(implementation.into_any()),
        ))?,
    ))?;

    let subject = context.resolve_symbol_query(
        SymbolQueryRequest::<ImplementationSubjectQuery>::new(implementation),
    )?;

    let trait_application = context.resolve_symbol_query(SymbolQueryRequest::<
        ImplementedTraitApplicationQuery,
    >::new(implementation))?;

    let diagnostics = generic
        .diagnostics()
        .merged(subject.diagnostics())
        .merged(trait_application.diagnostics());

    // The composed head shares immutable values owned by its dependency binding_context.
    Ok(DiagnosticResult::new(
        ImplementationHeadTemplate::new(
            implementation,
            generic.value().clone(),
            subject.value().clone(),
            trait_application.value().clone(),
        ),
        diagnostics,
    ))
}

fn bind_default_template(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
) -> BindingQueryResult<DiagnosticResult<UnevaluatedDefaultTemplate>> {
    if let Some(parameter) = CallableParameterSymbolId::try_from_any(owner) {
        if let Some(address) = context.imported_semantic_address(owner)? {
            return imported_callable_parameter_default(context, address);
        }

        if matches!(
            context.symbols.symbol_origin(owner),
            Some(
                bray_symbols::SymbolOrigin::CompilerKnown
                    | bray_symbols::SymbolOrigin::CompilerProvided
            )
        ) {
            return bind_compiler_known_callable_parameter_default(context, parameter);
        }
    }

    if context.imported_semantic_address(owner)?.is_some()
        && matches!(
            owner,
            AnySymbolId::StructField(_) | AnySymbolId::UnionPayloadField(_)
        )
    {
        let provider = super::super::declaration_body::runtime_default_provider(context, owner)?;

        let Some(provider) = provider else {
            return Ok(DiagnosticResult::without_diagnostics(
                UnevaluatedDefaultTemplate::Absent,
            ));
        };

        let address =
            context
                .imported_semantic_address(provider)?
                .ok_or(BindingQueryError::Binding(
                    BindingError::SymbolRecordUnavailable(provider),
                ))?;

        let imported = super::super::imported::imported_declaration_template(
            context,
            address,
            bray_bound_tree::CheckedTemplateKind::RuntimeDefault,
        )?;

        if imported.value().is_none() {
            return Err(missing_imported_template(
                address,
                bray_package_interface::InterfaceSemanticRecordKind::DeclarationTemplate,
            ));
        }

        return Ok(DiagnosticResult::new(
            UnevaluatedDefaultTemplate::Resolved,
            imported.diagnostics().clone(),
        ));
    }

    with_declaration_root(context, owner, |root| {
        let default = direct_children::<ExpressionSyntax>(&root)?
            .into_iter()
            .next()
            .map(|expression| {
                UnevaluatedDefaultTemplate::Present(DeclarationExpressionTemplate::new(
                    owner,
                    SyntaxAnchor::from_node(&expression),
                ))
            })
            .unwrap_or(UnevaluatedDefaultTemplate::Absent);

        Ok(DiagnosticResult::without_diagnostics(default))
    })
}

fn bind_compiler_known_callable_parameter_default(
    context: &CompilationBindingContext<'_>,
    parameter: CallableParameterSymbolId,
) -> BindingQueryResult<DiagnosticResult<UnevaluatedDefaultTemplate>> {
    let parameter_symbol =
        context
            .symbols
            .callable_parameter(parameter)
            .ok_or(BindingQueryError::Binding(
                BindingError::SymbolRecordUnavailable(parameter.into()),
            ))?;

    let surface = declaration_callable_surface(context, parameter_symbol.owner().into_any())?;

    let parameter = surface
        .parameters
        .parameters()
        .nth(parameter_symbol.ordinal() as usize)
        .ok_or_else(|| {
            binding_contract(
                crate::compilation::SemanticQueryContext::Symbol(parameter.into()),
                crate::compilation::SemanticQueryViolation::CountMismatch {
                    data: crate::compilation::SemanticDataKind::CallableSignature,
                    expected: parameter_symbol.ordinal() as usize + 1,
                    actual: surface.parameters.parameters().count(),
                },
            )
        })?;

    let default = parameter
        .expression()
        .map(|expression| {
            UnevaluatedDefaultTemplate::Present(DeclarationExpressionTemplate::new(
                parameter_symbol.id().into(),
                SyntaxAnchor::from_node(&expression),
            ))
        })
        .unwrap_or(UnevaluatedDefaultTemplate::Absent);

    Ok(DiagnosticResult::without_diagnostics(default))
}

fn overload_template(
    owner: AnySymbolId,
    source_arms: &[SyntaxAnchor],
    resolved_arms: &[AnySymbolId],
) -> OverloadSignatureTemplate {
    let arms = source_arms
        .iter()
        .copied()
        .map(OverloadArmTemplate::Source)
        .chain(
            resolved_arms
                .iter()
                .copied()
                .map(OverloadArmTemplate::Resolved),
        );

    OverloadSignatureTemplate::new(owner, arms)
}

fn direct_children<T>(root: &SyntaxNodeView<'_>) -> BindingQueryResult<Vec<T>>
where
    T: bray_syntax::SyntaxCast,
{
    let mut children = Vec::new();
    let mut cast_failed = false;

    walk_direct_child_nodes(root, |node| {
        if node.kind() == T::KIND {
            match node.cast::<T>() {
                Some(child) => children.push(child),
                None => {
                    cast_failed = true;

                    return SyntaxWalkControl::Stop;
                }
            }
        }

        SyntaxWalkControl::Continue
    });

    if cast_failed {
        return Err(syntax_contract(SyntaxAnchor::from_node(root)));
    }

    Ok(children)
}

fn syntax_contract(source: SyntaxAnchor) -> BindingQueryError<crate::fact::FactQueryError> {
    BindingQueryError::Binding(BindingError::SyntaxContract(source))
}

#[cfg(test)]
mod tests {
    use bray_symbols::CallableConditions;
    use std::sync::Arc;

    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceValidationPolicy,
        test_support::encoded_semantic_test_interface,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::{
        CallableContractTemplate, CallableContractTemplateQuery, CallableOverloadTemplateQuery,
        CallableParameterDefaultTemplateQuery, CallableSymbolId, GenericDeclarationTemplateQuery,
        GenericOwnerId, ImplementationHeadTemplateQuery, ImplementationSymbolId, PackageIdentity,
        PredicateDefinitionSymbolId, PredicateSignatureTemplateQuery, SymbolOrigin,
        SymbolQueryRequest, TypeExpressionTemplate, UnevaluatedDefaultTemplate,
    };

    use crate::compilation::binder::symbol::test_support::{
        binding_context, resolved_query, source_id, symbol_graph,
    };
    use crate::fact::CancellationToken;
    use crate::test_support::compilation;
    use crate::{Compilation, CompilationRequest, DependencyInterfaceInput};

    const DECLARATION_TEMPLATES: &str = r#"module app;

predicate valid<T>(left: T, right: T) = true;

func choose<T, const count: usize>(value: T = true) -> T
    requires(true)
    ensures(true)
    with(count > 0)
    uses(memory)
{
    return value;
}

struct Recursive<T>
    with(true)
{
    next: Recursive<T> ?;
}

impl Recursive<bool>
    with(true)
{
}

func fast()
{
}

overload choose_any = {fast}
"#;

    #[test]
    fn source_templates_publish_without_checking_declaration_expressions() {
        let compilation = compilation(DECLARATION_TEMPLATES);

        assert!(
            compilation.syntax_tree_result().diagnostics().is_empty(),
            "test source must parse without recovery: {:?}",
            compilation.syntax_tree_result().diagnostics()
        );

        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let binding_context = binding_context(&compilation, &cancellation);

        let function = source_id(
            symbols.functions(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let Some(owner) = GenericOwnerId::try_new(function.into()) else {
            panic!("function must be a generic declaration owner");
        };

        let generic = resolved_query(
            &binding_context,
            SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(owner),
        );

        assert_eq!(generic.value().parameters().len(), 2);
        assert_eq!(generic.value().constraints().len(), 1);

        assert_eq!(
            expression_text(&compilation, generic.value().constraints()[0].expression()),
            "count > 0"
        );

        let contract = resolved_query(
            &binding_context,
            SymbolQueryRequest::<CallableContractTemplateQuery>::new(CallableSymbolId::from(
                function,
            )),
        );

        let CallableContractTemplate::Source(contract) = contract.value() else {
            panic!("source callable must publish source-backed contracts");
        };

        assert_eq!(contract.expressions().len(), 3);
        assert_eq!(contract.capabilities().len(), 1);

        let Some(function) = symbols.function(function) else {
            panic!("source function must remain in the graph");
        };

        let [parameter] = function.parameters() else {
            panic!("source function must retain one parameter");
        };

        let default = resolved_query(
            &binding_context,
            SymbolQueryRequest::<CallableParameterDefaultTemplateQuery>::new(*parameter),
        );

        let UnevaluatedDefaultTemplate::Present(default) = default.value() else {
            panic!("declared parameter default must remain unevaluated");
        };

        assert_eq!(expression_text(&compilation, Some(*default)), "true");
    }

    #[test]
    fn predicate_implementation_and_overload_templates_use_symbol_identities() {
        let compilation = compilation(DECLARATION_TEMPLATES);
        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let binding_context = binding_context(&compilation, &cancellation);

        let predicate = source_id(
            symbols.predicates(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let predicate = resolved_query(
            &binding_context,
            SymbolQueryRequest::<PredicateSignatureTemplateQuery>::new(
                PredicateDefinitionSymbolId::from(predicate),
            ),
        );

        assert_eq!(predicate.value().parameters().len(), 2);

        assert!(matches!(
            predicate.value().parameters()[0].ty(),
            TypeExpressionTemplate::Resolved(_) | TypeExpressionTemplate::Named { .. }
        ));

        let implementation = source_id(
            symbols.inherent_implementations(),
            |symbol| symbol.origin(),
            |symbol| ImplementationSymbolId::from(symbol.id()),
        );

        let head = resolved_query(
            &binding_context,
            SymbolQueryRequest::<ImplementationHeadTemplateQuery>::new(implementation),
        );

        assert_eq!(head.value().implementation(), implementation);
        assert!(head.value().generic().parameters().is_empty());
        assert_eq!(head.value().generic().constraints().len(), 1);
        assert!(head.value().trait_application().is_none());

        let overload = source_id(
            symbols.callable_overloads(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let first = resolved_query(
            &binding_context,
            SymbolQueryRequest::<CallableOverloadTemplateQuery>::new(overload),
        );

        let second = resolved_query(
            &binding_context,
            SymbolQueryRequest::<CallableOverloadTemplateQuery>::new(overload),
        );

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.value().arms().len(), 1);
    }

    #[test]
    fn predicate_trust_comes_only_from_the_declaration_modifier() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "predicate plain() = trusted true;\n",
            "trusted predicate opaque();\n",
        ));

        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let binding_context = binding_context(&compilation, &cancellation);

        let predicates = symbols
            .predicates()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|symbol| symbol.id())
            .collect::<Vec<_>>();

        let [plain, opaque] = predicates.as_slice() else {
            panic!("test source must publish two predicates");
        };

        let plain = resolved_query(
            &binding_context,
            SymbolQueryRequest::<PredicateSignatureTemplateQuery>::new(
                PredicateDefinitionSymbolId::from(*plain),
            ),
        );

        let opaque = resolved_query(
            &binding_context,
            SymbolQueryRequest::<PredicateSignatureTemplateQuery>::new(
                PredicateDefinitionSymbolId::from(*opaque),
            ),
        );

        assert!(!plain.value().is_trusted());
        assert!(opaque.value().is_trusted());
    }

    #[test]
    fn mutually_referential_contract_templates_do_not_force_expression_binding() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "func first()\n",
            "    with(second())\n",
            "{\n",
            "}\n",
            "\n",
            "func second()\n",
            "    with(first())\n",
            "{\n",
            "}\n",
        ));

        let symbols = symbol_graph(&compilation);
        let cancellation = CancellationToken::new();
        let binding_context = binding_context(&compilation, &cancellation);

        let functions = symbols
            .functions()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|symbol| symbol.id())
            .collect::<Vec<_>>();

        assert_eq!(functions.len(), 2);

        for function in functions {
            let contract = resolved_query(
                &binding_context,
                SymbolQueryRequest::<CallableContractTemplateQuery>::new(CallableSymbolId::from(
                    function,
                )),
            );

            let CallableContractTemplate::Source(contract) = contract.value() else {
                panic!("source callable must publish source-backed contracts");
            };

            assert_eq!(contract.expressions().len(), 1);
        }
    }

    #[test]
    fn imported_constraints_and_contracts_publish_resolved_templates() {
        let fixture = encoded_semantic_test_interface();

        let dependency = DependencyInterfaceInput::new(
            fixture.package.clone(),
            fixture.product,
            "template-dependency.brayi",
            Arc::<[u8]>::from(fixture.bytes),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        );

        let request = CompilationRequest::new(
            package("example.current"),
            vec![SourceInput::virtual_text(
                SourceIdentity::new(0),
                "main.bray",
                SourceVersion::new(0),
                "module example.current;",
            )],
        )
        .with_dependency_interfaces([dependency]);

        let compilation = Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

        let cancellation = CancellationToken::new();
        let binding_context = binding_context(&compilation, &cancellation);

        let imported = compilation
            .imported_symbol_skeleton_result()
            .unwrap_or_else(|error| panic!("imported symbols must load: {error:?}"));

        let Some(symbols) = imported.value().as_deref() else {
            panic!("test dependency must publish imported symbols");
        };

        let Some(function) = symbols
            .functions()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Imported)
            .map(|symbol| symbol.id())
        else {
            panic!("test dependency must publish an imported function");
        };

        let Some(generic_owner) = GenericOwnerId::try_new(function.into()) else {
            panic!("imported function must be a generic declaration owner");
        };

        let generic = resolved_query(
            &binding_context,
            SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(generic_owner),
        );

        assert_eq!(generic.value().parameters().len(), 2);
        assert_eq!(generic.value().constraints().len(), 1);
        assert!(generic.value().constraints()[0].resolved().is_some());

        let contract = resolved_query(
            &binding_context,
            SymbolQueryRequest::<CallableContractTemplateQuery>::new(CallableSymbolId::from(
                function,
            )),
        );

        assert!(matches!(
            contract.value(),
            CallableContractTemplate::Resolved(contract)
                if contract.conditions().invocation_preconditions().len() == 1
        ));
    }

    fn expression_text(
        compilation: &Compilation,
        expression: Option<bray_symbols::DeclarationExpressionTemplate>,
    ) -> &str {
        let Some(expression) = expression else {
            panic!("source template must retain an expression");
        };

        let syntax = expression.syntax();

        let Some(source) = compilation.source(syntax.source_id()) else {
            panic!("template source must remain loaded");
        };

        let Some(text) = source.text_slice(syntax.full_range()) else {
            panic!("template range must remain valid");
        };

        text
    }

    fn package(value: &str) -> PackageIdentity {
        PackageIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test package identity must be valid"))
    }
}
