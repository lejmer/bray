use bray_symbols::{PackageIdentity};
use crate::{Compilation, CompilationRequest, WorkerBudget};
use crate::test_support::{ProductionFixtureCache, production_fixture, production_native_link_inputs, production_source_inputs};
use std::sync::{OnceLock};

/// Shared declarations for pointer-bound receipt fixtures.
pub(super) const RECEIPT_DECLARATIONS: &str = r#"
using core.memory;

struct Marker
{
}

struct Receipt
{
    marker: Marker;
    pointer: DataPointer<u32>;
}

trusted predicate held(owner: &Marker);
"#;

/// A producer for tests that declare an Owner and its live predicate.
pub(super) const LIVE_OWNER_PRODUCER: &str = r#"
trusted func owner() -> Owner
    ensures(trusted live(&result))
{
    return
    {
        epoch = 1
    };
}
"#;

pub(super) fn foreign_compilation(source: &str) -> Compilation {
    crate::test_support::compilation_with_options(source,
        crate::CompilationOptions::default().with_native_link_inputs([
            bray_symbols::NativeLinkRequirement::new(
                bray_base::NonEmptySharedStr::try_new("native").expect("fixture link name is nonempty"),
                bray_symbols::NativeLinkKind::Dynamic,
            ),
        ]))
}

pub(super) fn production_sync_compilation(caller: &str) -> Compilation {
    production_sync_compilation_for_target(caller, crate::SelectedTarget::baseline())
}

pub(super) fn production_sync_compilation_for_target(
    caller: &str,
    target: crate::SelectedTarget,
) -> Compilation {
    static FIXTURES: ProductionFixtureCache = OnceLock::new();

    production_fixture(&FIXTURES, caller, target, build_production_sync)
}

fn build_production_sync(caller: &str, target: crate::SelectedTarget) -> Compilation {
    let sources = production_source_inputs(&[
        "standard-library/std/src",
        "runtime/bootstrap/src/atomic.bray",
        "runtime/bootstrap/src/atomic",
    ], caller);

    let request = CompilationRequest::with_options(
        PackageIdentity::try_new("std").expect("fixture package identity must be valid"),
        sources,
        crate::CompilationOptions::new(
            WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            target,
        )
        .with_native_link_inputs(production_native_link_inputs([])),
    )
    .with_standard_library_source_authority();

    let compilation = Compilation::load(request).expect("fixture input must load");

    assert!(
        !compilation.syntax_tree_result().diagnostics().has_errors(),
        "{:?}",
        compilation.syntax_tree_result().diagnostics()
    );

    compilation
}

pub(super) fn body_belongs_to(
    compilation: &Compilation,
    key: &bray_bound_tree::BoundUnitKey,
    modules: &[&[&str]],
    types: &[&str],
) -> bool {
    let symbols = compilation
        .symbol_graph()
        .expect("fixture compilation must build its symbol graph");

    let mut symbol = symbols
        .symbol_for_key(key.declared_owner())
        .expect("declared body retains its symbol");

    let module = symbols
        .containing_module(symbol)
        .expect("declared body belongs to its module");

    if modules
        .iter()
        .any(|path| module.path().segments().eq(path.iter().copied()))
    {
        return true;
    }

    while let Some(owner) = symbols.containing_symbol(symbol) {
        if matches!(owner, bray_symbols::AnySymbolId::Struct(_))
            && symbols
                .member_name(owner)
                .is_some_and(|name| types.contains(&name.as_str()))
        {
            return true;
        }

        symbol = owner;
    }

    false
}

#[track_caller]
pub(super) fn assert_production_probe(caller: &str, valid: bool) {
    let compilation = production_host_compilation(caller);

    let checked = compilation
        .lowered_unit(crate::test_support::source_function_body_key(
            &compilation,
            "production_probe",
        ))
        .expect("fixture callable must lower successfully");

    bray_testing::assert_goal_state_diagnostic_outcome(
        checked.diagnostics(),
        (!valid).then_some(bray_diagnostics::DiagnosticKind::CheckingTrustedObligationNotProven),
        caller,
    );
}

