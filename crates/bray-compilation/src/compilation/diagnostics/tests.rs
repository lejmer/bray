use std::sync::Arc;

use bray_binder::semantic_unit_context;
use bray_bound_tree::{BoundUnitKind, BoundUnitRoot};
use bray_checker::SemanticUnitContext;
use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
use bray_messages::{DiagnosticRenderer, RenderedDiagnostic};
use bray_source::TextSize;
use bray_symbols::{CallableContractClauseKind, ConstantTermData, PackageIdentity, ProductKind};
use bray_target::NativeTarget;

use crate::fact::{CancellationToken, FactCellTestEvent};
use crate::test_support::{
    FactTestGate, compilation, compilation_with_sources_and_worker_budget, diagnostic_kinds,
    source_callable_body_key, source_input,
};
use crate::{Compilation, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget};

#[test]
fn semantic_query_failure_preserves_diagnostic_context() {
    use crate::compilation::semantic_error::{
        SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
    };

    let compilation = compilation(
        r#"
module app;
func main() {}
"#,
    );

    let key = crate::test_support::source_function_body_key(&compilation, "main");

    let error = SemanticQueryFailure::contract(
        SemanticQueryContext::Unit(key.clone()),
        SemanticQueryViolation::CapacityExceeded {
            data: SemanticDataKind::Type,
            value: usize::MAX,
        },
    )
    .into();

    let diagnostics = super::unit::semantic_query_failure_diagnostics(&key, &error);

    bray_testing::assert_goal_state_diagnostic_kind(
        &diagnostics,
        DiagnosticKind::CheckingCapacityExceeded,
    );

    assert_eq!(diagnostics.len(), 1);

    assert_eq!(
        diagnostics
            .iter()
            .next()
            .expect("fixture iterator must provide its expected item")
            .primary_span(),
        error.source()
    );
}

#[test]
#[ignore = "tier:proof"]
fn diagnostics_share_body_work_beyond_the_query_cache_working_set() {
    use std::fmt::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_symbols::SymbolQueryKind;

    use crate::fact::{CompilationFactKey, FactEvaluationTestObserver};
    use crate::test_support::source_function_body_key;

    let count = 4_100;
    let mut source = String::from("module app;\n");

    // Equal unit sizes keep the batch in source order, so the first item leaves
    // the working set before the last one.
    for index in 0..count {
        writeln!(source, "func item{index:04}() {{}}").expect("fixture source must be constructed");
    }

    let compilation = compilation(&source);
    let bodies = Arc::new(AtomicUsize::new(0));
    let contracts = Arc::new(AtomicUsize::new(0));
    let observed_bodies = Arc::clone(&bodies);
    let observed_contracts = Arc::clone(&contracts);

    compilation
        .state
        .fact_runtime
        .set_test_observer(FactEvaluationTestObserver::new(move |key| {
            if matches!(key, CompilationFactKey::BodySemantics(_)) {
                observed_bodies.fetch_add(1, Ordering::SeqCst);
            }

            if matches!(key, CompilationFactKey::Symbol(query)
                    if query.kind() == SymbolQueryKind::CallableContracts)
            {
                observed_contracts.fetch_add(1, Ordering::SeqCst);
            }
        }))
        .expect("fixture task observer must be installed");

    assert!(compilation.check_diagnostics().is_empty());
    assert_eq!(bodies.load(Ordering::SeqCst), count);
    assert_eq!(contracts.load(Ordering::SeqCst), count);

    let first = source_function_body_key(&compilation, "item0000");
    let last = source_function_body_key(&compilation, "item4099");

    assert_eq!(
        compilation.state.body_semantics.is_published(&first),
        Ok(false)
    );

    assert_eq!(
        compilation.state.body_semantics.is_published(&last),
        Ok(true)
    );
}

#[test]
fn unit_diagnostic_bags_do_not_retain_checked_body_results() {
    let compilation = compilation(
        r#"
            module app;

            func main()
            {
            }
            "#,
    );

    let key = source_callable_body_key(&compilation);
    let cancellation = CancellationToken::new();

    let initial = compilation
        .semantic_unit_diagnostic_sources(key.clone(), &cancellation)
        .expect("unit diagnostics must initialize the analysis caches");

    drop(initial);

    let body = compilation
        .body_semantics_with_cancellation(key.clone(), &cancellation)
        .expect("checked body must remain cached");

    let owners = Arc::strong_count(&body);

    let (_, diagnostics) = compilation
        .semantic_unit_diagnostic_sources(key, &cancellation)
        .expect("unit diagnostics must remain available");

    assert!(diagnostics.iter().all(DiagnosticBag::is_empty));
    assert_eq!(Arc::strong_count(&body), owners);
}

