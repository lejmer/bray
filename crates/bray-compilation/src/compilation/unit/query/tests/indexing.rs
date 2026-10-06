use super::support::fixtures::custom_index_storage_compilation;
use crate::test_support::{compilation, source_callable_body_key, source_function_body_key};
use bray_bound_tree::{
    BoundDependencyRequirement, BoundDependencySubject, BoundExpression, ConversionTarget,
    IndexTarget, SelectedOperation, SemanticSelection, StorageAccessRoot, StorageIdentity,
};
use bray_compiler_known::CompilerKnownOperationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticKind,
    DiagnosticStorageProjection, DiagnosticStorageRoot,
};
use bray_messages::DiagnosticRenderer;
use bray_source::SourceSpan;
use bray_testing::assert_goal_state_diagnostic_kind;

#[test]
fn built_in_index_and_conversion_selections_retain_exact_rules() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(pos values: [i32; 2])\n",
        "{\n",
        "    let widened = values[0] as i64;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let selections = match compilation.semantic_selections(key) {
        Ok(selections) => selections,
        Err(error) => panic!("operation selections must publish: {error:?}"),
    };

    assert!(selections.value().entries().iter().any(|entry| {
        matches!(
            entry.selection(),
            SemanticSelection::Operation(SelectedOperation::Index {
                target: IndexTarget::ArrayElement,
                ..
            })
        )
    }));

    assert!(selections.value().entries().iter().any(|entry| {
        matches!(
            entry.selection(),
            SemanticSelection::Operation(SelectedOperation::Conversion(conversion))
                if matches!(conversion.target(), ConversionTarget::BuiltInScalar)
        )
    }));

    assert!(
        selections.diagnostics().is_empty(),
        "{:?}",
        selections.diagnostics()
    );
}

#[test]
fn source_index_selections_preserve_slice_bounds_and_custom_storage() {
    let compilation = compilation(
        r#"module app;

struct Values
{
    value: i32;
}

impl Values(SliceIndex<i32>)
{
    type Output = i32;

    func slice(pos start: i32?, pos end: i32?) -> &i32
    {
        return &self.value;
    }
}

func select(pos values: Values) -> i32
{
    let lower = values[1..];
    let upper = values[..2];

    return values[1..2];
}
"#,
    );

    let key = source_callable_body_key(&compilation);

    let selections = compilation
        .semantic_selections(key.clone())
        .unwrap_or_else(|error| panic!("custom slice selection must publish: {error:?}"));

    assert!(selections.diagnostics().is_empty(), "{selections:?}");

    assert_eq!(
        selections
            .value()
            .entries()
            .iter()
            .filter(|entry| {
                matches!(
                    entry.selection(),
                    SemanticSelection::Operation(SelectedOperation::Index {
                        target: IndexTarget::Custom { .. },
                        ..
                    })
                )
            })
            .count(),
        3
    );

    let storage = compilation
        .storage_plan(key)
        .unwrap_or_else(|error| panic!("custom slice storage must publish: {error:?}"));

    let custom_roots = storage
        .value()
        .accesses()
        .iter()
        .filter(|access| access.projections().is_empty())
        .filter_map(|access| match access.root() {
            StorageAccessRoot::BorrowedStorage {
                storage: identity, ..
            } => storage.value().identity(identity),
            _ => None,
        })
        .filter(|identity| matches!(identity, StorageIdentity::CustomIndexBorrow(_)))
        .count();

    assert!(custom_roots >= 3, "{storage:?}");
}

#[test]
fn live_shared_custom_index_borrow_conflicts_with_mutable_indexing() {
    let compilation = custom_index_storage_compilation(
        r#"func exercise(pos input: Values)
{
    let mut values: Values = input;
    let selected: &Item = &values[0];
    values[0] = Item { value = 1 };
    selected;
}
"#,
    );

    assert!(
        compilation
            .check_diagnostics()
            .by_kind(DiagnosticKind::CheckingConflictingBorrow)
            .next()
            .is_some()
    );
}

#[test]
fn live_mutable_custom_index_borrow_conflicts_with_shared_indexing() {
    let compilation = custom_index_storage_compilation(
        r#"func exercise(pos input: Values)
{
    let mut values: Values = input;
    let selected: &mut Item = &mut values[0];
    let observed: i32 = values[0].value;
    selected;
}
"#,
    );

    let diagnostics = compilation.check_diagnostics();

    let access = diagnostics
        .by_kind(DiagnosticKind::CheckingConflictingBorrow)
        .filter_map(|diagnostic| {
            diagnostic
                .args()
                .iter()
                .find_map(|arg| match (arg.name(), arg.value()) {
                    (
                        DiagnosticArgName::StorageAccess,
                        DiagnosticArgValue::StorageAccess(access),
                    ) => Some(access),
                    _ => None,
                })
        })
        .find(|access| {
            access.projections().iter().any(|projection| {
                matches!(
                    projection,
                    DiagnosticStorageProjection::ProductField(name) if name == "value"
                )
            })
        })
        .unwrap_or_else(|| panic!("custom-index conflict must retain its exact field access"));

    assert_eq!(access.root(), DiagnosticStorageRoot::BorrowedStorage);

    assert_goal_state_diagnostic_kind(diagnostics, DiagnosticKind::CheckingConflictingBorrow);
}

