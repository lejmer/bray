use crate::test_support::{
    compilation, source_callable_body_key, source_function_body_key,
    source_trait_callable_fulfillment_body_key,
};
use bray_binder::semantic_unit_context;
use bray_bound_tree::{
    BoundUnitKind, StorageAccessPurpose, StorageAccessRoot, StorageBinding, StorageBindingTarget,
    StorageIdentity, StorageProjection,
};

use bray_diagnostics::DiagnosticKind;
use bray_symbols::TypeData;
use std::sync::Arc;

#[test]
fn storage_plans_publish_unit_local_identities_accesses_and_dependencies() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Point\n",
        "{\n",
        "    x: i32;\n",
        "    y: i32;\n",
        "}\n",
        "func main(input: Point, items: [i32; 4]) -> i32\n",
        "{\n",
        "    let mut value: i32 = input.x;\n",
        "    let { x, y }: Point = input;\n",
        "    let shared = &value;\n",
        "    let exclusive = & mut value;\n",
        "    let first = items[0];\n",
        "    let middle = items[1..3];\n",
        "    value = x;\n",
        "    return value;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    assert_eq!(
        compilation.state.storage_plans.is_published(&key),
        Ok(false)
    );

    let first = match compilation.storage_plan(key.clone()) {
        Ok(plan) => plan,
        Err(error) => panic!("storage planning must publish: {error:?}"),
    };

    assert!(
        first
            .value()
            .identities()
            .iter()
            .any(|identity| { matches!(identity, StorageIdentity::Parameter(_)) })
    );

    assert!(
        first
            .value()
            .identities()
            .iter()
            .any(|identity| { matches!(identity, StorageIdentity::LocalOwned(_)) })
    );

    assert!(
        first
            .value()
            .identities()
            .iter()
            .any(|identity| { matches!(identity, StorageIdentity::Result(_)) })
    );

    assert!(
        first
            .value()
            .identities()
            .iter()
            .any(|identity| { matches!(identity, StorageIdentity::Temporary(_)) })
    );

    assert!(
        first
            .value()
            .bindings()
            .iter()
            .any(|(target, _)| { matches!(target, StorageBindingTarget::Parameter(_)) })
    );

    assert!(
        first
            .value()
            .bindings()
            .iter()
            .any(|(target, _)| { matches!(target, StorageBindingTarget::Local(_)) })
    );

    assert!(
        first
            .value()
            .bindings()
            .iter()
            .any(|(target, _)| { matches!(target, StorageBindingTarget::Result) })
    );

    assert!(first.value().accesses().iter().any(|access| {
        access
            .projections()
            .iter()
            .any(|projection| matches!(projection, StorageProjection::ProductField(_)))
    }));

    assert!(!first.value().borrow_capabilities().is_empty());

    assert!(
        first
            .value()
            .accesses()
            .iter()
            .any(|access| { matches!(access.root(), StorageAccessRoot::Borrow(_)) })
    );

    for purpose in [
        StorageAccessPurpose::Read,
        StorageAccessPurpose::Initialize,
        StorageAccessPurpose::Write,
        StorageAccessPurpose::ValueTransfer,
        StorageAccessPurpose::Assignment,
        StorageAccessPurpose::Member,
        StorageAccessPurpose::Index,
        StorageAccessPurpose::Slice,
        StorageAccessPurpose::Projection,
        StorageAccessPurpose::Borrow(bray_symbols::BorrowKind::Shared),
        StorageAccessPurpose::Borrow(bray_symbols::BorrowKind::Mutable),
    ] {
        assert!(
            first
                .value()
                .access_plans()
                .iter()
                .any(|plan| plan.purpose() == purpose),
            "storage plan must retain {purpose:?}"
        );
    }

    let second = match compilation.storage_plan(key.clone()) {
        Ok(plan) => plan,
        Err(error) => panic!("repeated storage planning must publish: {error:?}"),
    };

    assert!(Arc::ptr_eq(&first, &second));

    let dependencies = match compilation
        .state
        .fact_runtime
        .dependencies(&crate::fact::CompilationFactKey::StoragePlan(key.clone()))
    {
        Ok(Some(dependencies)) => dependencies,
        Ok(None) => panic!("published storage plans must retain dependencies"),
        Err(error) => panic!("storage-plan dependencies must be readable: {error:?}"),
    };

    assert!(dependencies.contains(&crate::fact::CompilationFactKey::BoundUnit(key.clone())));

    assert!(
        dependencies.contains(&crate::fact::CompilationFactKey::ExpressionSemantics(
            key.clone()
        ))
    );

    assert!(
        dependencies.contains(&crate::fact::CompilationFactKey::CheckedPatterns(
            key.clone()
        ))
    );
}