#[test]
fn package_semantic_diagnostics_request_all_declared_unit_categories() {
    let compilation = compilation(concat!(
        "module app;\n",
        "const size: i32 = 1;\n",
        "predicate valid() = true;\n",
        "struct value with(true)\n",
        "{\n",
        "}\n",
        "func check(value: i32 = 1) requires(true) ensures(true) with(true)\n",
        "{\n",
        "}\n",
    ));

    let mut kinds = match compilation.declared_unit_keys_for_test() {
        Ok(keys) => keys.into_iter().map(|key| key.kind()).collect::<Vec<_>>(),
        Err(error) => panic!("semantic unit keys must be discoverable: {error:?}"),
    };

    kinds.sort_unstable();

    assert_eq!(
        kinds,
        [
            BoundUnitKind::CallableBody,
            BoundUnitKind::RuntimeDefault,
            BoundUnitKind::ConstantTemplate,
            BoundUnitKind::PredicateDefinition,
            BoundUnitKind::Constraint,
            BoundUnitKind::Constraint,
            BoundUnitKind::ContractClause,
            BoundUnitKind::ContractClause,
        ]
    );
}

#[test]
fn static_directive_arguments_are_not_initializer_units() {
    let source = concat!(
        "module app;\n",
        "@symbol(name = \"exported_value\")\n",
        "static EXPORTED_VALUE: i32 = 41;\n",
    );

    let compilation = compilation(source);

    let keys = compilation
        .declared_unit_keys_for_test()
        .unwrap_or_else(|error| panic!("static initializer key must be available: {error:?}"));

    let [key] = keys.as_slice() else {
        panic!("test package must contain one semantic unit");
    };

    let initializer_start = source
        .find("41")
        .and_then(|offset| u32::try_from(offset).ok())
        .map(TextSize::from);

    assert_eq!(
        Some(key.source().syntax().full_range().start()),
        initializer_start
    );
}

#[test]
fn recovered_static_construction_preserves_its_binding_diagnostic() {
    let compilation = compilation(
        r#"
                module app;
                struct Value { number: i32; }
                static VALUE: Value = { number = missing() };
            "#,
    );

    let diagnostics = compilation.check_diagnostics();

    assert_eq!(
        diagnostic_kinds(diagnostics),
        [DiagnosticKind::BindingUnresolvedName]
    );
}

#[test]
fn static_initializers_do_not_prevent_constant_pattern_analysis() {
    let compilation = compilation(concat!(
        "module app;\n",
        "static LOCK_STATE: u32 = 0;\n",
        "const ONE: u32 = 1;\n",
        "func is_one(pos value: u32) -> bool\n",
        "{\n",
        "    return match value\n",
        "    {\n",
        "        case ONE { yield true; }\n",
        "        case _ { yield false; }\n",
        "    };\n",
        "}\n",
    ));

    let diagnostics = compilation
        .semantic_diagnostics_with_cancellation(&compilation.state.cancellation)
        .unwrap_or_else(|error| {
            panic!("static and constant diagnostics must be available: {error:?}")
        });

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn target_gated_nested_module_values_resolve_across_source_units() {
    let sources = [
        concat!(
            "@target(target.identity.NAME == \"x86_64-unknown-linux-gnu\")\n",
            "trusted module std.os.linux;\n",
            "const FLAG: u32 = 1;\n",
            "extern trusted func native_value() -> u32 uses(foreign_call);\n",
        ),
        concat!(
            "@target(target.identity.NAME == \"x86_64-unknown-linux-gnu\")\n",
            "trusted module std.platform;\n",
            "using std.os.linux;\n",
            "trusted func value() -> u32\n",
            "{\n",
            "    return trusted std.os.linux.native_value() + std.os.linux.FLAG;\n",
            "}\n",
        ),
    ];

    let compilation = compilation_with_sources_and_worker_budget(&sources, WorkerBudget::serial());

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );
}

