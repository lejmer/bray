use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) use bray_bound_tree::testing::{
    checked_expression_types, checked_expression_types_from_entries, push_expression,
};
use bray_bound_tree::{
    BoundBlock, BoundBlockItem, BoundCallableBody, BoundCallableBodyId, BoundErrorExpression,
    BoundExpression, BoundExpressionId, BoundLiteralExpression, BoundLiteralKind,
    BoundNameExpression, BoundNodeOrigin, BoundReferenceTarget, BoundSourceAnchor, BoundTree,
    BoundTreeBuilder, BoundUnit, BoundUnitId, BoundUnitKey, BoundUnitRoot,
};
use bray_declarations::{
    DeclarationId, SyntaxAnchor, discover_source_unit_declarations, merge_declaration_chunks,
};
use bray_parser::{SyntaxTreeResult, parse_source_unit};
use bray_source::{
    SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceSpan, SourceVersion,
    TextSizeOverflow,
};
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableDependencyContracts, CallableInstanceData,
    CallablePhaseBehaviors, ConstantTermData, DependencyContractTemplateData, ExactSymbolId,
    FunctionSymbolId, GenericOwnerId, GenericSubstitutionData, IntegerConstant, IntegerSign,
    LocalScopeBoundary, LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole,
    LocalSymbolSnapshotBuilder, ModulePathKey, PackageIdentity, SemanticValueStore, SymbolGraph,
    SymbolId, SymbolKey, SymbolKind, SymbolName, SymbolRootKey, TargetSizedIntegerType,
    TraitCallableMemberSymbolId, TypeData, TypeId,
};
use bray_target::TargetProfile;

pub(crate) use bray_symbols::testing::available_compiler_known_symbols;

use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerQueryError, CheckerQueryResult,
    CheckerRequestContext, CheckerSemanticQueryProvider, CheckerSource, CheckerUnitView,
    DeclaredUnitContext, DefaultExpressionTypeChecker, ExpressionTypeChecker, ExpressionTypeInput,
    SemanticUnitContext,
};

pub(crate) struct TestCheckerContext {
    cancelled: bool,
    cancel_after: Option<usize>,
    observations: AtomicUsize,
    target_observations: AtomicUsize,
    source: Option<SourceSnapshot>,
    target: Option<TargetProfile>,
}

impl TestCheckerContext {
    pub(crate) fn new(cancelled: bool) -> Self {
        Self {
            cancelled,
            cancel_after: None,
            observations: AtomicUsize::new(0),
            target_observations: AtomicUsize::new(0),
            source: None,
            target: None,
        }
    }

    pub(crate) fn cancelling_after(observations: usize) -> Self {
        Self {
            cancelled: false,
            cancel_after: Some(observations),
            observations: AtomicUsize::new(0),
            target_observations: AtomicUsize::new(0),
            source: None,
            target: None,
        }
    }

    pub(crate) fn with_source(source: SourceSnapshot) -> Self {
        Self {
            cancelled: false,
            cancel_after: None,
            observations: AtomicUsize::new(0),
            target_observations: AtomicUsize::new(0),
            source: Some(source),
            target: None,
        }
    }

    pub(crate) fn with_selected_target(mut self, target: TargetProfile) -> Self {
        self.target = Some(target);

        self
    }

    pub(crate) fn target_observations(&self) -> usize {
        self.target_observations.load(Ordering::Relaxed)
    }
}

impl bray_base::Cancellation for TestCheckerContext {
    fn is_cancelled(&self) -> bool {
        self.cancelled
            || self
                .cancel_after
                .is_some_and(|limit| self.observations.fetch_add(1, Ordering::Relaxed) >= limit)
    }
}

impl CheckerRequestContext for TestCheckerContext {
    fn semantic_context_matches(&self, unit: &BoundUnit, context: &SemanticUnitContext) -> bool {
        match context {
            SemanticUnitContext::CallableBody(_) => callable_entry(unit.key()) == *context,
            SemanticUnitContext::ConstantTemplate(declaration) => {
                declaration.key() == unit.key()
                    && declaration.owner() == declaration.declaration()
                    && declaration.owner().kind() == SymbolKind::Constant
            }
            _ => false,
        }
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        semantic_values()
    }

    fn symbols(&self) -> &SymbolGraph {
        symbol_graph()
    }

    fn available_compiler_known_symbols(&self) -> &bray_symbols::AvailableCompilerKnownSymbols {
        available_compiler_known_symbols()
    }

