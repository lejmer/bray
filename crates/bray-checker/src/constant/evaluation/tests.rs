use std::num::{NonZeroU16, NonZeroU32};

use bray_bound_tree::{
    BoundBinaryExpression, BoundExpression, BoundExpressionId, BoundLiteralExpression,
    BoundLiteralKind, BoundNameExpression, BoundNodeOrigin, BoundOperator, BoundReferenceTarget,
    BoundStructuredExpression, BoundStructuredExpressionKind, BoundTreeBuilder, BoundUnit,
    BoundUnitId, BoundUnitKey, BoundUnitRoot,
};
use bray_compiler_known::RepresentationRole;
use bray_declarations::{DeclarationId, SyntaxAnchor, discover_source_unit_declarations};
use bray_diagnostics::DiagnosticKind;
use bray_parser::parse_source_unit;
use bray_source::{SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion};
use bray_symbols::{
    AnySymbolId, ConstantSymbolId, ConstantTermData, ConstantValueData, ConstantValueKind,
    LocalScopeBoundary, LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole,
    LocalSymbolSnapshotBuilder, ModulePathKey, PackageIdentity, RealConstantBits, SymbolFactKind,
    SymbolId, SymbolKey, SymbolKind, SymbolRootKey, TargetSizedIntegerType, TypeId,
};
use bray_syntax::LiteralExpressionSyntax;
use bray_target::{
    Endianness, ObjectFormat, TargetArchitecture, TargetIdentity, TargetMachineProperties,
    TargetProfile,
};

use crate::representation::representation_type;
use crate::test_support::{TestCheckerContext, push_expression, semantic_values, tuple_type};
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerUnitView, ConstantChecker,
    ConstantEvaluationInput, ConstantEvaluationLimits, ConstantEvaluator,
    ConstantReferenceResolution, DeclaredUnitContext, DefaultConstantChecker,
    DefaultConstantEvaluator, DefaultExpressionTypeChecker, ExpressionTypeChecker,
    ExpressionTypeExpectation, ExpressionTypeInput, SemanticUnitContext,
};

#[test]
fn evaluation_publishes_canonical_typed_literal_values() {
    let (unit, root, context) = literal_unit(
        BoundUnitId::new(90),
        "module example;\nconst value: u16 = 0x00_ff;\n",
        BoundLiteralKind::Integer,
    );

    let expected = representation(&unit, &context, RepresentationRole::ScalarU16);

    let (types, result) = evaluate(&unit, root, &context, expected, None);

    let value = constant_value(*result.value());

    assert!(!types.is_recovered());

    assert!(
        result.diagnostics().is_empty(),
        "{:?}",
        result.diagnostics()
    );

    let ConstantValueKind::Integer(integer) = value.kind() else {
        panic!("integer literal must publish an integer constant");
    };

    assert_eq!(value.ty(), expected);
    assert_eq!(integer.magnitude(), &[0xff]);
    assert_eq!(context.target_observations(), 0);
}

#[test]
fn evaluation_uses_the_selected_target_integer_width() {
    let (unit, root, context) = literal_unit(
        BoundUnitId::new(101),
        "module example;\nconst value: usize = 4294967296;\n",
        BoundLiteralKind::Integer,
    );

    let context = context.with_selected_target(target_profile_32());
    let expected = representation(&unit, &context, RepresentationRole::ScalarUsize);

    let (_, result) = evaluate(&unit, root, &context, expected, None);

    assert_eq!(
        result
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingConstantLiteralNotRepresentable)
            .count(),
        1
    );

    assert_eq!(context.target_observations(), 1);
}

#[test]
fn open_checking_retains_target_sized_literals_without_demanding_target_facts() {
    let (unit, root, context) = literal_unit(
        BoundUnitId::new(104),
        "module example;\nconst value: usize = 4294967296;\n",
        BoundLiteralKind::Integer,
    );

    let expected = representation(&unit, &context, RepresentationRole::ScalarUsize);
    let term = check_term(&unit, root, &context, expected);
    let data = semantic_values().constant_term_data(*term.value());

    let Ok(data) = data else {
        panic!("checked constant term must be available");
    };

    assert!(term.diagnostics().is_empty());

    assert!(matches!(
        data.as_ref(),
        ConstantTermData::IntegerLiteral {
            ty: TargetSizedIntegerType::Usize,
            ..
        }
    ));

    assert_eq!(context.target_observations(), 0);
}