#[test]
fn static_atomic_storage_uses_constant_initialization() {
    let source = r#"module app;

using core.atomic;
static INPUT_LOCK: core.atomic.Atomic<u32> = core.atomic.initialize<u32>(0);
static OUTPUT_LOCK: core.atomic.Atomic<u32> = core.atomic.initialize<u32>(0);
static ERROR_LOCK: core.atomic.Atomic<u32> = core.atomic.initialize<u32>(0);
"#;

    let package = PackageIdentity::try_new("std")
        .unwrap_or_else(|| panic!("standard library package identity must be valid"));

    let options = CompilationOptions::new(
        WorkerBudget::serial(),
        ProductKind::Library,
        SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
    );

    let request = CompilationRequest::with_options(package, vec![source_input(source, 0)], options)
        .with_standard_library_source_authority();

    let compilation = Compilation::load(request)
        .unwrap_or_else(|error| panic!("standard library test compilation must load: {error:?}"));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let symbols = compilation
        .symbol_graph()
        .unwrap_or_else(|error| panic!("atomic static symbols must publish: {error:?}"));

    for declaration in symbols.statics().iter().map(bray_symbols::StaticSymbol::id) {
        let template = compilation
            .static_instance_template(declaration)
            .unwrap_or_else(|error| panic!("atomic static template must publish: {error:?}"));

        assert!(
            template.diagnostics().is_empty(),
            "{:#?}",
            template.diagnostics()
        );
    }
}

#[test]
fn check_diagnostics_lazily_request_and_cache_semantics() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let value = 1;\n",
        "    let value = 2;\n",
        "}\n",
    ));

    let keys = match compilation.declared_unit_keys_for_test() {
        Ok(keys) => keys,
        Err(error) => panic!("callable key must be discoverable: {error:?}"),
    };

    let [key] = keys.as_slice() else {
        panic!("test package must contain one semantic unit");
    };

    assert_eq!(compilation.state.bound_units.is_published(key), Ok(false));

    assert_eq!(
        compilation.state.checked_control_flow.is_published(key),
        Ok(false)
    );

    assert_eq!(
        compilation.state.expression_semantics.is_published(key),
        Ok(false)
    );

    assert_eq!(
        compilation.state.body_semantics.is_published(key),
        Ok(false)
    );

    assert_eq!(
        compilation.state.checked_body_behaviors.is_published(key),
        Ok(false)
    );

    let first = compilation.check_diagnostics();
    let second = compilation.check_diagnostics();

    assert!(std::ptr::eq(first, second));
    assert_eq!(compilation.state.bound_units.is_published(key), Ok(true));

    assert_eq!(
        compilation.state.checked_control_flow.is_published(key),
        Ok(true)
    );

    assert_eq!(
        compilation.state.expression_semantics.is_published(key),
        Ok(true)
    );

    assert_eq!(compilation.state.body_semantics.is_published(key), Ok(true));

    assert_eq!(
        compilation.state.checked_body_behaviors.is_published(key),
        Ok(true)
    );

    assert_eq!(
        diagnostic_kinds(first),
        [DiagnosticKind::BindingNameAlreadyDefined]
    );
}

#[test]
fn check_diagnostics_include_expression_target_validity() {
    let compilation = compilation(
        r#"module app;

func main(value: r16)
{
    value;
}
"#,
    );

    assert!(
        diagnostic_kinds(compilation.check_diagnostics())
            .contains(&DiagnosticKind::CheckingTargetRepresentationUnavailable)
    );
}

#[test]
fn check_diagnostics_request_embedded_constant_expressions() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main(pos source: [i32; false])\n",
        "{\n",
        "    let copy: [i32; false] = source;\n",
        "}\n",
    ));

    let keys = compilation
        .declared_unit_keys_for_test()
        .unwrap_or_else(|error| panic!("callable unit must be discoverable: {error:?}"));

    let [key] = keys.as_slice() else {
        panic!("test source must produce one callable unit");
    };

    let declared = compilation
        .declared_value_type_templates(key.clone())
        .unwrap_or_else(|error| panic!("declared types must publish: {error:?}"));

    let occurrence = declared
        .value()
        .evidence()
        .iter()
        .flat_map(|evidence| evidence.template().constant_expressions())
        .next()
        .unwrap_or_else(|| panic!("array type must retain its length expression"));

    let checked = compilation
        .embedded_constant_term(occurrence)
        .unwrap_or_else(|error| panic!("embedded constant must recover: {error:?}"));

    assert!(!checked.diagnostics().is_empty());

    assert!(
        diagnostic_kinds(compilation.check_diagnostics())
            .contains(&DiagnosticKind::CheckingIncompatibleExpressionType)
    );
}