#[test]
fn storage_plans_retain_exact_branch_dependent_alternative_bindings() {
    let compilation = compilation(concat!(
        "module app;\n",
        "union Choice\n",
        "{\n",
        "    First(value: i32);\n",
        "    Second(value: i32);\n",
        "}\n",
        "func main(value: Choice)\n",
        "{\n",
        "    match value\n",
        "    {\n",
        "        case First(value = item) | Second(value = item)\n",
        "        {\n",
        "            item;\n",
        "        }\n",
        "    }\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let plan = match compilation.storage_plan(key) {
        Ok(plan) => plan,
        Err(error) => panic!("alternative-pattern storage planning must publish: {error:?}"),
    };

    let Some((_, StorageBinding::Identity(storage))) = plan
        .value()
        .bindings()
        .iter()
        .find(|(target, _)| matches!(target, StorageBindingTarget::Local(_)))
    else {
        panic!("coherent alternatives must retain one logical local binding");
    };

    let Some(StorageIdentity::Alternative { alternative, .. }) = plan.value().identity(*storage)
    else {
        panic!("coherent alternatives must retain an exact logical alias");
    };

    let Some(alternative) = plan.value().alternative(alternative) else {
        panic!("logical alias must retain its branch accesses");
    };

    assert_eq!(alternative.accesses().len(), 2);

    assert!(
        plan.value()
            .accesses()
            .iter()
            .all(|access| !access.is_recovered())
    );

    let Some((logical_access, _)) = plan.value().access_entries().find(
        |(_, access)| matches!(access.root(), StorageAccessRoot::Storage(root) if root == *storage),
    ) else {
        panic!("the matched region must access the logical alias");
    };

    assert_eq!(
        plan.value().relationship(logical_access, logical_access),
        bray_bound_tree::StorageRelationship::Identical
    );

    assert!(alternative.accesses().iter().all(|branch| {
        plan.value().relationship(logical_access, *branch)
            == bray_bound_tree::StorageRelationship::PotentiallyOverlapping
    }));
}

#[test]
fn recovered_pattern_projections_preserve_binding_diagnostics() {
    for (valid_source, occupied) in [
        (
            r#"module app;

struct Guard
{
    id: usize;
}

func guard() {}

func main(pos value: &box Guard) -> usize
{
    return match value
    {
        case box(fresh) { yield fresh.id; }
    };
}
"#,
            "guard",
        ),
        (
            r#"module app;

struct Guard
{
    id: usize;
}

func guard() {}

func main(pos value: &box Guard?) -> usize
{
    return match value
    {
        case box(?fresh) { yield fresh.id; }
        case _ { yield 0; }
    };
}
"#,
            "guard",
        ),
        (
            r#"module app;

struct Guard
{
    id: usize;
}

func guard() {}

func main(pos value: &(Guard?, Guard?)) -> usize
{
    return match value
    {
        case(?fresh, ?other) { yield other.id; }
        case _ { yield 0; }
    };
}
"#,
            "guard",
        ),
        (
            r#"module app;

struct Guard
{
    id: usize;
}

func main(pos value: &(Guard?, Guard?)) -> usize
{
    return match value
    {
        case(?first, ?fresh) { yield first.id; }
        case _ { yield 0; }
    };
}
"#,
            "first",
        ),
        (
            r#"module app;

struct Guard
{
    id: usize;
}

func main(pos value: &[Guard?; 2]) -> usize
{
    return match value
    {
        case [?first, ?fresh] { yield first.id; }
        case _ { yield 0; }
    };
}
"#,
            "first",
        ),
        (
            r#"module app;

struct Guard
{
    id: usize;
}

union Choice
{
    Pair(first: Guard, second: Guard);
    Empty;
}

func guard() {}

func main(pos value: &Choice) -> usize
{
    return match value
    {
        case Pair(first = fresh, second = other) { yield other.id; }
        case Empty { yield 0; }
    };
}
"#,
            "guard",
        ),
        (
            r#"module app;

struct Guard
{
    id: usize;
}

union Choice
{
    Pair(first: Guard, second: Guard);
    Empty;
}

func main(pos value: &Choice) -> usize
{
    return match value
    {
        case Pair(first = first, second = fresh) { yield first.id; }
        case Empty { yield 0; }
    };
}
"#,
            "first",
        ),
        (
            r#"module app;

struct Guard
{
    id: usize;
}

func guard() {}

func main(pos value: &(Guard?, Guard?)) -> usize
{
    if let (?fresh, ?other) = value
    {
        return other.id;
    }

    return 0;
}
"#,
            "guard",
        ),
    ] {
        let valid = compilation(valid_source);
        let diagnostics = valid.check_diagnostics();

        assert!(diagnostics.is_empty(), "{valid_source}: {diagnostics:?}");

        assert!(
            valid
                .lowered_unit(source_function_body_key(&valid, "main"))
                .unwrap()
                .value()
                .is_some()
        );

        let source = valid_source.replace("fresh", occupied);
        let compilation = compilation(&source);
        let key = source_function_body_key(&compilation, "main");
        let bound = compilation.bound_unit(key.clone()).unwrap();

        let expected = bound
            .diagnostics()
            .by_kind(DiagnosticKind::BindingNameAlreadyDefined)
            .collect::<Vec<_>>();

        assert_eq!(expected.len(), 1, "{source}: {:?}", bound.diagnostics());

        let span = expected[0].primary_span().unwrap();

        assert_eq!(
            span.start().bytes(),
            u32::try_from(valid_source.find("fresh").unwrap()).unwrap()
        );

        assert_eq!(span.range().slice_str(&source).unwrap(), occupied);
        assert!(!expected[0].related_locations().is_empty());

        let diagnostics = compilation.check_diagnostics();

        let actual = diagnostics
            .by_kind(DiagnosticKind::BindingNameAlreadyDefined)
            .collect::<Vec<_>>();

        assert_eq!(actual, expected, "{source}: {diagnostics:?}");
        assert!(compilation.lowered_unit(key).unwrap().value().is_none());
    }
}

#[test]
#[should_panic(expected = "InactiveProjection")]
fn missing_projection_refinements_remain_storage_invariant_failures() {
    use bray_bound_tree::CheckedRefinements;
    use bray_checker::{CheckerOutcome, DefaultStorageFlowChecker, StorageFlowChecker};

    let compilation = compilation(
        r#"module app;

struct Guard
{
    id: usize;
}

func main(pos value: &Guard?) -> usize
{
    return match value
    {
        case ?guard { yield guard.id; }
        case none { yield 0; }
    };
}
"#,
    );

    assert!(compilation.check_diagnostics().is_empty());

    let key = source_function_body_key(&compilation, "main");
    let bound = compilation.bound_unit(key.clone()).unwrap();
    let storage = compilation.storage_plan(key.clone()).unwrap();

    let expressions = compilation
        .expression_semantics_with_cancellation(key.clone(), &compilation.state.cancellation)
        .unwrap();

    let patterns = compilation.patterns(key.clone()).unwrap();
    let memory = compilation.memory_operations(key.clone()).unwrap();
    let refinements = compilation.refinements(key.clone()).unwrap();

    let context = compilation
        .checker_context_for(&key, &compilation.state.cancellation)
        .unwrap();

    let semantic_context = semantic_unit_context(context.symbols(), bound.value());

    let request = bray_checker::CheckerUnitView::new(bound.value(), &semantic_context, &context);

    assert!(!storage.value().is_recovered());
    assert!(!refinements.value().is_recovered());

    assert!(matches!(
        DefaultStorageFlowChecker.check_storage_flow(
            request,
            expressions.result().value(),
            patterns.value(),
            storage.value(),
            refinements.value(),
            memory.value()
        ),
        CheckerOutcome::Complete(_)
    ));

    let missing = CheckedRefinements::try_new(bound.value().unit(), key.kind(), [], false).unwrap();

    DefaultStorageFlowChecker.check_storage_flow(
        request,
        expressions.result().value(),
        patterns.value(),
        storage.value(),
        &missing,
        memory.value(),
    );
}

#[test]
fn storage_plans_retain_recovery_without_panicking() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(value: i32)\n",
        "{\n",
        "    let broken: i32 = ;\n",
        "    value;\n",
        "}\n",
    ));

    let key = source_callable_body_key(&compilation);

    let control = match compilation.control_flow(key.clone()) {
        Ok(control) => control,
        Err(error) => panic!("recovered control flow must publish: {error:?}"),
    };

    assert!(control.value().is_recovered());

    let plan = match compilation.storage_plan(key.clone()) {
        Ok(plan) => plan,
        Err(error) => panic!("recovered storage planning must publish: {error:?}"),
    };

    assert!(
        plan.value()
            .accesses()
            .iter()
            .any(bray_bound_tree::StorageAccess::is_recovered)
    );

    let liveness = match compilation.liveness(key) {
        Ok(liveness) => liveness,
        Err(error) => panic!("recovered liveness analysis must publish: {error:?}"),
    };

    assert!(liveness.value().is_recovered());
}

