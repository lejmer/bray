use bray_binder::{BinderFactError, BinderFactResult, SymbolFactProvider};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_package_interface::{ImportedSemanticFact, InterfaceSemanticFactKind};
use bray_symbols::{
    AnySymbolId, CallableContractExpressionTemplate, CallableContractTemplate,
    CallableContractTemplateFact, CallableOverloadTemplateFact,
    CallableParameterDefaultTemplateFact, CallableSymbolId, DeclarationCapabilityTemplate,
    DeclarationExpressionTemplate, DeclarationPredicateClauseKind, GenericConstraintTemplate,
    GenericDeclarationTemplate, GenericDeclarationTemplateFact, GenericOwnerId,
    ImplementationHeadTemplate, ImplementationHeadTemplateFact, ImplementationOverloadTemplateFact,
    ImplementationSubjectFact, ImplementationSymbolId, ImplementedTraitApplicationFact,
    ImportedSymbolFactAddress, OverloadArmTemplate, OverloadSignatureTemplate,
    PredicateDefinitionSymbolId, PredicateParameterSymbolId, PredicateParameterTemplate,
    PredicateSignatureTemplate, PredicateSignatureTemplateFact, StructFieldDefaultTemplateFact,
    SymbolFactRequest, SymbolFactResult, SymbolGraph, UnevaluatedDefaultTemplate,
    UnionPayloadFieldDefaultTemplateFact,
};
use bray_syntax::{
    ExpressionSyntax, PredicateParameterListSyntax, SyntaxKind, SyntaxNodeView, SyntaxWalkControl,
    UsesClauseSyntax, WithClauseSyntax, walk_direct_child_nodes,
};

use super::cache::CompilationSymbolFacts;
use super::compute::CompilationSymbolFactBinding;
use super::environment::{generic_parameter_ids, type_binder};
use super::surface::{symbol_ordinal, with_declaration_root};
use crate::compilation::binder::CompilationBinderFacts;
use crate::fact::{ImportedSemanticFactKey, SymbolFactCache};

impl CompilationSymbolFactBinding<GenericDeclarationTemplateFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<GenericDeclarationTemplateFact> {
        &self.generic_declaration_templates
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<GenericDeclarationTemplateFact>,
    ) -> BinderFactResult<SymbolFactResult<GenericDeclarationTemplateFact>> {
        bind_generic_declaration_template(context, request.owner())
    }
}

impl CompilationSymbolFactBinding<CallableContractTemplateFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<CallableContractTemplateFact> {
        &self.callable_contract_templates
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<CallableContractTemplateFact>,
    ) -> BinderFactResult<SymbolFactResult<CallableContractTemplateFact>> {
        bind_callable_contract_template(context, request.owner())
    }
}

impl CompilationSymbolFactBinding<PredicateSignatureTemplateFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<PredicateSignatureTemplateFact> {
        &self.predicate_signature_templates
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<PredicateSignatureTemplateFact>,
    ) -> BinderFactResult<SymbolFactResult<PredicateSignatureTemplateFact>> {
        bind_predicate_signature_template(context, request.owner())
    }
}

impl CompilationSymbolFactBinding<ImplementationHeadTemplateFact> for CompilationSymbolFacts {
    fn cache(&self) -> &SymbolFactCache<ImplementationHeadTemplateFact> {
        &self.implementation_head_templates
    }

    fn bind(
        &self,
        context: &CompilationBinderFacts<'_>,
        request: SymbolFactRequest<ImplementationHeadTemplateFact>,
    ) -> BinderFactResult<SymbolFactResult<ImplementationHeadTemplateFact>> {
        bind_implementation_head_template(context, request.owner())
    }
}

macro_rules! impl_default_template_binding {
    ($contract:ty, $field:ident) => {
        impl CompilationSymbolFactBinding<$contract> for CompilationSymbolFacts {
            fn cache(&self) -> &SymbolFactCache<$contract> {
                &self.$field
            }

            fn bind(
                &self,
                context: &CompilationBinderFacts<'_>,
                request: SymbolFactRequest<$contract>,
            ) -> BinderFactResult<SymbolFactResult<$contract>> {
                bind_default_template(context, request.symbol())
            }
        }
    };
}