#[test]
fn bare_generic_constant_arguments_bind_as_embedded_values() {
    let compilation = compilation(concat!(
        "module app;\n",
        "struct Buffer<const size: usize>\n",
        "{\n",
        "}\n",
        "func use_buffer<const count: usize>(pos value: Buffer<count>)\n",
        "{\n",
        "}\n",
    ));

    let keys = compilation
        .declared_unit_keys_for_test()
        .unwrap_or_else(|error| panic!("callable unit must be discoverable: {error:?}"));

    let key = keys
        .into_iter()
        .find(|key| key.kind() == BoundUnitKind::CallableBody)
        .unwrap_or_else(|| panic!("test source must produce one callable body"));

    let declared = compilation
        .declared_value_type_templates(key)
        .unwrap_or_else(|error| panic!("declared types must publish: {error:?}"));

    let occurrence = declared
        .value()
        .evidence()
        .iter()
        .flat_map(|evidence| evidence.template().constant_expressions())
        .next()
        .unwrap_or_else(|| panic!("generic argument must retain its constant expression"));

    let checked = compilation
        .embedded_constant_term(occurrence)
        .unwrap_or_else(|error| panic!("embedded constant must bind: {error:?}"));

    let term = compilation
        .semantic_value_store()
        .unwrap_or_else(|error| panic!("semantic values must publish: {error:?}"))
        .constant_term_data(*checked.value());

    assert!(checked.diagnostics().is_empty());

    assert!(matches!(term.as_ref(), ConstantTermData::Parameter(_)));
}

#[test]
fn malformed_calls_to_known_functions_publish_diagnostics_without_panicking() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func known(pos value: i32)\n",
        "{\n",
        "}\n",
        "func main()\n",
        "{\n",
        "    known(value = );\n",
        "}\n",
    ));

    assert!(!compilation.check_diagnostics().is_empty());
}

#[test]
fn package_diagnostics_include_nested_unit_diagnostics() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let callable = lambda()\n",
        "    {\n",
        "        let value = 1;\n",
        "        let value = 2;\n",
        "    };\n",
        "}\n",
    ));

    assert_eq!(
        diagnostic_kinds(compilation.check_diagnostics()),
        [DiagnosticKind::BindingNameAlreadyDefined]
    );
}

#[test]
fn package_diagnostics_bind_every_contract_clause_expression() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func check() requires(true, missing)\n",
        "{\n",
        "}\n",
    ));

    let keys = match compilation.declared_unit_keys_for_test() {
        Ok(keys) => keys,
        Err(error) => panic!("contract-clause key must be discoverable: {error:?}"),
    };

    let Some(key) = keys
        .into_iter()
        .find(|key| key.kind() == BoundUnitKind::ContractClause)
    else {
        panic!("test source must produce a contract-clause key");
    };

    assert_eq!(
        diagnostic_kinds(compilation.check_diagnostics()),
        [DiagnosticKind::BindingUnresolvedName],
        "{:#?}",
        compilation.check_diagnostics()
    );

    let bound = match compilation.bound_unit(key) {
        Ok(bound) => bound,
        Err(error) => panic!("contract clause must bind: {error:?}"),
    };

    let BoundUnitRoot::ExpressionSequence(root) = bound.value().root() else {
        panic!("contract clause must publish an expression sequence");
    };

    let Some(sequence) = bound.value().tree().block(root) else {
        panic!("contract-clause sequence root must resolve");
    };

    assert_eq!(sequence.items().len(), 2);
}

#[test]
fn postcondition_result_reaches_the_semantic_unit_context() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func check() -> i32 ensures(result == 1)\n",
        "{\n",
        "    return 1;\n",
        "}\n",
    ));

    let keys = match compilation.declared_unit_keys_for_test() {
        Ok(keys) => keys,
        Err(error) => panic!("contract-clause key must be discoverable: {error:?}"),
    };

    let Some(key) = keys
        .into_iter()
        .find(|key| key.kind() == BoundUnitKind::ContractClause)
    else {
        panic!("test source must produce a contract-clause key");
    };

    let bound = match compilation.bound_unit(key) {
        Ok(bound) => bound,
        Err(error) => panic!("contract clause must bind: {error:?}"),
    };

    assert!(bound.diagnostics().is_empty());

    let symbols = match compilation.symbol_graph() {
        Ok(symbols) => symbols,
        Err(error) => panic!("symbol graph must be available: {error:?}"),
    };

    let entry = semantic_unit_context(symbols, bound.value());

    let SemanticUnitContext::ContractClause(entry) = entry else {
        panic!("contract clause must produce a contract-clause checker entry");
    };

    let [result] = bound.value().local_symbols().postcondition_results() else {
        panic!("value-producing postcondition must declare one result symbol");
    };

    assert_eq!(entry.kind(), CallableContractClauseKind::Ensures);
    assert_eq!(entry.result(), Some(result.id()));
}