    fn selected_target(&self) -> &TargetProfile {
        self.target_observations.fetch_add(1, Ordering::Relaxed);

        self.target
            .as_ref()
            .unwrap_or_else(|| test_target_profile())
    }

    fn checked_constant_expression(
        &self,
        _occurrence: bray_symbols::ConstantExpressionOccurrence,
    ) -> CheckerQueryResult<bray_diagnostics::DiagnosticResult<bray_symbols::ConstantTermId>> {
        Err(CheckerQueryError::Infrastructure(
            CheckerInfrastructureError::SemanticValueUnavailable,
        ))
    }

    fn generic_constraints(
        &self,
        _obligation: bray_symbols::GenericConstraintObligationKey,
    ) -> CheckerQueryResult<bray_diagnostics::DiagnosticResult<bray_symbols::ProofOutcome>> {
        Ok(bray_diagnostics::DiagnosticResult::without_diagnostics(
            bray_symbols::ProofOutcome::Proven,
        ))
    }

    fn declared_type_representation(
        &self,
        subject: bray_symbols::NamedTypeSymbolId,
    ) -> CheckerQueryResult<
        bray_diagnostics::DiagnosticResult<bray_symbols::DeclaredTypeRepresentation>,
    > {
        Ok(bray_diagnostics::DiagnosticResult::without_diagnostics(
            bray_symbols::DeclaredTypeRepresentation::new(subject),
        ))
    }

    fn declared_type_has_lifecycle(
        &self,
        _subject: bray_symbols::NamedTypeSymbolId,
    ) -> CheckerQueryResult<bray_diagnostics::DiagnosticResult<bool>> {
        Ok(bray_diagnostics::DiagnosticResult::without_diagnostics(
            false,
        ))
    }

    fn statically_establishes_copyability(
        &self,
        _context: &SemanticUnitContext,
        _ty: TypeId,
    ) -> CheckerQueryResult<bool> {
        Ok(false)
    }

    fn source(
        &self,
        anchor: BoundSourceAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError> {
        let span = SourceSpan::new(anchor.syntax().source_id(), anchor.syntax().full_range());
        let source = self.source.as_ref().unwrap_or_else(|| source_snapshot());

        let Some(text) = source.text_slice(span.range()) else {
            return Err(CheckerInfrastructureError::InvalidSourceRange { span });
        };

        Ok(CheckerSource::new(span, text))
    }

    fn source_syntax(
        &self,
        anchor: SyntaxAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError> {
        let span = SourceSpan::new(anchor.source_id(), anchor.full_range());
        let source = self.source.as_ref().unwrap_or_else(|| source_snapshot());

        let Some(text) = source.text_slice(span.range()) else {
            return Err(CheckerInfrastructureError::InvalidSourceRange { span });
        };

        Ok(CheckerSource::new(span, text))
    }

    fn cancellation(&self) -> &dyn bray_base::Cancellation {
        self
    }
}

impl<C> CheckerSemanticQueryProvider<C> for TestCheckerContext
where
    C: bray_symbols::SymbolQueryContract,
{
    fn resolve_symbol_query(
        &self,
        request: bray_symbols::SymbolQueryRequest<C>,
    ) -> CheckerQueryResult<
        std::sync::Arc<
            bray_diagnostics::DiagnosticResult<<C as bray_symbols::SymbolQueryContract>::Value>,
        >,
    > {
        Err(CheckerQueryError::Infrastructure(
            CheckerInfrastructureError::SemanticQueryUnavailable {
                symbol: request.symbol(),
                kind: request.kind(),
            },
        ))
    }
}

pub(crate) fn callable_entry(key: &BoundUnitKey) -> SemanticUnitContext {
    let owner = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(0)));

    // The context and bound unit share the same immutable key identity.
    SemanticUnitContext::CallableBody(DeclaredUnitContext::new(key.clone(), owner, owner))
}

pub(crate) fn semantic_values() -> &'static SemanticValueStore {
    static VALUES: OnceLock<SemanticValueStore> = OnceLock::new();

    VALUES.get_or_init(|| match SemanticValueStore::try_new() {
        Ok(values) => values,
        Err(error) => panic!("test semantic value store must be available: {error:?}"),
    })
}

pub(crate) fn empty_callable_phase_behaviors() -> CallablePhaseBehaviors {
    let dependencies = semantic_values()
        .intern_dependency_contract_template(DependencyContractTemplateData::new([]))
        .unwrap_or_else(|error| panic!("empty dependency contract must intern: {error:?}"));

    CallablePhaseBehaviors::empty(CallableDependencyContracts::synchronous(dependencies))
}