#[test]
fn complex_literals_publish_selected_component_bits() {
    let source = "module example;\nconst value: c64 = 1.5 + 2.0i;\n";

    let (unit, root, context) =
        expression_unit(BoundUnitId::new(91), source, |tree, origins, _| {
            let [real, imaginary] = origins else {
                panic!("complex source must contain two literals");
            };

            let real = push_expression(
                tree,
                BoundExpression::Literal(BoundLiteralExpression::new(
                    real.origin,
                    real.spelling_range,
                    BoundLiteralKind::Real,
                    None,
                    false,
                )),
            );

            let imaginary = push_expression(
                tree,
                BoundExpression::Literal(BoundLiteralExpression::new(
                    imaginary.origin,
                    imaginary.spelling_range,
                    BoundLiteralKind::Imaginary,
                    None,
                    false,
                )),
            );

            push_expression(
                tree,
                BoundExpression::Binary(BoundBinaryExpression::new(
                    first_origin(origins),
                    BoundOperator::Add,
                    [real, imaginary],
                    None,
                    false,
                )),
            )
        });

    let expected = representation(&unit, &context, RepresentationRole::ScalarC64);

    let (_, result) = evaluate(&unit, root, &context, expected, None);

    let value = constant_value(*result.value());

    assert!(
        result.diagnostics().is_empty(),
        "{:?}",
        result.diagnostics()
    );

    assert_eq!(
        value.kind(),
        &ConstantValueKind::Complex {
            real: RealConstantBits::Binary32(0x3fc0_0000),
            imaginary: RealConstantBits::Binary32(0x4000_0000),
        }
    );
}

#[test]
fn aggregate_evaluation_is_bounded_and_recovers_with_a_typed_error_value() {
    let source = "module example;\nconst value: (u16, u16) = (1, 2);\n";

    let (unit, root, context) =
        expression_unit(BoundUnitId::new(92), source, |tree, origins, _| {
            let elements = origins
                .iter()
                .map(|origin| {
                    push_expression(
                        tree,
                        BoundExpression::Literal(BoundLiteralExpression::new(
                            origin.origin,
                            origin.spelling_range,
                            BoundLiteralKind::Integer,
                            None,
                            false,
                        )),
                    )
                })
                .collect::<Vec<_>>();

            push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    first_origin(origins),
                    BoundStructuredExpressionKind::Tuple,
                    elements,
                    [],
                    [],
                    None,
                    false,
                )),
            )
        });

    let element = representation(&unit, &context, RepresentationRole::ScalarU16);
    let expected = tuple_type([element, element]);

    let limits = ConstantEvaluationLimits::new(16, 1, 64);

    let (_, result) = evaluate(&unit, root, &context, expected, Some(limits));

    let value = constant_value(*result.value());

    assert_eq!(
        result
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingConstantAggregateLimitExceeded)
            .count(),
        1
    );

    assert_eq!(value.ty(), expected);
    assert_eq!(value.kind(), &ConstantValueKind::Error);
}

#[test]
fn operation_and_literal_budgets_produce_distinct_diagnostics() {
    let (unit, root, context) = literal_unit(
        BoundUnitId::new(93),
        "module example;\nconst value: u16 = 255;\n",
        BoundLiteralKind::Integer,
    );

    let expected = representation(&unit, &context, RepresentationRole::ScalarU16);

    let cases = [
        (
            ConstantEvaluationLimits::new(0, 16, 16),
            DiagnosticKind::CheckingConstantEvaluationStepLimitExceeded,
        ),
        (
            ConstantEvaluationLimits::new(16, 16, 2),
            DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded,
        ),
    ];

    for (limits, diagnostic_kind) in cases {
        let (_, result) = evaluate(&unit, root, &context, expected, Some(limits));

        let value = constant_value(*result.value());

        assert_eq!(result.diagnostics().by_kind(diagnostic_kind).count(), 1);
        assert_eq!(value.kind(), &ConstantValueKind::Error);
    }
}

