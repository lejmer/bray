use std::collections::BTreeMap;
use std::sync::OnceLock;

use bray_bound_tree::{BoundUnitKey, BoundUnitKind};
use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
use bray_symbols::PackageIdentity;

use crate::test_support::{
    ProductionFixtureCache, production_fixture, production_native_link_inputs,
    production_source_inputs, source_function_body_key,
};
use crate::{Compilation, CompilationRequest, WorkerBudget};

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
    let key = source_function_body_key(&compilation, "production_probe");
    let expected = (!valid).then_some(DiagnosticKind::CheckingTrustedObligationNotProven);
    let diagnostics = production_probe_diagnostics(&compilation, key, expected, check_probe_storage);

    bray_testing::assert_goal_state_diagnostic_outcome(&diagnostics, expected, caller);
}

pub(super) use crate::test_support::diagnostic_at_call;

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
    expected: Option<DiagnosticKind>,
    site: &str,
    predicate: &str,
) {
    let compilation = production_host_compilation(caller);
    let key = source_function_body_key(&compilation, "production_probe");

    assert_contract_case(&compilation, key, caller, expected, site, predicate);
}

#[track_caller]
pub(super) fn assert_production_contracts<'case>(
    cases: impl Iterator<Item = (String, Option<DiagnosticKind>, &'case str, &'case str)>,
) {
    let cases = cases.collect::<Vec<_>>();
    let sources = cases.iter().map(|(caller, ..)| caller.as_str()).collect::<Vec<_>>();
    let baseline = production_host_compilation("module app;");
    let compilation = crate::test_support::with_fixture_callers(&baseline, &sources);

    for (case, (caller, expected, site, predicate)) in cases.into_iter().enumerate() {
        let key = source_function_body_key(&compilation, &format!("production_probe_{case}"));

        assert_contract_case(&compilation, key, &caller, expected, site, predicate);
    }
}

#[track_caller]
fn assert_contract_case(
    compilation: &Compilation,
    key: BoundUnitKey,
    caller: &str,
    expected: Option<DiagnosticKind>,
    site: &str,
    predicate: &str,
) {
    use bray_diagnostics::{
        DiagnosticArgValue, DiagnosticContractObligationKind, DiagnosticWitnessTransferKind,
    };

    let source = key.source().syntax().source_id();
    let diagnostics = production_probe_diagnostics(compilation, key, expected, check_probe_storage);

    bray_testing::assert_goal_state_diagnostic_outcome(&diagnostics, expected, caller);

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

pub(super) fn assert_production_probes(
    expected: &[(&str, Option<DiagnosticKind>)],
    expected_sites: &[(&str, &str)],
    check_rejection: fn(&Compilation, BoundUnitKey) -> DiagnosticBag,
) {
    let compilation = production_probe_compilation();

    let (first, _) = expected.first().expect("production probe test includes a callable");

    let caller = source_function_body_key(compilation, first).source().syntax().source_id();
    let source = compilation.sources().text(caller).expect("probe body retains its source");

    let symbols = compilation.symbol_graph().expect("production probe declarations");

    let probes = compilation.declared_unit_keys_for_test().expect("production probe units").into_iter()
        .filter(|key| key.kind() == BoundUnitKind::CallableBody
            && key.source().syntax().source_id() == caller)
        .collect::<Vec<_>>();

    let mut expected = expected.iter().copied().collect::<BTreeMap<_, _>>();

    assert_eq!(probes.len(), expected.len(), "every probe retains its expected outcome");

    for key in probes {
        let name = symbols.symbol_for_key(key.declared_owner())
            .and_then(|symbol| symbols.member_name(symbol)).expect("probe function name").as_str();

        let expected = expected.remove(name)
            .expect("each declared probe has an expected outcome");

        let diagnostics = production_probe_diagnostics(compilation, key, expected, check_rejection);

        bray_testing::assert_goal_state_diagnostic_outcome(&diagnostics, expected, name);

        if let Some((_, site)) = expected_sites.iter().find(|(probe, _)| *probe == name) {
            assert!(diagnostics.iter().any(|diagnostic| {
                Some(diagnostic.kind()) == expected && diagnostic_at_call(diagnostic, caller, source, site)
            }), "{name}, {site}: {diagnostics:?}");
        }

    }
}

pub(super) fn check_probe_storage(compilation: &Compilation, key: BoundUnitKey) -> DiagnosticBag {
    let checked = compilation.storage_flow(key).expect("production probe reaches the storage checker");

    checked.diagnostics().clone()
}

fn production_probe_diagnostics(
    compilation: &Compilation,
    key: BoundUnitKey,
    expected: Option<DiagnosticKind>,
    check_rejection: fn(&Compilation, BoundUnitKey) -> DiagnosticBag,
) -> DiagnosticBag {
    if expected.is_some() {
        return check_rejection(compilation, key);
    }

    let lowered = compilation
        .lowered_unit(key)
        .expect("accepted production probe reaches lowering");

    assert!(
        lowered.value().is_some() || lowered.diagnostics().has_errors(),
        "accepted production probe must produce a lowered body"
    );

    lowered.diagnostics().clone()
}

pub(in crate::compilation) fn production_host_compilation_for_target(
    caller: &str,
    target: crate::SelectedTarget,
) -> Compilation {
    static FIXTURES: ProductionFixtureCache = OnceLock::new();

    production_fixture(&FIXTURES, caller, target, |caller, target| {
        build_production_host(production_source_inputs(
            &["standard-library/std/src", "runtime/bootstrap/src"], caller,
        ), target, WorkerBudget::serial())
    })
}

fn production_probe_compilation() -> &'static Compilation {
    static PROBES: OnceLock<Compilation> = OnceLock::new();

    PROBES.get_or_init(|| {
        let sources = production_source_inputs(&[
            "standard-library/std/src",
            "runtime/bootstrap/src",
            "xtask/fixtures/native-execution/contracts",
            "crates/bray-compilation/src/compilation/trusted_contracts/contract_tests/ownership-fixtures",
        ], "module app;");

        build_production_host(sources, crate::SelectedTarget::baseline(), WorkerBudget::available_parallelism())
    })
}

fn build_production_host(
    sources: Vec<bray_source::SourceInput>,
    target: crate::SelectedTarget,
    workers: WorkerBudget,
) -> Compilation {
    let request = CompilationRequest::with_options(
        PackageIdentity::try_new("std").expect("standard library identity must be valid"),
        sources,
        crate::CompilationOptions::new(
            workers,
            bray_symbols::ProductKind::Library,
            target,
        )
        .with_native_link_inputs(production_native_link_inputs(["bray_runtime_host"])),
    )
    .with_standard_library_source_authority();

    let compilation = Compilation::load(request).expect("actual production host declarations must load");

    assert!(!compilation.syntax_tree_result().diagnostics().has_errors(),
        "actual bootstrap module declarations must parse: {:?}", compilation.syntax_tree_result().diagnostics());

    compilation
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::production_host_compilation;
    use crate::test_support::source_function_body_key;

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

}