pub(crate) fn symbol_graph() -> &'static SymbolGraph {
    static GRAPH: OnceLock<SymbolGraph> = OnceLock::new();

    GRAPH.get_or_init(|| {
        let snapshot = source_snapshot();
        let parsed = parse_source_unit(snapshot);
        let discovered = discover_source_unit_declarations(parsed.source_unit());
        let merged = merge_declaration_chunks([&discovered]);

        let (syntax, _) = SyntaxTreeResult::from_source_unit_results([parsed]).into_parts();

        let Some(package) = PackageIdentity::try_new("example.package") else {
            panic!("test package identity must be non-empty");
        };

        match SymbolGraph::build_source(package, merged.table(), &syntax) {
            Ok(graph) => graph,
            Err(error) => panic!("test symbol graph must build: {error:?}"),
        }
    })
}

pub(crate) fn test_target_profile() -> &'static TargetProfile {
    static TARGET: OnceLock<TargetProfile> = OnceLock::new();

    TARGET.get_or_init(bray_target::test_support::test_target_profile)
}

pub(crate) fn compiler_known_symbol<I: ExactSymbolId>(key: &str) -> I {
    let Some(key) = bray_compiler_known::CompilerKnownDeclarationKey::try_new(key) else {
        panic!("compiler-known test key must be valid");
    };

    let Some(symbol) = available_compiler_known_symbols().declaration_symbol::<I>(&key) else {
        panic!("compiler-known test symbol must be available");
    };

    symbol
}

pub(crate) fn trait_callable_instance(
    definition: TraitCallableMemberSymbolId,
) -> CallableInstanceData {
    callable_instance(definition.into())
}

pub(crate) fn callable_instance(definition: AnySymbolId) -> CallableInstanceData {
    let substitution = empty_substitution(definition);

    let Some(definition) = CallableDefinitionId::try_new(definition) else {
        panic!("test declaration must be callable");
    };

    CallableInstanceData::new(definition, substitution)
}

fn empty_substitution(owner: AnySymbolId) -> bray_symbols::GenericSubstitutionId {
    let owner = generic_owner(owner);

    let substitution = match GenericSubstitutionData::try_new(owner, [], []) {
        Ok(substitution) => substitution,
        Err(error) => panic!("empty substitution must validate: {error:?}"),
    };

    match semantic_values().intern_generic_substitution(substitution) {
        Ok(substitution) => substitution,
        Err(error) => panic!("empty substitution must be interned: {error:?}"),
    }
}

fn generic_owner(owner: AnySymbolId) -> GenericOwnerId {
    let Some(owner) = GenericOwnerId::try_new(owner) else {
        panic!("test symbol must support generic substitution");
    };

    owner
}

pub(crate) fn callable_key() -> BoundUnitKey {
    let snapshot = source_snapshot();
    let parsed = parse_source_unit(snapshot);

    assert!(parsed.diagnostics().is_empty());

    let declarations = discover_source_unit_declarations(parsed.source_unit());

    assert!(declarations.diagnostics().is_empty());

    let [part] = declarations.chunk().module_parts() else {
        panic!("test source must contain one module part");
    };

    let source = BoundSourceAnchor::new(part.syntax_anchor(), snapshot.version());

    let Some(package) = PackageIdentity::try_new("example.package") else {
        panic!("test package identity must be non-empty");
    };

    let Some(path) = ModulePathKey::try_new(["example"]) else {
        panic!("test module path must be non-empty");
    };

    let module = SymbolKey::module(SymbolRootKey::Package(package), path);

    let Some(owner) =
        SymbolKey::source_declaration(module, SymbolKind::Function, DeclarationId::new(0))
    else {
        panic!("function symbols must be source-declared");
    };

    let Some(key) = BoundUnitKey::callable_body(owner, source) else {
        panic!("functions must support callable-body units");
    };

    key
}

pub(crate) fn recovered_tree(
    unit: BoundUnitId,
    key: &BoundUnitKey,
) -> (BoundTree, BoundCallableBodyId) {
    let mut builder = BoundTreeBuilder::new(unit);
    let origin = BoundNodeOrigin::source(key.source());

    let Ok(root) = builder.push_callable_body(BoundCallableBody::error(origin, None)) else {
        panic!("one recovered callable body must fit in an empty test tree");
    };

    (builder.finish(), root)
}