#[test]
fn cancellation_publishes_neither_a_value_nor_diagnostics() {
    let (unit, root, context) = literal_unit(
        BoundUnitId::new(94),
        "module example;\nconst value: u16 = 1;\n",
        BoundLiteralKind::Integer,
    );

    let expected = representation(&unit, &context, RepresentationRole::ScalarU16);
    let types = checked_types(&unit, root, &context, expected);
    let selections = empty_selections(&unit, &types);

    let input = ConstantEvaluationInput::new(&types, &selections);
    let cancelled = TestCheckerContext::new(true);

    let entry = checker_entry(&unit);

    let request = match CheckerUnitView::new(&unit, &entry, &cancelled) {
        Ok(request) => request,
        Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
    };

    let outcome = DefaultConstantEvaluator.evaluate_constant(request, &input);

    assert!(matches!(outcome, CheckerOutcome::Cancelled));
}

#[test]
fn constant_references_reuse_values_and_recover_dependency_cycles() {
    let (seed, seed_root, seed_context) = literal_unit(
        BoundUnitId::new(95),
        "module example;\nconst value: u16 = 1;\n",
        BoundLiteralKind::Integer,
    );

    let expected = representation(&seed, &seed_context, RepresentationRole::ScalarU16);

    let (_, seed_result) = evaluate(&seed, seed_root, &seed_context, expected, None);

    let referenced_value = *seed_result.value();

    let (unit, root, context) = reference_unit(BoundUnitId::new(96), expected);

    let result = evaluate_reference(
        &unit,
        root,
        &context,
        expected,
        ConstantReferenceResolution::Cycle,
    );

    let value = constant_value(*result.value());

    assert_eq!(
        result
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingCyclicConstantDefinition)
            .count(),
        1
    );

    assert_eq!(value.kind(), &ConstantValueKind::Error);

    let (unit, root, context) = reference_unit(BoundUnitId::new(97), expected);

    let result = evaluate_reference(
        &unit,
        root,
        &context,
        expected,
        ConstantReferenceResolution::Value(referenced_value),
    );

    assert!(result.diagnostics().is_empty());
    assert_eq!(*result.value(), referenced_value);

    let types = checked_types(&unit, root, &context, expected);
    let selections = empty_selections(&unit, &types);

    let input = ConstantEvaluationInput::new(&types, &selections).with_references([
        (root, ConstantReferenceResolution::Cycle),
        (root, ConstantReferenceResolution::Value(referenced_value)),
    ]);

    let entry = checker_entry(&unit);

    let request = match CheckerUnitView::new(&unit, &entry, &context) {
        Ok(request) => request,
        Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
    };

    let outcome = DefaultConstantEvaluator.evaluate_constant(request, &input);

    assert_eq!(
        outcome,
        CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidConstantEvaluationInput
        )
    );
}

#[test]
fn open_target_facts_are_valid_terms_but_not_closed_values() {
    let (seed, _, seed_context) = literal_unit(
        BoundUnitId::new(102),
        "module example;\nconst value: u16 = 1;\n",
        BoundLiteralKind::Integer,
    );

    let expected = representation(&seed, &seed_context, RepresentationRole::ScalarU16);
    let (unit, root, context) = reference_unit(BoundUnitId::new(103), expected);

    let target_fact = match semantic_values().intern_constant_term(ConstantTermData::TargetFact(
        ConstantSymbolId::from_symbol_id(SymbolId::new(2)),
    )) {
        Ok(term) => term,
        Err(error) => panic!("target-fact term must intern: {error:?}"),
    };

    let types = checked_types(&unit, root, &context, expected);
    let selections = empty_selections(&unit, &types);

    let input = ConstantEvaluationInput::new(&types, &selections)
        .with_references([(root, ConstantReferenceResolution::Term(target_fact))]);

    let entry = checker_entry(&unit);

    let request = match CheckerUnitView::new(&unit, &entry, &context) {
        Ok(request) => request,
        Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
    };

    let Some(checked) = DefaultConstantChecker
        .check_constant_term(request, &input)
        .into_result()
    else {
        panic!("open constant checking must complete");
    };

    assert_eq!(*checked.value(), target_fact);
    assert!(checked.diagnostics().is_empty());

    let request = match CheckerUnitView::new(&unit, &entry, &context) {
        Ok(request) => request,
        Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
    };

    let Some(closed) = DefaultConstantEvaluator
        .evaluate_constant(request, &input)
        .into_result()
    else {
        panic!("closed constant evaluation must complete with recovery");
    };

    assert_eq!(
        closed
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingInvalidConstantExpression)
            .count(),
        1
    );

    assert_eq!(
        constant_value(*closed.value()).kind(),
        &ConstantValueKind::Error
    );
}