impl_default_template_binding!(
    CallableParameterDefaultTemplateFact,
    callable_parameter_default_templates
);
impl_default_template_binding!(
    StructFieldDefaultTemplateFact,
    struct_field_default_templates
);
impl_default_template_binding!(
    UnionPayloadFieldDefaultTemplateFact,
    union_payload_field_default_templates
);

macro_rules! impl_overload_template_binding {
    ($contract:ty, $cache:ident, $accessor:ident) => {
        impl CompilationSymbolFactBinding<$contract> for CompilationSymbolFacts {
            fn cache(&self) -> &SymbolFactCache<$contract> {
                &self.$cache
            }

            fn bind(
                &self,
                context: &CompilationBinderFacts<'_>,
                request: SymbolFactRequest<$contract>,
            ) -> BinderFactResult<SymbolFactResult<$contract>> {
                let symbol = if let Some(symbol) = context.symbols.$accessor(request.owner()) {
                    symbol
                } else {
                    context
                        .imported_symbols()?
                        .and_then(|symbols| symbols.$accessor(request.owner()))
                        .ok_or(BinderFactError::DependencyUnavailable)?
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
    CallableOverloadTemplateFact,
    callable_overload_templates,
    callable_overload
);
impl_overload_template_binding!(
    ImplementationOverloadTemplateFact,
    implementation_overload_templates,
    implementation_overload
);

fn bind_generic_declaration_template(
    context: &CompilationBinderFacts<'_>,
    owner: GenericOwnerId,
) -> BinderFactResult<DiagnosticResult<GenericDeclarationTemplate>> {
    let symbol = owner.symbol();

    if let Some(address) = context.imported_fact_address(symbol)? {
        let symbols = context
            .imported_symbols()?
            .ok_or(BinderFactError::DependencyUnavailable)?;

        let parameters = generic_parameter_ids(symbols, symbol)?;

        let (constraints, diagnostics) = imported_constraints(context, address)?;

        return Ok(DiagnosticResult::new(
            GenericDeclarationTemplate::new(owner, parameters, constraints),
            diagnostics,
        ));
    }

    let parameters = generic_parameter_ids(context.symbols, symbol)?;

    with_declaration_root(context, symbol, |root| {
        let mut constraints = Vec::new();

        for clause in direct_children::<WithClauseSyntax>(&root)? {
            for expression in clause.expressions() {
                constraints.push(GenericConstraintTemplate::new(
                    symbol_ordinal(constraints.len())?,
                    DeclarationExpressionTemplate::new(
                        symbol,
                        SyntaxAnchor::from_node(&expression),
                    ),
                ));
            }
        }

        Ok(DiagnosticResult::without_diagnostics(
            GenericDeclarationTemplate::new(owner, parameters, constraints),
        ))
    })
}

fn bind_callable_contract_template(
    context: &CompilationBinderFacts<'_>,
    owner: CallableSymbolId,
) -> BinderFactResult<DiagnosticResult<CallableContractTemplate>> {
    let symbol = owner.into_any();

    if let Some(address) = context.imported_fact_address(symbol)? {
        return imported_callable_contract(context, address);
    }

    with_declaration_root(context, symbol, |root| {
        let mut expressions = Vec::new();
        let mut capabilities = Vec::new();

        let mut traversal_error = None;

        walk_direct_child_nodes(&root, |node| {
            if let Err(error) =
                push_callable_contract_child(symbol, &node, &mut expressions, &mut capabilities)
            {
                traversal_error = Some(error);

                return SyntaxWalkControl::Stop;
            }

            SyntaxWalkControl::Continue
        });

        if let Some(error) = traversal_error {
            return Err(error);
        }

        Ok(DiagnosticResult::without_diagnostics(
            CallableContractTemplate::source(owner, expressions, capabilities),
        ))
    })
}

fn push_callable_contract_child(
    owner: AnySymbolId,
    node: &SyntaxNodeView<'_>,
    expressions: &mut Vec<CallableContractExpressionTemplate>,
    capabilities: &mut Vec<DeclarationCapabilityTemplate>,
) -> BinderFactResult<()> {
    match node.kind() {
        SyntaxKind::RequiresClause => push_contract_expressions(
            owner,
            DeclarationPredicateClauseKind::Requires,
            node,
            expressions,
        ),
        SyntaxKind::EnsuresClause => push_contract_expressions(
            owner,
            DeclarationPredicateClauseKind::Ensures,
            node,
            expressions,
        ),
        SyntaxKind::WithClause => push_contract_expressions(
            owner,
            DeclarationPredicateClauseKind::Static,
            node,
            expressions,
        ),
        SyntaxKind::UsesClause => push_capabilities(node, capabilities),
        _ => Ok(()),
    }
}

fn push_capabilities(
    node: &SyntaxNodeView<'_>,
    output: &mut Vec<DeclarationCapabilityTemplate>,
) -> BinderFactResult<()> {
    let Some(clause) = node.cast::<UsesClauseSyntax>() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    for path in clause.paths() {
        output.push(DeclarationCapabilityTemplate::new(
            symbol_ordinal(output.len())?,
            SyntaxAnchor::from_node(&path),
        ));
    }

    Ok(())
}

fn push_contract_expressions(
    owner: AnySymbolId,
    kind: DeclarationPredicateClauseKind,
    node: &SyntaxNodeView<'_>,
    output: &mut Vec<CallableContractExpressionTemplate>,
) -> BinderFactResult<()> {
    let expressions = direct_children::<ExpressionSyntax>(node)?;

    for expression in expressions {
        let ordinal = symbol_ordinal(output.len())?;

        output.push(CallableContractExpressionTemplate::new(
            ordinal,
            kind,
            DeclarationExpressionTemplate::new(owner, SyntaxAnchor::from_node(&expression)),
        ));
    }

    Ok(())
}

fn bind_predicate_signature_template(
    context: &CompilationBinderFacts<'_>,
    owner: PredicateDefinitionSymbolId,
) -> BinderFactResult<DiagnosticResult<PredicateSignatureTemplate>> {
    let symbol = owner.into_any();
    let parameters = predicate_parameters(context.symbols, owner)?;

    with_declaration_root(context, symbol, |root| {
        let list = direct_children::<PredicateParameterListSyntax>(&root)?
            .into_iter()
            .next()
            .ok_or(BinderFactError::DependencyUnavailable)?;

        let syntax = list.predicate_parameters().collect::<Vec<_>>();

        if syntax.len() != parameters.len() {
            return Err(BinderFactError::DependencyUnavailable);
        }

        let mut entries = Vec::with_capacity(parameters.len());
        let mut diagnostics = DiagnosticBag::new();

        for (parameter, syntax) in parameters.iter().copied().zip(syntax) {
            let result =
                type_binder(context, symbol)?.bind_type_expression(&syntax.type_expression())?;

            let (ty, parameter_diagnostics) = result.into_parts();

            diagnostics = diagnostics.merged(&parameter_diagnostics);
            entries.push(PredicateParameterTemplate::new(parameter, ty));
        }

        let is_trusted = root
            .tokens()
            .any(|token| token.kind() == SyntaxKind::TrustedKeyword);

        Ok(DiagnosticResult::new(
            PredicateSignatureTemplate::new(owner, entries, is_trusted),
            diagnostics,
        ))
    })
}

fn bind_implementation_head_template(
    context: &CompilationBinderFacts<'_>,
    implementation: ImplementationSymbolId,
) -> BinderFactResult<DiagnosticResult<ImplementationHeadTemplate>> {
    let generic = context.symbol_fact(SymbolFactRequest::<GenericDeclarationTemplateFact>::new(
        GenericOwnerId::try_new(implementation.into_any())
            .ok_or(BinderFactError::DependencyUnavailable)?,
    ))?;

    let subject = context.symbol_fact(SymbolFactRequest::<ImplementationSubjectFact>::new(
        implementation,
    ))?;

    let trait_application = context.symbol_fact(SymbolFactRequest::<
        ImplementedTraitApplicationFact,
    >::new(implementation))?;

    let diagnostics = generic
        .diagnostics()
        .merged(subject.diagnostics())
        .merged(trait_application.diagnostics());

    // The composed head shares immutable values owned by its dependency facts.
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
    context: &CompilationBinderFacts<'_>,
    owner: AnySymbolId,
) -> BinderFactResult<DiagnosticResult<UnevaluatedDefaultTemplate>> {
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

fn predicate_parameters(
    symbols: &SymbolGraph,
    owner: PredicateDefinitionSymbolId,
) -> BinderFactResult<&[PredicateParameterSymbolId]> {
    match owner {
        PredicateDefinitionSymbolId::Predicate(owner) => {
            symbols.predicate(owner).map(|symbol| symbol.parameters())
        }
        PredicateDefinitionSymbolId::TraitMember(owner) => symbols
            .trait_predicate_member(owner)
            .map(|symbol| symbol.parameters()),
        PredicateDefinitionSymbolId::TraitFulfillment(owner) => symbols
            .trait_predicate_fulfillment(owner)
            .map(|symbol| symbol.parameters()),
    }
    .ok_or(BinderFactError::DependencyUnavailable)
}

fn direct_children<T>(root: &SyntaxNodeView<'_>) -> BinderFactResult<Vec<T>>
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
        return Err(BinderFactError::DependencyUnavailable);
    }

    Ok(children)
}

fn imported_constraints(
    context: &CompilationBinderFacts<'_>,
    address: ImportedSymbolFactAddress,
) -> BinderFactResult<(Vec<GenericConstraintTemplate>, DiagnosticBag)> {
    let result = imported_facts(
        context,
        address,
        InterfaceSemanticFactKind::GenericConstraint,
    )?;

    let mut constraints = Vec::new();

    for fact in result.value().iter() {
        let ImportedSemanticFact::GenericConstraint(fact) = fact else {
            return Err(BinderFactError::DependencyUnavailable);
        };

        constraints.push(GenericConstraintTemplate::Resolved(fact.constraint()));
    }

    // Imported fact results share immutable diagnostic storage.
    Ok((constraints, result.diagnostics().clone()))
}

fn imported_callable_contract(
    context: &CompilationBinderFacts<'_>,
    address: ImportedSymbolFactAddress,
) -> BinderFactResult<DiagnosticResult<CallableContractTemplate>> {
    let result = imported_facts(
        context,
        address,
        InterfaceSemanticFactKind::CallableContracts,
    )?;

    let [ImportedSemanticFact::CallableContracts(fact)] = result.value().as_ref() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    // The candidate-facing result shares the imported contract and diagnostics.
    Ok(DiagnosticResult::new(
        CallableContractTemplate::Resolved(fact.contract().clone()),
        result.diagnostics().clone(),
    ))
}

fn imported_facts(
    context: &CompilationBinderFacts<'_>,
    address: ImportedSymbolFactAddress,
    kind: InterfaceSemanticFactKind,
) -> BinderFactResult<std::sync::Arc<DiagnosticResult<std::sync::Arc<[ImportedSemanticFact]>>>> {
    context
        .compilation
        .imported_semantic_fact_result_with_cancellation(
            ImportedSemanticFactKey::new(address.interface(), address.symbol(), kind),
            context.cancellation,
        )
        .map_err(|error| match error {
            crate::fact::FactQueryError::Cancelled => BinderFactError::Cancelled,
            _ => BinderFactError::DependencyUnavailable,
        })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceValidationPolicy,
        test_support::encoded_template_test_interface,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::{
        CallableContractTemplate, CallableContractTemplateFact, CallableOverloadTemplateFact,
        CallableParameterDefaultTemplateFact, CallableSymbolId, GenericDeclarationTemplateFact,
        GenericOwnerId, ImplementationHeadTemplateFact, ImplementationSymbolId, PackageIdentity,
        PredicateDefinitionSymbolId, PredicateSignatureTemplateFact, SymbolFactRequest,
        SymbolOrigin, TypeExpressionTemplate, UnevaluatedDefaultTemplate,
    };

    use crate::compilation::binder::symbol::test_support::{
        binder_facts, published_fact, source_id, symbol_graph,
    };
    use crate::fact::CancellationToken;
    use crate::test_support::compilation;
    use crate::{Compilation, CompilationRequest, DependencyInterfaceInput};

    const DECLARATION_TEMPLATES: &str = r#"module app;

predicate valid<T>(value: T) = true;

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
        let facts = binder_facts(&compilation, &cancellation);

        let function = source_id(
            symbols.functions(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let Some(owner) = GenericOwnerId::try_new(function.into()) else {
            panic!("function must be a generic declaration owner");
        };

        let generic = published_fact(
            &facts,
            SymbolFactRequest::<GenericDeclarationTemplateFact>::new(owner),
        );

        assert_eq!(generic.value().parameters().len(), 2);
        assert_eq!(generic.value().constraints().len(), 1);
        assert_eq!(
            expression_text(&compilation, generic.value().constraints()[0].expression()),
            "count > 0"
        );

        let contract = published_fact(
            &facts,
            SymbolFactRequest::<CallableContractTemplateFact>::new(CallableSymbolId::from(
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

        let default = published_fact(
            &facts,
            SymbolFactRequest::<CallableParameterDefaultTemplateFact>::new(*parameter),
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
        let facts = binder_facts(&compilation, &cancellation);

        let predicate = source_id(
            symbols.predicates(),
            |symbol| symbol.origin(),
            |symbol| symbol.id(),
        );

        let predicate = published_fact(
            &facts,
            SymbolFactRequest::<PredicateSignatureTemplateFact>::new(
                PredicateDefinitionSymbolId::from(predicate),
            ),
        );

        assert_eq!(predicate.value().parameters().len(), 1);

        assert!(matches!(
            predicate.value().parameters()[0].ty(),
            TypeExpressionTemplate::Resolved(_) | TypeExpressionTemplate::Named { .. }
        ));

        let implementation = source_id(
            symbols.inherent_implementations(),
            |symbol| symbol.origin(),
            |symbol| ImplementationSymbolId::from(symbol.id()),
        );

        let head = published_fact(
            &facts,
            SymbolFactRequest::<ImplementationHeadTemplateFact>::new(implementation),
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

        let first = published_fact(
            &facts,
            SymbolFactRequest::<CallableOverloadTemplateFact>::new(overload),
        );

        let second = published_fact(
            &facts,
            SymbolFactRequest::<CallableOverloadTemplateFact>::new(overload),
        );

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.value().arms().len(), 1);
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
        let facts = binder_facts(&compilation, &cancellation);

        let functions = symbols
            .functions()
            .iter()
            .filter(|symbol| symbol.origin() == SymbolOrigin::Source)
            .map(|symbol| symbol.id())
            .collect::<Vec<_>>();

        assert_eq!(functions.len(), 2);

        for function in functions {
            let contract = published_fact(
                &facts,
                SymbolFactRequest::<CallableContractTemplateFact>::new(CallableSymbolId::from(
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
    fn imported_declarations_publish_resolved_candidate_templates() {
        let fixture = encoded_template_test_interface();

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
        let facts = binder_facts(&compilation, &cancellation);

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

        let generic = published_fact(
            &facts,
            SymbolFactRequest::<GenericDeclarationTemplateFact>::new(generic_owner),
        );

        assert_eq!(generic.value().parameters().len(), 1);
        assert_eq!(generic.value().constraints().len(), 1);
        assert!(generic.value().constraints()[0].resolved().is_some());

        let contract = published_fact(
            &facts,
            SymbolFactRequest::<CallableContractTemplateFact>::new(CallableSymbolId::from(
                function,
            )),
        );

        assert!(matches!(
            contract.value(),
            CallableContractTemplate::Resolved(contract)
                if contract.invocation_preconditions().len() == 1
        ));
    }

    fn expression_text(
        compilation: &crate::Compilation,
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