pub(crate) fn callable_unit(
    key: &BoundUnitKey,
    tree: BoundTree,
    root: BoundCallableBodyId,
) -> BoundUnit {
    let region = LocalSymbolRegionId::new(tree.unit().raw());

    let Some(region_key) = LocalSymbolRegionKey::try_new(
        key.declared_owner().clone(),
        LocalSymbolRegionRole::CallableBody,
        [key.source().syntax()],
        None,
    ) else {
        panic!("callable test keys must form local symbol regions");
    };

    let mut symbols = LocalSymbolSnapshotBuilder::new(region, region_key);

    if let Err(error) = symbols.push_scope(
        None,
        LocalScopeBoundary::Root,
        key.source().syntax(),
        key.source().syntax().full_range().start(),
    ) {
        panic!("callable test root scope must validate: {error:?}");
    }

    let symbols = match symbols.finish() {
        Ok(symbols) => symbols,
        Err(error) => panic!("callable test symbols must validate: {error:?}"),
    };

    match BoundUnit::try_new(
        key.clone(),
        tree,
        symbols,
        [],
        BoundUnitRoot::CallableBody {
            execution: bray_symbols::CallableExecution::Synchronous,
            body: root,
        },
    ) {
        Ok(unit) => unit,
        Err(error) => panic!("callable test unit must validate: {error:?}"),
    }
}

pub(crate) fn expression_unit(
    unit: BoundUnitId,
    build: impl FnOnce(&mut BoundTreeBuilder, BoundNodeOrigin) -> Vec<BoundExpressionId>,
) -> (BoundUnit, Vec<BoundExpressionId>) {
    let key = callable_key();
    let origin = BoundNodeOrigin::source(key.source());

    let mut tree = BoundTreeBuilder::new(unit);

    let expressions = build(&mut tree, origin);
    let items = expressions.iter().copied().map(BoundBlockItem::Expression);

    let block = push_block(&mut tree, origin, items);
    let root = push_callable(&mut tree, origin, block);
    let unit = callable_unit(&key, tree.finish(), root);

    (unit, expressions)
}

pub(crate) fn completed_expression_check(
    unit: &BoundUnit,
    input: &ExpressionTypeInput,
) -> bray_diagnostics::DiagnosticResult<bray_bound_tree::CheckedExpressionTypes> {
    let entry = callable_entry(unit.key());

    let context = TestCheckerContext::new(false);

    let Ok(request) = CheckerUnitView::new(unit, &entry, &context) else {
        panic!("test checker unit view must be valid");
    };

    let outcome = DefaultExpressionTypeChecker.check_expression_types(request, input);

    let CheckerOutcome::Complete(result) = outcome else {
        panic!("expression type checking must complete");
    };

    result
}

pub(crate) fn literal_expression(
    origin: BoundNodeOrigin,
    kind: BoundLiteralKind,
    ty: Option<TypeId>,
) -> BoundExpression {
    BoundExpression::Literal(BoundLiteralExpression::new(
        origin,
        origin.source_anchor().syntax().full_range(),
        kind,
        ty,
        false,
    ))
}

pub(crate) fn integer_literal_expression(
    origin: BoundNodeOrigin,
    ty: Option<TypeId>,
) -> BoundExpression {
    literal_expression(origin, BoundLiteralKind::Integer, ty)
}

pub(crate) fn unselected_name_expression(origin: BoundNodeOrigin) -> BoundExpression {
    let symbol = FunctionSymbolId::from_symbol_id(SymbolId::new(0));

    BoundExpression::Name(BoundNameExpression::new(
        origin,
        BoundReferenceTarget::Surface(AnySymbolId::from(symbol)),
        None,
        false,
    ))
}

pub(crate) fn tuple_type(elements: impl IntoIterator<Item = TypeId>) -> TypeId {
    match semantic_values().intern_type(TypeData::tuple(elements)) {
        Ok(ty) => ty,
        Err(error) => panic!("test tuple type must be valid: {error:?}"),
    }
}

pub(crate) fn array_type(element: TypeId, length: usize) -> TypeId {
    let magnitude = u64::try_from(length)
        .unwrap_or_else(|_| panic!("test array length must fit in u64"))
        .to_be_bytes();

    let value = IntegerConstant::new(IntegerSign::NonNegative, magnitude);

    let length = semantic_values()
        .intern_constant_term(ConstantTermData::IntegerLiteral {
            ty: TargetSizedIntegerType::Usize,
            value,
        })
        .unwrap_or_else(|error| panic!("test array length must be valid: {error:?}"));

    semantic_values()
        .intern_type(TypeData::Array { element, length })
        .unwrap_or_else(|error| panic!("test array type must be valid: {error:?}"))
}