fn reference_unit(
    unit: BoundUnitId,
    expected: TypeId,
) -> (BoundUnit, BoundExpressionId, TestCheckerContext) {
    let source = "module example;\nconst value: u16 = other;\n";

    expression_unit(unit, source, |tree, _, origin| {
        let target = BoundReferenceTarget::Surface(AnySymbolId::from(
            ConstantSymbolId::from_symbol_id(SymbolId::new(1)),
        ));

        push_expression(
            tree,
            BoundExpression::Name(BoundNameExpression::new(
                origin,
                target,
                Some(expected),
                false,
            )),
        )
    })
}

fn evaluate_reference(
    unit: &BoundUnit,
    root: BoundExpressionId,
    context: &TestCheckerContext,
    expected: TypeId,
    resolution: ConstantReferenceResolution,
) -> bray_diagnostics::DiagnosticResult<bray_symbols::ConstantValueId> {
    let types = checked_types(unit, root, context, expected);
    let selections = empty_selections(unit, &types);

    let input =
        ConstantEvaluationInput::new(&types, &selections).with_references([(root, resolution)]);

    let entry = checker_entry(unit);

    let request = match CheckerUnitView::new(unit, &entry, context) {
        Ok(request) => request,
        Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
    };

    let outcome = DefaultConstantEvaluator.evaluate_constant(request, &input);

    let Some(result) = outcome.into_result() else {
        panic!("constant-reference evaluation must complete");
    };

    result
}

fn literal_unit(
    unit: BoundUnitId,
    source: &str,
    kind: BoundLiteralKind,
) -> (BoundUnit, BoundExpressionId, TestCheckerContext) {
    expression_unit(unit, source, |tree, origins, _| {
        let literal = first_literal(origins);

        push_expression(
            tree,
            BoundExpression::Literal(BoundLiteralExpression::new(
                literal.origin,
                literal.spelling_range,
                kind,
                None,
                false,
            )),
        )
    })
}

fn expression_unit(
    unit: BoundUnitId,
    source_text: &str,
    build: impl FnOnce(&mut BoundTreeBuilder, &[LiteralSource], BoundNodeOrigin) -> BoundExpressionId,
) -> (BoundUnit, BoundExpressionId, TestCheckerContext) {
    let source = snapshot(source_text);
    let parsed = parse_source_unit(&source);

    assert!(parsed.diagnostics().is_empty());

    let literals =
        bray_testing::syntax_descendants::<LiteralExpressionSyntax>(parsed.source_unit());

    let origins = literals
        .iter()
        .map(|literal| {
            let origin = BoundNodeOrigin::source(bray_bound_tree::BoundSourceAnchor::new(
                SyntaxAnchor::from_node(literal),
                source.version(),
            ));

            let Some(token) = literal.literal_token() else {
                panic!("parsed literal expression must contain its token");
            };

            LiteralSource {
                origin,
                spelling_range: token.range(),
            }
        })
        .collect::<Vec<_>>();

    let key = constant_key(&source, parsed.source_unit());

    let mut tree = BoundTreeBuilder::new(unit);

    let root = build(&mut tree, &origins, BoundNodeOrigin::source(key.source()));

    let local_symbols = local_symbols(unit, &key);

    let unit = BoundUnit::try_new(
        key,
        tree.finish(),
        local_symbols,
        [],
        BoundUnitRoot::Expression(root),
    );

    let Ok(unit) = unit else {
        panic!("constant test unit must be valid");
    };

    (unit, root, TestCheckerContext::with_source(source))
}