#[test]
fn contract_clause_entries_preserve_kind_and_exact_result_availability() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func omitted() ensures(true)\n",
        "{\n",
        "}\n",
        "func unit_result() -> unit ensures(true)\n",
        "{\n",
        "}\n",
        "func never_result() -> never ensures(true)\n",
        "{\n",
        "}\n",
        "func value_result() -> i32 requires(true) ensures(result == 1) with(true)\n",
        "{\n",
        "    return 1;\n",
        "}\n",
    ));

    let keys = match compilation.declared_unit_keys_for_test() {
        Ok(keys) => keys,
        Err(error) => panic!("contract-clause keys must be discoverable: {error:?}"),
    };

    let symbols = match compilation.symbol_graph() {
        Ok(symbols) => symbols,
        Err(error) => panic!("symbol graph must be available: {error:?}"),
    };

    let mut entries = Vec::new();

    for key in keys
        .into_iter()
        .filter(|key| key.kind() == BoundUnitKind::ContractClause)
    {
        let source_start = key.source().syntax().full_range().start();

        let bound = match compilation.bound_unit(key) {
            Ok(bound) => bound,
            Err(error) => panic!("contract clause must bind: {error:?}"),
        };

        assert!(bound.diagnostics().is_empty());

        let entry = semantic_unit_context(symbols, bound.value());

        let SemanticUnitContext::ContractClause(entry) = entry else {
            panic!("contract clause must produce a contract-clause checker entry");
        };

        entries.push((source_start, entry.kind(), entry.result().is_some()));
    }

    entries.sort_unstable_by_key(|entry| entry.0);

    assert_eq!(
        entries
            .into_iter()
            .map(|(_, kind, has_result)| (kind, has_result))
            .collect::<Vec<_>>(),
        [
            (CallableContractClauseKind::Ensures, false),
            (CallableContractClauseKind::Ensures, false),
            (CallableContractClauseKind::Ensures, false),
            (CallableContractClauseKind::Requires, false),
            (CallableContractClauseKind::Ensures, true),
        ]
    );
}

#[test]
fn declarations_without_semantic_units_do_not_force_binding() {
    let compilation = compilation(concat!(
        "module app;\n",
        "extern func write(value: i32);\n",
        "trait Writer\n",
        "{\n",
        "    func flush();\n",
        "}\n",
    ));

    assert!(
        compilation
            .declared_unit_keys_for_test()
            .is_ok_and(|keys| keys.is_empty())
    );

    assert!(compilation.check_diagnostics().is_empty());
}

#[test]
fn malformed_predicate_parameters_stop_before_signature_binding() {
    let compilation = compilation(
        "trusted module app;\ntrusted predicate readable(pos pointer: DataPointer<u8>);\n",
    );

    assert!(compilation.syntax_tree_result().diagnostics().has_errors());
    assert!(compilation.check_diagnostics().has_errors());
}

#[test]
fn malformed_bodies_publish_recovered_semantics_without_panicking() {
    let cases = [
        concat!(
            "module app;\n",
            "func missing_initializer()\n",
            "{\n",
            "    let value = ;\n",
            "}\n",
        ),
        concat!(
            "module app;\n",
            "func malformed_pattern()\n",
            "{\n",
            "    let (first, second = 1;\n",
            "}\n",
        ),
    ];

    for source in cases {
        let compilation = compilation(source);

        let keys = match compilation.declared_unit_keys_for_test() {
            Ok(keys) => keys,
            Err(error) => panic!("recovered unit keys must be discoverable: {error:?}"),
        };

        let mut recovered = false;

        assert!(!keys.is_empty(), "{source}");

        for key in keys {
            let checked = match compilation.control_flow(key) {
                Ok(checked) => checked,
                Err(error) => panic!("recovered semantic check failed: {source}: {error:?}"),
            };

            recovered |= checked.value().is_recovered();
        }

        assert!(recovered, "{source}");
        assert!(!compilation.check_diagnostics().is_empty(), "{source}");
    }
}