pub(super) fn diagnostic_at_call(
    diagnostic: &bray_diagnostics::Diagnostic,
    source: bray_source::SourceId,
    caller: &str,
    call: &str,
) -> bool {
    let start = caller.rfind(call).expect("fixture contains the checked call");
    let end = start + caller[start..].find(';').expect("checked call ends in a semicolon");

    diagnostic.primary_span().is_some_and(|span| {
        span.source_id() == source && (span.start().bytes() as usize) < end
            && (span.end().bytes() as usize) > start
    })
}

fn condition_contains_predicate(condition: &bray_diagnostics::DiagnosticCondition, predicate: &str) -> bool {
    use bray_diagnostics::DiagnosticCondition;

    match condition {
        DiagnosticCondition::Application { path, arguments, .. } => {
            path.last().is_some_and(|name| name == predicate)
                || arguments.iter().any(|argument| condition_contains_predicate(argument, predicate))
        }
        DiagnosticCondition::Operation { operands, .. } => {
            operands.iter().any(|operand| condition_contains_predicate(operand, predicate))
        }
        _ => false,
    }
}

#[track_caller]
pub(super) fn assert_production_contract(
    caller: &str,
    expected: Option<bray_diagnostics::DiagnosticKind>,
    site: &str,
    predicate: &str,
) {
    use bray_diagnostics::{
        DiagnosticArgValue, DiagnosticContractObligationKind, DiagnosticKind, DiagnosticWitnessTransferKind,
    };

    let compilation = production_host_compilation(caller);
    let key = crate::test_support::source_function_body_key(&compilation, "production_probe");
    let source = key.source().syntax().source_id();
    let checked = compilation.lowered_unit(key).expect("fixture callable must lower successfully");
    let diagnostics = checked.diagnostics();

    bray_testing::assert_goal_state_diagnostic_outcome(diagnostics, expected, caller);

    let Some(expected) = expected else { return; };

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.kind() == expected
            && diagnostic_at_call(diagnostic, source, caller, site)
            && diagnostic.args().iter().any(|argument| match argument.value() {
                DiagnosticArgValue::ContractObligation(obligation)
                    if expected == DiagnosticKind::CheckingTrustedObligationNotProven => {
                    obligation.kind == DiagnosticContractObligationKind::CallRequirement
                        && condition_contains_predicate(&obligation.required, predicate)
                }
                DiagnosticArgValue::WitnessTransfer(transfer)
                    if expected == DiagnosticKind::CheckingTrustedWitnessTransferNotProven => {
                    transfer.kind == DiagnosticWitnessTransferKind::Separation
                        && condition_contains_predicate(&transfer.guarantee, predicate)
                }
                _ => false,
            })
    }), "expected {predicate} at {site}: {diagnostics:#?}");
}

pub(in crate::compilation) fn production_host_compilation(caller: &str) -> Compilation {
    production_host_compilation_for_target(caller, crate::SelectedTarget::baseline())
}

pub(in crate::compilation) fn production_host_compilation_for_target(
    caller: &str,
    target: crate::SelectedTarget,
) -> Compilation {
    static FIXTURES: ProductionFixtureCache = OnceLock::new();

    production_fixture(&FIXTURES, caller, target, |caller, target| {
        build_production_host(caller, target, WorkerBudget::serial())
    })
}

/// Whole-provider checks share one compiler pool instead of blocking serial caller probes.
pub(super) fn production_host_provider() -> Compilation {
    static PROVIDER: OnceLock<Compilation> = OnceLock::new();

    PROVIDER.get_or_init(|| build_production_host(
        "module app;", crate::SelectedTarget::baseline(), WorkerBudget::available_parallelism(),
    )).clone()
}