fn snapshot(text: &str) -> SourceSnapshot {
    let source = SourceSnapshot::new(
        SourceId::new(0),
        SourceIdentity::new(0),
        SourceOrigin::virtual_source("constant-evaluation-test"),
        SourceVersion::new(1),
        text,
    );

    match source {
        Ok(source) => source,
        Err(error) => panic!("test source must fit: {error:?}"),
    }
}

fn constant_key(
    source: &SourceSnapshot,
    source_unit: &bray_syntax::SourceUnitSyntax,
) -> BoundUnitKey {
    let declarations = discover_source_unit_declarations(source_unit);

    assert!(declarations.diagnostics().is_empty());

    let [part] = declarations.chunk().module_parts() else {
        panic!("test source must contain one module part");
    };

    let source_anchor =
        bray_bound_tree::BoundSourceAnchor::new(part.syntax_anchor(), source.version());

    let Some(package) = PackageIdentity::try_new("example.package") else {
        panic!("test package identity must be non-empty");
    };

    let Some(path) = ModulePathKey::try_new(["example"]) else {
        panic!("test module path must be non-empty");
    };

    let module = SymbolKey::module(SymbolRootKey::Package(package), path);

    let Some(owner) =
        SymbolKey::source_declaration(module, SymbolKind::Constant, DeclarationId::new(0))
    else {
        panic!("constant symbols must be source-declared");
    };

    let Some(key) = BoundUnitKey::constant_template(owner, source_anchor) else {
        panic!("constants must support constant-template units");
    };

    key
}

fn local_symbols(unit: BoundUnitId, key: &BoundUnitKey) -> bray_symbols::LocalSymbolSnapshot {
    let region = LocalSymbolRegionId::new(unit.raw());

    let region_key = LocalSymbolRegionKey::try_new(
        key.declared_owner().clone(),
        LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::ConstantDefinition),
        [key.source().syntax()],
        None,
    );

    let Some(region_key) = region_key else {
        panic!("constant test key must form a local symbol region");
    };

    let mut symbols = LocalSymbolSnapshotBuilder::new(region, region_key);

    if let Err(error) = symbols.push_scope(
        None,
        LocalScopeBoundary::Root,
        key.source().syntax(),
        key.source().syntax().full_range().start(),
    ) {
        panic!("constant test root scope must validate: {error:?}");
    }

    match symbols.finish() {
        Ok(symbols) => symbols,
        Err(error) => panic!("constant test symbols must validate: {error:?}"),
    }
}

fn evaluate(
    unit: &BoundUnit,
    root: BoundExpressionId,
    context: &TestCheckerContext,
    expected: TypeId,
    limits: Option<ConstantEvaluationLimits>,
) -> (
    bray_bound_tree::CheckedExpressionTypes,
    bray_diagnostics::DiagnosticResult<bray_symbols::ConstantValueId>,
) {
    let types = checked_types(unit, root, context, expected);
    let selections = empty_selections(unit, &types);

    let mut input = ConstantEvaluationInput::new(&types, &selections);

    if let Some(limits) = limits {
        input = input.with_limits(limits);
    }

    let entry = checker_entry(unit);

    let request = match CheckerUnitView::new(unit, &entry, context) {
        Ok(request) => request,
        Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
    };

    let outcome = DefaultConstantEvaluator.evaluate_constant(request, &input);

    let Some(result) = outcome.into_result() else {
        panic!("constant evaluation must complete");
    };

    (types, result)
}

