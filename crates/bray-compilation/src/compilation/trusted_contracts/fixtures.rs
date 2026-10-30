use std::sync::OnceLock;

use bray_symbols::PackageIdentity;

use crate::test_support::{ProductionFixtureCache, production_fixture, source_input};
use crate::{Compilation, CompilationRequest};

pub(in crate::compilation) fn production_host_compilation(caller: &str) -> Compilation {
    production_host_compilation_for_target(caller, crate::SelectedTarget::baseline())
}

pub(in crate::compilation) fn production_host_compilation_for_target(
    caller: &str,
    target: crate::SelectedTarget,
) -> Compilation {
    static FIXTURES: ProductionFixtureCache = OnceLock::new();

    production_fixture(&FIXTURES, caller, target, build_production_host)
}

fn build_production_host(caller: &str, target: crate::SelectedTarget) -> Compilation {
    let compilation = Compilation::load(production_host_request(caller, target))
        .expect("actual production host declarations must load");

    assert!(
        !compilation.syntax_tree_result().diagnostics().has_errors(),
        "actual bootstrap module declarations must parse: {:?}",
        compilation.syntax_tree_result().diagnostics()
    );

    compilation
}

fn production_host_request(caller: &str, target: crate::SelectedTarget) -> CompilationRequest {
    let sources = [
        include_str!("../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../standard-library/std/src/numeric/policy.bray"),
        include_str!("../../../../../standard-library/std/src/memory/pointer.bray"),
        include_str!("../../../../../standard-library/std/src/memory/storage.bray"),
        include_str!("../../../../../standard-library/std/src/memory/allocation.bray"),
        include_str!("../../../../../standard-library/std/src/numeric/limits.bray"),
        include_str!("../../../../../standard-library/std/src/memory/buffer.bray"),
        include_str!("../../../../../standard-library/std/src/memory/function.bray"),
        include_str!("../../../../../standard-library/std/src/platform.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host.bray"),
        include_str!("../../../../../runtime/bootstrap/src/domain.bray"),
        include_str!("../../../../../runtime/bootstrap/src/provider.bray"),
        include_str!("../../../../../runtime/bootstrap/src/entry.bray"),
        include_str!("../../../../../runtime/bootstrap/src/entry/protocol.bray"),
        include_str!("../../../../../runtime/bootstrap/src/entry/execution.bray"),
        include_str!("../../../../../runtime/bootstrap/src/entry/bridge.bray"),
        include_str!("../../../../../runtime/bootstrap/src/entry/bridge/address.bray"),
        include_str!("../../../../../runtime/bootstrap/src/entry/bridge/control.bray"),
        include_str!("../../../../../runtime/bootstrap/src/entry/control.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/abi.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/admission.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/cleanup.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/control.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/execution.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/formation.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/operations.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/owner.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/progress.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/provider.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/provider_admission.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/provider_entry.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/provider_retirement.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/residency.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/state.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/storage.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/thread_domain.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/thread_cleanup.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/product/thread_registration.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/owner.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/access.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment/entry.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment/acquisition.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment/exit.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/attachment/cleanup.bray"),
        include_str!("../../../../../runtime/bootstrap/src/bootstrap.bray"),
        include_str!("../../../../../runtime/bootstrap/src/pool.bray"),
        include_str!("../../../../../runtime/bootstrap/src/pool/access.bray"),
        include_str!("../../../../../runtime/bootstrap/src/pool/growth.bray"),
        include_str!("../../../../../runtime/bootstrap/src/record_pool.bray"),
        include_str!("../../../../../runtime/bootstrap/src/records.bray"),
        include_str!("../../../../../runtime/bootstrap/src/panic.bray"),
        include_str!("../../../../../runtime/bootstrap/src/report/transfer.bray"),
        include_str!("../../../../../runtime/bootstrap/src/report.bray"),
        include_str!("../../../../../runtime/bootstrap/src/report/chain.bray"),
        include_str!("../../../../../runtime/bootstrap/src/report/consumer.bray"),
        include_str!("../../../../../standard-library/std/src/sync.bray"),
        include_str!("../../../../../standard-library/std/src/sync/spin_lock.bray"),
        include_str!("../../../../../standard-library/std/src/sync/spin_lock/state.bray"),
        include_str!("../../../../../standard-library/std/src/sync/tickets.bray"),
        include_str!("../../../../../standard-library/std/src/sync/spin_lock/lock.bray"),
        include_str!("../../../../../standard-library/std/src/sync/spin_lock/guard.bray"),
        include_str!("../../../../../runtime/bootstrap/src/thread.bray"),
        include_str!("../../../../../runtime/bootstrap/src/thread/destruction.bray"),
        include_str!("../../../../../runtime/bootstrap/src/thread/destruction/common.bray"),
        include_str!("../../../../../runtime/bootstrap/src/thread/destruction/unix.bray"),
        include_str!("../../../../../runtime/bootstrap/src/thread/destruction/windows.bray"),
        include_str!("../../../../../standard-library/std/src/thread.bray"),
        include_str!("../../../../../standard-library/std/src/thread/boundary.bray"),
        include_str!("../../../../../standard-library/std/src/thread/windows.bray"),
        include_str!("../../../../../standard-library/std/src/thread/unix.bray"),
        include_str!("../../../../../standard-library/std/src/thread/linux.bray"),
        include_str!("../../../../../standard-library/std/src/thread/darwin.bray"),
        include_str!("../../../../../standard-library/std/src/platform/status.bray"),
        include_str!("../../../../../standard-library/std/src/platform/representation.bray"),
        include_str!("../../../../../standard-library/std/src/time/units.bray"),
        include_str!("../../../../../standard-library/std/src/platform/linux.bray"),
        include_str!("../../../../../standard-library/std/src/platform/darwin.bray"),
        include_str!("../../../../../standard-library/std/src/platform/windows.bray"),
        include_str!("../../../../../runtime/bootstrap/src/memory/heap.bray"),
        include_str!("../../../../../standard-library/std/src/runtime/memory.bray"),
        include_str!("../../../../../standard-library/std/src/platform/heap.bray"),
        include_str!("../../../../../standard-library/std/src/platform/heap/windows.bray"),
        include_str!("../../../../../standard-library/std/src/platform/heap/unix.bray"),
        include_str!("../../../../../standard-library/std/src/os/windows.bray"),
        include_str!("../../../../../standard-library/std/src/os.bray"),
        include_str!("../../../../../standard-library/std/src/os/unix.bray"),
        include_str!("../../../../../standard-library/std/src/os/unix/thread.bray"),
        include_str!("../../../../../standard-library/std/src/os/unix/linux.bray"),
        include_str!("../../../../../standard-library/std/src/os/unix/darwin.bray"),
        include_str!("../../../../../runtime/bootstrap/src/host/synchronization.bray"),
        include_str!(
            "../../../../../standard-library/std/src/os/generated/x86_64_pc_windows_msvc.bray"
        ),
        include_str!(
            "../../../../../standard-library/std/src/os/generated/aarch64_pc_windows_msvc.bray"
        ),
        include_str!(
            "../../../../../standard-library/std/src/os/generated/x86_64_unknown_linux_gnu.bray"
        ),
        include_str!(
            "../../../../../standard-library/std/src/os/generated/aarch64_unknown_linux_gnu.bray"
        ),
        include_str!(
            "../../../../../standard-library/std/src/os/generated/x86_64_apple_darwin.bray"
        ),
        include_str!(
            "../../../../../standard-library/std/src/os/generated/aarch64_apple_darwin.bray"
        ),
        include_str!("../../../../../runtime/bootstrap/src/report/output.bray"),
        include_str!("../../../../../runtime/bootstrap/src/report/incident.bray"),
        include_str!("../../../../../runtime/bootstrap/src/report/sink.bray"),
        include_str!("../../../../../standard-library/std/src/io.bray"),
        include_str!("../../../../../standard-library/std/src/io/platform.bray"),
        include_str!("../../../../../standard-library/std/src/io/windows.bray"),
        include_str!("../../../../../standard-library/std/src/io/unix.bray"),
        caller,
        include_str!("../../../../../runtime/bootstrap/src/activation/data.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/completion.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/provider.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/abandonment.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/access.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/advance.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/completion.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/entry.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/retirement.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/run/step.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/storage.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/access.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/inactive.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/transition.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/composition.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/callback.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/execution.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/execution/access.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/execution/invoke.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/execution/outcome.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/requirements.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/requirements/inactive.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/requirements/active.bray"),
        include_str!("../../../../../runtime/bootstrap/src/activation/requirements/state.bray"),
        include_str!("../../../../../runtime/bootstrap/src/storage/data.bray"),
        include_str!("../../../../../runtime/bootstrap/src/storage/admission.bray"),
        include_str!("../../../../../runtime/bootstrap/src/storage/activation.bray"),
        include_str!("../../../../../runtime/bootstrap/src/storage/boundary.bray"),
        include_str!("../../../../../runtime/bootstrap/src/storage/bundle.bray"),
        include_str!("../../../../../runtime/bootstrap/src/outgoing/admission.bray"),
        include_str!("../../../../../runtime/bootstrap/src/outgoing/activation.bray"),
    ];

    CompilationRequest::with_options(
        PackageIdentity::try_new("std").expect("standard library identity must be valid"),
        sources
            .iter()
            .enumerate()
            .map(|(index, source)| {
                source_input(
                    source,
                    u32::try_from(index).expect("host source index must fit"),
                )
            })
            .collect(),
        crate::CompilationOptions::new(
            crate::WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            target,
        )
        .with_native_link_inputs(["c", "pthread", "Kernel32", "bray_runtime_host"].map(
            |name| {
                bray_symbols::NativeLinkRequirement::new(
                    bray_base::NonEmptySharedStr::try_new(name).expect("native test library name"),
                    bray_symbols::NativeLinkKind::System,
                )
            },
        )),
    )
    .with_standard_library_source_authority()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{production_host_compilation, production_host_request};
    use crate::test_support::source_function_body_key;
    use crate::{
        Compilation, CompilationProfileConfiguration, CompilationProfileMode, SelectedTarget,
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
        let request = production_host_request("module app;", SelectedTarget::baseline())
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