#[test]
fn concurrent_package_diagnostic_requests_publish_one_cached_result() {
    let compilation = compilation(concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    missing;\n",
        "}\n",
    ));

    let gate = FactTestGate::holding(FactCellTestEvent::Computing);

    if let Err(error) = compilation
        .state
        .check_diagnostics
        .set_test_observer(gate.observer())
    {
        panic!("package diagnostic source must accept a test observer: {error:?}");
    }

    let diagnostics = std::thread::scope(|scope| {
        let owner = scope.spawn(|| compilation.check_diagnostics());

        gate.wait_until_observed(FactCellTestEvent::Computing, 1);

        let waiter = scope.spawn(|| compilation.check_diagnostics());

        gate.wait_until_observed(FactCellTestEvent::Waiting, 1);
        gate.release();

        [owner, waiter].map(|handle| match handle.join() {
            Ok(diagnostics) => diagnostics,
            Err(_) => panic!("package diagnostic request panicked"),
        })
    });

    assert!(
        diagnostics
            .iter()
            .skip(1)
            .all(|result| std::ptr::eq(diagnostics[0], *result))
    );

    assert_eq!(
        diagnostic_kinds(diagnostics[0]),
        [DiagnosticKind::BindingUnresolvedName]
    );
}

#[test]
fn semantic_diagnostics_ignore_serial_parallel_and_reversed_demand_order() {
    let sources = [
        concat!(
            "module app;\n",
            "func first()\n",
            "{\n",
            "    let value = 1;\n",
            "    let value = 2;\n",
            "}\n",
        ),
        concat!(
            "module app;\n",
            "func second()\n",
            "{\n",
            "    missing;\n",
            "}\n",
        ),
    ];

    let serial = compilation_with_sources_and_worker_budget(&sources, WorkerBudget::serial());

    let serial_keys = match serial.declared_unit_keys_for_test() {
        Ok(keys) => keys,
        Err(error) => panic!("serial unit keys must be discoverable: {error:?}"),
    };

    for key in serial_keys {
        if let Err(error) = serial.control_flow(key) {
            panic!("serial semantic demand failed: {error:?}");
        }
    }

    let parallel_budget = match WorkerBudget::new(4) {
        Ok(budget) => budget,
        Err(error) => panic!("parallel test budget must be valid: {error:?}"),
    };

    let parallel = compilation_with_sources_and_worker_budget(&sources, parallel_budget);

    let mut parallel_keys = match parallel.declared_unit_keys_for_test() {
        Ok(keys) => keys,
        Err(error) => panic!("parallel unit keys must be discoverable: {error:?}"),
    };

    parallel_keys.reverse();

    assert_eq!(parallel_keys.len(), 2);

    let gates = parallel_keys
        .iter()
        .map(|key| {
            let gate = FactTestGate::holding(FactCellTestEvent::Computing);

            if let Err(error) = parallel
                .state
                .checked_control_flow
                .set_test_observer(key, gate.observer())
            {
                panic!("control-flow source must accept a test observer: {error:?}");
            }

            gate
        })
        .collect::<Vec<_>>();

    // Bound-unit keys share immutable identity storage across worker requests.
    std::thread::scope(|scope| {
        let parallel = &parallel;

        let handles = parallel_keys
            .into_iter()
            .map(|key| scope.spawn(move || parallel.control_flow(key)))
            .collect::<Vec<_>>();

        for gate in &gates {
            gate.wait_until_observed(FactCellTestEvent::Computing, 1);
        }

        for (gate, handle) in gates.iter().zip(handles) {
            gate.release();

            match handle.join() {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => panic!("parallel semantic demand failed: {error:?}"),
                Err(_) => panic!("parallel semantic demand panicked"),
            }
        }
    });

    assert_eq!(serial.check_diagnostics(), parallel.check_diagnostics());

    assert_eq!(
        rendered_diagnostics(serial.check_diagnostics()),
        rendered_diagnostics(parallel.check_diagnostics())
    );

    assert_eq!(
        diagnostic_kinds(serial.check_diagnostics()),
        [
            DiagnosticKind::BindingNameAlreadyDefined,
            DiagnosticKind::BindingUnresolvedName,
        ]
    );
}

fn rendered_diagnostics(diagnostics: &DiagnosticBag) -> Vec<RenderedDiagnostic> {
    diagnostics
        .iter()
        .map(|diagnostic| DiagnosticRenderer::english().render(diagnostic))
        .collect()
}