#[test]
fn custom_index_result_contract_retains_the_receiver_borrow_lifetime() {
    let compilation = custom_index_storage_compilation(
        r#"func select(pos values: Values)
{
    let selected: &Item = &values[0];
    selected;
}
"#,
    );

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let key = source_callable_body_key(&compilation);

    let storage = compilation
        .storage_plan(key.clone())
        .unwrap_or_else(|error| panic!("custom index storage must publish: {error:?}"));

    let capability = storage
        .value()
        .accesses()
        .iter()
        .find_map(|access| match access.root() {
            StorageAccessRoot::BorrowedStorage { capability, .. } => Some(capability),
            _ => None,
        })
        .unwrap_or_else(|| panic!("custom index borrow capability must be retained"));

    let unit = compilation
        .bound_unit(key.clone())
        .unwrap_or_else(|error| panic!("custom index unit must publish: {error:?}"));

    let expression = unit
        .value()
        .tree()
        .expressions()
        .find_map(|(id, expression)| match expression {
            BoundExpression::Structured(structured)
                if structured.kind()
                    == bray_bound_tree::BoundStructuredExpressionKind::ElementIndex =>
            {
                Some(id)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("custom index expression must be bound"));

    let contracts = compilation
        .dependency_contracts(key)
        .unwrap_or_else(|error| panic!("custom index contracts must publish: {error:?}"));

    let contract = contracts
        .value()
        .expression(expression)
        .and_then(|contract| contracts.value().contract(contract))
        .unwrap_or_else(|| panic!("custom index result contract must be available"));

    assert!(
        contract.requirements().iter().any(|requirement| matches!(
            requirement,
            BoundDependencyRequirement::Direct {
                subject: BoundDependencySubject::BorrowCapability(actual),
                ..
            } if *actual == capability
        )),
        "{contract:?}"
    );
}

#[test]
fn operation_selection_reports_invalid_indexing() {
    let compilation = compilation(
        r#"module app;

func select(pos value: i32) -> i32
{
    return value[0];
}
"#,
    );

    let key = source_callable_body_key(&compilation);

    let semantics = compilation
        .expression_types(key)
        .unwrap_or_else(|error| panic!("invalid indexing must remain checkable: {error:?}"));

    assert!(semantics.diagnostics().has_errors());
}

#[test]
fn mutable_custom_indexing_requires_the_mutable_protocol() {
    let source = r#"module app;

struct Value
{
    mut element: i32;
}

impl Value(ElementIndex<i32>)
{
    type Output = i32;

    func index(pos selector: &i32) -> &i32
    {
        return &self.element;
    }
}

func mutate(pos input: Value)
{
    let mut value: Value = input;
    value[0] = 1;
}
"#;

    let compilation = compilation(source);

    let diagnostics = compilation.check_diagnostics();

    let kinds = diagnostics.iter().map(Diagnostic::kind).collect::<Vec<_>>();

    assert_eq!(
        kinds,
        [DiagnosticKind::CheckingMutableIndexContractRequired],
        "{diagnostics:#?}"
    );

    let diagnostic = bray_testing::single_diagnostic(diagnostics);

    assert_eq!(
        diagnostic.args(),
        &[DiagnosticArg::referenced_name(
            CompilerKnownOperationRole::MutableElementIndex.as_str()
        )]
    );

    let key = source_function_body_key(&compilation, "mutate");

    let unit = compilation
        .bound_unit(key)
        .unwrap_or_else(|error| panic!("mutate unit must publish: {error:?}"));

    let anchor = unit
        .value()
        .tree()
        .expressions()
        .find_map(|(_, expression)| match expression {
            BoundExpression::Structured(structured)
                if structured.kind()
                    == bray_bound_tree::BoundStructuredExpressionKind::ElementIndex =>
            {
                Some(structured.origin().source_anchor().syntax())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("mutable index expression must be bound"));

    assert_eq!(
        diagnostic.primary_span(),
        Some(SourceSpan::new(anchor.source_id(), anchor.full_range()))
    );

    assert_eq!(
        DiagnosticRenderer::english().render(diagnostic).message(),
        "mutable indexing requires an implementation of 'MutableElementIndex'"
    );

    assert_goal_state_diagnostic_kind(
        diagnostics,
        DiagnosticKind::CheckingMutableIndexContractRequired,
    );
}