fn check_term(
    unit: &BoundUnit,
    root: BoundExpressionId,
    context: &TestCheckerContext,
    expected: TypeId,
) -> bray_diagnostics::DiagnosticResult<bray_symbols::ConstantTermId> {
    let types = checked_types(unit, root, context, expected);
    let selections = empty_selections(unit, &types);
    let input = ConstantEvaluationInput::new(&types, &selections);
    let entry = checker_entry(unit);

    let request = match CheckerUnitView::new(unit, &entry, context) {
        Ok(request) => request,
        Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
    };

    let outcome = DefaultConstantChecker.check_constant_term(request, &input);

    let Some(result) = outcome.into_result() else {
        panic!("constant checking must complete");
    };

    result
}

fn empty_selections(
    unit: &BoundUnit,
    types: &bray_bound_tree::CheckedExpressionTypes,
) -> bray_bound_tree::CheckedSemanticSelections {
    match bray_bound_tree::CheckedSemanticSelections::try_new(unit, types, []) {
        Ok(selections) => selections,
        Err(error) => panic!("empty semantic selections must be valid: {error:?}"),
    }
}

fn checked_types(
    unit: &BoundUnit,
    root: BoundExpressionId,
    context: &TestCheckerContext,
    expected: TypeId,
) -> bray_bound_tree::CheckedExpressionTypes {
    let input = ExpressionTypeInput::new()
        .with_expectations([ExpressionTypeExpectation::new(root, expected)]);

    let entry = checker_entry(unit);

    let request = match CheckerUnitView::new(unit, &entry, context) {
        Ok(request) => request,
        Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
    };

    let outcome = DefaultExpressionTypeChecker.check_expression_types(request, &input);

    let Some(result) = outcome.into_result() else {
        panic!("constant expression typing must complete");
    };

    assert!(result.diagnostics().is_empty());

    result.into_parts().0
}

fn checker_entry(unit: &BoundUnit) -> SemanticUnitContext {
    let owner = AnySymbolId::from(ConstantSymbolId::from_symbol_id(SymbolId::new(0)));

    SemanticUnitContext::ConstantTemplate(DeclaredUnitContext::new(
        unit.key().clone(),
        owner,
        owner,
    ))
}

fn representation(
    unit: &BoundUnit,
    context: &TestCheckerContext,
    role: RepresentationRole,
) -> TypeId {
    let entry = checker_entry(unit);

    let request = match CheckerUnitView::new(unit, &entry, context) {
        Ok(request) => request,
        Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
    };

    match representation_type(request, role) {
        Ok(ty) => ty,
        Err(error) => panic!("test representation must be available: {error:?}"),
    }
}

fn constant_value(id: bray_symbols::ConstantValueId) -> std::sync::Arc<ConstantValueData> {
    match semantic_values().constant_value_data(id) {
        Ok(value) => value,
        Err(error) => panic!("test constant value must be available: {error:?}"),
    }
}

fn first_literal(origins: &[LiteralSource]) -> LiteralSource {
    let Some(origin) = origins.first() else {
        panic!("test source must contain a literal origin");
    };

    *origin
}

fn first_origin(origins: &[LiteralSource]) -> BoundNodeOrigin {
    first_literal(origins).origin
}

fn target_profile_32() -> TargetProfile {
    let Some(identity) = TargetIdentity::try_new("i686-unknown-linux-gnu") else {
        panic!("test target identity must be valid");
    };

    let pointer_width = NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN);

    let Some(machine) = TargetMachineProperties::try_new(
        TargetArchitecture::X86,
        ObjectFormat::Elf,
        Endianness::Little,
        pointer_width,
        NonZeroU32::new(4).unwrap_or(NonZeroU32::MIN),
        NonZeroU32::new(16).unwrap_or(NonZeroU32::MIN),
    ) else {
        panic!("test target machine must be valid");
    };

    match TargetProfile::try_new(
        identity,
        machine,
        match bray_target::TargetFacts::try_portable("unknown", "linux", "gnu", "gnu") {
            Some(facts) => facts,
            None => panic!("constant-evaluation test target facts must be valid"),
        },
    ) {
        Ok(profile) => profile,
        Err(error) => panic!("constant-evaluation test target must be valid: {error:?}"),
    }
}

#[derive(Clone, Copy)]
struct LiteralSource {
    origin: BoundNodeOrigin,
    spelling_range: bray_source::TextRange,
}