#[test]
fn implementation_self_parameter_storage_uses_the_checked_body_type() {
    for borrow in ["&", "&mut "] {
        let source = format!(
            "module app; struct Holder {{ value: i32; }} trait Forward {{ static func forward(pos value: {borrow}Self) -> {borrow}Self; }} impl Holder(Forward) {{ static func forward(pos value: {borrow}Self) -> {borrow}Self {{ return value; }} }}"
        );

        let compilation = compilation(&source);

        assert!(!compilation.check_diagnostics().has_errors(), "{source}");

        let key = source_trait_callable_fulfillment_body_key(&compilation, "forward");

        let plan = compilation.storage_plan(key.clone()).unwrap();
        let values = compilation.semantic_value_store().unwrap();
        let mut parameters = 0;

        for (identity, storage) in plan.value().identity_entries() {
            if !matches!(storage, bray_bound_tree::StorageIdentity::Parameter(_)) {
                continue;
            }

            parameters += 1;

            let ty = plan.value().storage_type(identity).unwrap();
            let data = values.type_data(ty);

            let TypeData::Borrow { target, .. } = data.as_ref() else {
                panic!("parameter must remain borrowed: {data:?}");
            };

            assert!(matches!(
                values.type_data(*target).as_ref(),
                TypeData::Named { .. }
            ));

            let lowered = compilation.lowered_unit(key.clone()).unwrap();
            let mir = lowered.value().as_ref().unwrap().mir().unwrap();

            assert_eq!(
                mir.storages()
                    .iter()
                    .find(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(0)))
                    .unwrap()
                    .ty(),
                ty
            );
        }

        assert_eq!(parameters, 1);
    }
}