fn build_production_host(caller: &str, target: crate::SelectedTarget, workers: WorkerBudget) -> Compilation {
    let compilation = Compilation::load(production_host_request(caller, target, workers))
        .expect("actual production host declarations must load");

    assert!(
        !compilation.syntax_tree_result().diagnostics().has_errors(),
        "actual bootstrap module declarations must parse: {:?}",
        compilation.syntax_tree_result().diagnostics()
    );

    compilation
}

fn production_host_request(
    caller: &str,
    target: crate::SelectedTarget,
    workers: WorkerBudget,
) -> CompilationRequest {
    let sources = production_source_inputs(
        &["standard-library/std/src", "runtime/bootstrap/src"], caller,
    );

    CompilationRequest::with_options(
        PackageIdentity::try_new("std").expect("standard library identity must be valid"),
        sources,
        crate::CompilationOptions::new(
            workers,
            bray_symbols::ProductKind::Library,
            target,
        )
        .with_native_link_inputs(production_native_link_inputs(["bray_runtime_host"])),
    )
    .with_standard_library_source_authority()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{production_host_compilation, production_host_request};
    use crate::test_support::source_function_body_key;
    use crate::{
        Compilation, CompilationProfileConfiguration, CompilationProfileMode, SelectedTarget, WorkerBudget,
    };

    #[test]
    #[ignore = "tier:proof"]
    fn production_fixture_shares_completed_proofs() {
        let first = production_host_compilation("module app;");
        let key = source_function_body_key(&first, "host_access");

        let first_proof = first
            .execution_properties(key.clone())
            .expect("production proof");

        let second = production_host_compilation("module app;");

        let second_proof = second
            .execution_properties(key)
            .expect("reused production proof");

        assert!(Arc::ptr_eq(&first_proof, &second_proof));
    }

    #[test]
    #[ignore = "tier:proof"]
    fn production_fixture_phase_timings() {
        let request = production_host_request("module app;", SelectedTarget::baseline(), WorkerBudget::available_parallelism())
            .with_profile(CompilationProfileConfiguration::new(
                CompilationProfileMode::Summary,
            ));

        let compilation = Compilation::load(request).expect("profiled production fixture");

        assert!(!compilation.syntax_tree_result().diagnostics().has_errors());
        checkpoint(&compilation, "parse");
        compilation.symbol_graph().expect("production symbols");
        checkpoint(&compilation, "bind");
        compilation.check_diagnostics();
        checkpoint(&compilation, "check");

        compilation
            .lowered_unit(source_function_body_key(&compilation, "host_access"))
            .expect("production lowering");

        checkpoint(&compilation, "lower");

        let before = query_evaluations(&compilation);

        compilation.check_diagnostics();

        compilation
            .symbol_graph()
            .expect("cached production symbols");

        compilation
            .lowered_unit(source_function_body_key(&compilation, "host_access"))
            .expect("cached production lowering");

        assert_eq!(
            query_evaluations(&compilation),
            before,
            "repeated production demands must perform no additional query evaluations"
        );

        checkpoint(&compilation, "reuse");
    }

    fn query_evaluations(compilation: &Compilation) -> u64 {
        compilation
            .profile_report()
            .expect("fixture profiling enabled")
            .queries
            .iter()
            .map(|query| query.evaluations)
            .sum()
    }

    fn checkpoint(compilation: &Compilation, phase: &str) {
        let report = compilation
            .profile_report()
            .expect("fixture profiling enabled");

        if let Some(directory) = std::env::var_os("BRAY_TEST_PROFILE_OUTPUT") {
            let directory = std::path::PathBuf::from(directory);

            std::fs::create_dir_all(&directory).expect("profile report directory");

            std::fs::write(
                directory.join(format!("production-{phase}.json")),
                serde_json::to_vec_pretty(&report).expect("profile encodes"),
            )
            .expect("profile report writes");
        }
    }
}