pub(crate) fn symbol_name(name: &str) -> SymbolName {
    let Some(name) = SymbolName::try_new(name) else {
        panic!("test symbol name must be valid");
    };

    name
}

pub(crate) fn declaration_key(kind: SymbolKind, declaration: u32) -> SymbolKey {
    let Some(package) = PackageIdentity::try_new("example.package") else {
        panic!("test package identity must be valid");
    };

    let owner = SymbolKey::package(package);

    let Some(key) = SymbolKey::source_declaration(owner, kind, DeclarationId::new(declaration))
    else {
        panic!("test declaration key must support the requested kind");
    };

    key
}

pub(crate) fn type_data(ty: TypeId) -> TypeData {
    match semantic_values().type_data(ty) {
        Ok(data) => data.as_ref().clone(),
        Err(error) => panic!("test type must belong to the semantic store: {error:?}"),
    }
}

pub(crate) fn push_block(
    tree: &mut BoundTreeBuilder,
    origin: BoundNodeOrigin,
    items: impl IntoIterator<Item = BoundBlockItem>,
) -> bray_bound_tree::BoundBlockId {
    match tree.push_block(BoundBlock::new(origin, items, false)) {
        Ok(block) => block,
        Err(error) => panic!("test block must be valid: {error:?}"),
    }
}

pub(crate) fn push_callable(
    tree: &mut BoundTreeBuilder,
    origin: BoundNodeOrigin,
    block: bray_bound_tree::BoundBlockId,
) -> BoundCallableBodyId {
    match tree.push_callable_body(BoundCallableBody::block(origin, block)) {
        Ok(body) => body,
        Err(error) => panic!("test callable body must be valid: {error:?}"),
    }
}

pub(crate) fn normally_completing_recovered_tree(
    unit: BoundUnitId,
    key: &BoundUnitKey,
) -> (BoundTree, BoundCallableBodyId) {
    let mut builder = BoundTreeBuilder::new(unit);
    let origin = BoundNodeOrigin::source(key.source());

    let expression = BoundExpression::Error(BoundErrorExpression::new(origin, error_type()));

    let Ok(expression) = builder.push_expression(expression) else {
        panic!("one recovered expression must fit in an empty test tree");
    };

    let block = BoundBlock::new(origin, [BoundBlockItem::Expression(expression)], false);

    let Ok(block) = builder.push_block(block) else {
        panic!("one block must fit in the test tree");
    };

    let Ok(root) = builder.push_callable_body(BoundCallableBody::block(origin, block)) else {
        panic!("one callable body must fit in the test tree");
    };

    (builder.finish(), root)
}

pub(crate) fn error_type() -> TypeId {
    let Ok(ty) = semantic_values().intern_type(TypeData::Error) else {
        panic!("test error type must be interned");
    };

    ty
}

pub(crate) fn distinct_source_origins() -> [BoundNodeOrigin; 2] {
    let [source, module, _] = test_source_origins();

    [source, module]
}

pub(crate) fn test_source_origins() -> [BoundNodeOrigin; 3] {
    let parsed = parse_source_unit(source_snapshot());
    let source_unit = parsed.source_unit();

    let Some(module) = source_unit.source_unit_module_declaration() else {
        panic!("test source must contain its module declaration");
    };

    let Some(function) = source_unit.function_declarations().next() else {
        panic!("test source must contain its function declaration");
    };

    let source = BoundSourceAnchor::new(
        SyntaxAnchor::from_node(source_unit),
        source_snapshot().version(),
    );

    let module = BoundSourceAnchor::new(
        SyntaxAnchor::from_node(&module),
        source_snapshot().version(),
    );

    let function = BoundSourceAnchor::new(
        SyntaxAnchor::from_node(&function),
        source_snapshot().version(),
    );

    [
        BoundNodeOrigin::source(source),
        BoundNodeOrigin::source(module),
        BoundNodeOrigin::source(function),
    ]
}

fn source() -> Result<SourceSnapshot, TextSizeOverflow> {
    SourceSnapshot::new(
        SourceId::new(0),
        SourceIdentity::new(0),
        SourceOrigin::virtual_source("checker-test"),
        SourceVersion::new(1),
        "module example;\nfunc main() {}\n",
    )
}

fn source_snapshot() -> &'static SourceSnapshot {
    static SOURCE: OnceLock<SourceSnapshot> = OnceLock::new();

    SOURCE.get_or_init(|| match source() {
        Ok(source) => source,
        Err(error) => panic!("test source must fit in the source range representation: {error:?}"),
    })
}