#[test]
fn storage_plans_cover_receiver_predicate_and_anonymous_parameters() {
    let compilation = compilation(concat!(
        "module app;\n",
        "predicate accepts(value: i32) = true;\n",
        "struct Counter\n",
        "{\n",
        "    func read() -> i32\n",
        "    {\n",
        "        return 0;\n",
        "    }\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    let callable = lambda(value: i32)\n",
        "    {\n",
        "        value;\n",
        "    };\n",
        "}\n",
    ));

    let keys = match compilation.declared_unit_keys_for_test() {
        Ok(keys) => keys,
        Err(error) => panic!("declared unit keys must be available: {error:?}"),
    };

    let receiver = keys
        .iter()
        .filter(|key| key.kind() == BoundUnitKind::CallableBody)
        .filter_map(|key| compilation.storage_plan(key.clone()).ok())
        .find(|plan| {
            plan.value()
                .identities()
                .iter()
                .any(|identity| matches!(identity, StorageIdentity::Receiver(_)))
        })
        .unwrap_or_else(|| panic!("type callable storage must retain its receiver"));

    assert!(receiver.diagnostics().is_empty());

    let predicate_key = keys
        .iter()
        .find(|key| key.kind() == BoundUnitKind::PredicateDefinition)
        .unwrap_or_else(|| panic!("predicate definition key must be available"));

    let predicate = match compilation.storage_plan(predicate_key.clone()) {
        Ok(plan) => plan,
        Err(error) => panic!("predicate storage planning must publish: {error:?}"),
    };

    assert!(
        predicate
            .value()
            .identities()
            .iter()
            .any(|identity| matches!(identity, StorageIdentity::PredicateParameter(_)))
    );

    let main = source_callable_body_key(&compilation);

    let bound = match compilation.bound_unit(main) {
        Ok(bound) => bound,
        Err(error) => panic!("source callable must bind: {error:?}"),
    };

    let [nested] = bound.value().nested_units() else {
        panic!("source callable must contain one anonymous callable");
    };

    let anonymous = match compilation.storage_plan(nested.clone()) {
        Ok(plan) => plan,
        Err(error) => panic!("anonymous callable storage planning must publish: {error:?}"),
    };

    assert!(
        anonymous
            .value()
            .identities()
            .iter()
            .any(|identity| matches!(identity, StorageIdentity::AnonymousParameter(_)))
    );
}
