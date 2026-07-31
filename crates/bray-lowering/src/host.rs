use bray_bound_tree::BoundUnitKey;
use bray_ir::{
    MirBlockKind, MirHostOperation, MirOperationKind, MirRuntimeReference, MirSourceAnchor,
    MirTargetFacts, MirTerminatorKind, MirUnit, MirUnitBuildError, MirUnitBuilder, MirUnitId,
};
use bray_runtime_interface::{ExecutableHostContract, RuntimeAbiRole};

/// Complete synthetic input for lowering a compiler-generated executable host stub.
///
/// The host is not represented as a bound source unit. Its validated contract already names the
/// root mode, private ABI roles, distinguished main-thread lane, and structured shutdown policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutableHostLoweringInput {
    unit: MirUnitId,
    root: BoundUnitKey,
    contract: ExecutableHostContract,
    target: MirTargetFacts,
}

impl ExecutableHostLoweringInput {
    /// Creates synthetic lowering input for one selected executable or test product.
    pub const fn new(
        unit: MirUnitId,
        root: BoundUnitKey,
        contract: ExecutableHostContract,
        target: MirTargetFacts,
    ) -> Self {
        Self {
            unit,
            root,
            contract,
            target,
        }
    }
}

/// Lowers one validated product host contract into explicit executable-host MIR.
pub fn lower_executable_host(
    input: ExecutableHostLoweringInput,
) -> Result<MirUnit, MirUnitBuildError> {
    let ExecutableHostLoweringInput {
        unit,
        root,
        contract,
        target,
    } = input;

    // The generated source anchor owns the same immutable product identity as the host contract.
    let source = MirSourceAnchor::executable_host(contract.product().clone());
    let execution = contract.root();
    let runtime_abi = target.runtime_abi();

    let root_role = if execution == bray_runtime_interface::RootExecution::Synchronous
        && contract
            .requirements()
            .roles()
            .contains(&RuntimeAbiRole::SynchronousRootExecution)
    {
        RuntimeAbiRole::SynchronousRootExecution
    } else {
        RuntimeAbiRole::RootExecution
    };

    let entry_error = match contract.entry_result() {
        bray_runtime_interface::ExecutableEntryResult::Fallible { error, .. } => Some(error),
        bray_runtime_interface::ExecutableEntryResult::Unit
        | bray_runtime_interface::ExecutableEntryResult::I32 => None,
    };

    let mut builder = MirUnitBuilder::for_executable_host(unit, contract, target);

    let entry = builder.push_block(source.clone(), MirBlockKind::Ordinary)?;

    let operations = [
        MirHostOperation::ExecuteRoot {
            root,
            execution,
            runtime: runtime_reference(root_role, runtime_abi),
        },
        MirHostOperation::ObserveRootTerminal {
            runtime: runtime_reference(RuntimeAbiRole::RootTerminalObservation, runtime_abi),
        },
        MirHostOperation::ResolveRootTerminal {
            error: entry_error,
            completion: runtime_reference(RuntimeAbiRole::RootCompletionResolution, runtime_abi),
            panic: runtime_reference(RuntimeAbiRole::PanicReporting, runtime_abi),
            entry_failure: runtime_reference(RuntimeAbiRole::EntryFailureReporting, runtime_abi),
        },
        MirHostOperation::ReportCleanupIncidents {
            runtime: runtime_reference(RuntimeAbiRole::CleanupIncidentReporting, runtime_abi),
        },
        MirHostOperation::StructuredShutdown {
            runtime: runtime_reference(RuntimeAbiRole::StructuredShutdown, runtime_abi),
        },
    ];

    for operation in operations {
        builder.push_operation(
            entry,
            // Each immutable MIR operation retains the same generated source identity.
            source.clone(),
            MirOperationKind::Host(operation),
            None,
        )?;
    }

    builder.set_terminator(entry, source, MirTerminatorKind::Return(None))?;

    builder.finish(entry)
}

const fn runtime_reference(
    role: RuntimeAbiRole,
    abi_version: bray_runtime_interface::RuntimeAbiVersion,
) -> MirRuntimeReference {
    MirRuntimeReference::new(role, abi_version)
}

#[cfg(test)]
mod tests {
    use bray_ir::{
        MirBlockKind, MirHostOperation, MirOperationKind, MirSourceAnchor, MirTerminatorKind,
        MirUnitBuildError, MirUnitBuilder, MirUnitId, MirUnitKey, MirUnitKind,
    };
    use bray_runtime_interface::{
        ExecutableEntryResult, RootExecution, RuntimeAbiRole, RuntimeRoleImplementation,
    };
    use bray_symbols::{SymbolId, UnionVariantSymbolId};
    use bray_testing::{
        test_async_executable_host_contract, test_executable_host_contract, test_mir_target,
    };

    use super::ExecutableHostLoweringInput;

    #[test]
    fn host_input_creates_generated_mir_without_a_bound_unit() {
        let host = test_executable_host_contract();
        let product = host.product().clone();

        let root = bray_testing::test_bound_unit(90).key().clone();

        let input = ExecutableHostLoweringInput::new(
            MirUnitId::new(90),
            root.clone(),
            host.clone(),
            test_mir_target(),
        );

        let Ok(unit) = super::lower_executable_host(input) else {
            panic!("generated host MIR must validate");
        };

        assert_eq!(unit.key(), &MirUnitKey::ExecutableHost(product));
        assert_eq!(unit.kind(), &MirUnitKind::ExecutableHost(host));

        assert!(matches!(
            unit.operations().first().map(bray_ir::MirOperation::kind),
            Some(bray_ir::MirOperationKind::Host(
                bray_ir::MirHostOperation::ExecuteRoot {
                    root: actual,
                    ..
                }
            )) if actual == &root
        ));

        let host_operations = unit
            .operations()
            .iter()
            .map(bray_ir::MirOperation::kind)
            .collect::<Vec<_>>();

        assert!(matches!(
            host_operations.as_slice(),
            [
                MirOperationKind::Host(MirHostOperation::ExecuteRoot { .. }),
                MirOperationKind::Host(MirHostOperation::ObserveRootTerminal { .. }),
                MirOperationKind::Host(MirHostOperation::ResolveRootTerminal { .. }),
                MirOperationKind::Host(MirHostOperation::ReportCleanupIncidents { .. }),
                MirOperationKind::Host(MirHostOperation::StructuredShutdown { .. }),
            ]
        ));
    }

    #[test]
    fn asynchronous_host_retains_the_distinguished_main_thread_lane_contract() {
        let host = test_async_executable_host_contract();

        assert!(matches!(host.root(), RootExecution::Asynchronous { .. }));

        for role in [
            RuntimeAbiRole::MainThreadLaneStartup,
            RuntimeAbiRole::MainThreadLaneDrive,
        ] {
            assert_eq!(
                host.role_binding(role)
                    .map(bray_runtime_interface::RuntimeRoleBinding::implementation),
                Some(RuntimeRoleImplementation::BrayRuntime)
            );
        }

        let input = ExecutableHostLoweringInput::new(
            MirUnitId::new(92),
            bray_testing::test_bound_unit(92).key().clone(),
            host.clone(),
            test_mir_target(),
        );

        let Ok(unit) = super::lower_executable_host(input) else {
            panic!("asynchronous generated host MIR must validate");
        };

        assert_eq!(unit.kind(), &MirUnitKind::ExecutableHost(host));

        assert!(matches!(
            unit.operations().first().map(bray_ir::MirOperation::kind),
            Some(MirOperationKind::Host(MirHostOperation::ExecuteRoot {
                execution: RootExecution::Asynchronous { .. },
                ..
            }))
        ));
    }

    #[test]
    fn fallible_host_contract_types_are_reachable_from_finished_mir() {
        let result = bray_testing::test_mir_type();
        let error = bray_testing::test_mir_type();

        let success_variant = UnionVariantSymbolId::from_symbol_id(SymbolId::new(17));

        let host = bray_testing::test_executable_host_contract_with_result(
            ExecutableEntryResult::Fallible {
                ty: result,
                error,
                success_variant,
            },
        );

        let input = ExecutableHostLoweringInput::new(
            MirUnitId::new(93),
            bray_testing::test_bound_unit(93).key().clone(),
            host,
            test_mir_target(),
        );

        let unit = super::lower_executable_host(input)
            .unwrap_or_else(|error| panic!("fallible host MIR must validate: {error:?}"));

        assert!(unit.referenced_types().contains(&result));
        assert!(unit.referenced_types().contains(&error));
    }

    #[test]
    fn executable_host_validation_rejects_reordered_shutdown_operations() {
        let host = test_executable_host_contract();
        let source = MirSourceAnchor::executable_host(host.product().clone());
        let target = test_mir_target();
        let runtime_abi = target.runtime_abi();

        let mut builder = MirUnitBuilder::for_executable_host(MirUnitId::new(91), host, target);

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap_or_else(|error| panic!("host entry must be valid: {error:?}"));

        for operation in [
            MirHostOperation::ExecuteRoot {
                root: bray_testing::test_bound_unit(91).key().clone(),
                execution: RootExecution::Synchronous,
                runtime: super::runtime_reference(RuntimeAbiRole::RootExecution, runtime_abi),
            },
            MirHostOperation::ObserveRootTerminal {
                runtime: super::runtime_reference(
                    RuntimeAbiRole::RootTerminalObservation,
                    runtime_abi,
                ),
            },
            MirHostOperation::ReportCleanupIncidents {
                runtime: super::runtime_reference(
                    RuntimeAbiRole::CleanupIncidentReporting,
                    runtime_abi,
                ),
            },
            MirHostOperation::ResolveRootTerminal {
                error: None,
                completion: super::runtime_reference(
                    RuntimeAbiRole::RootCompletionResolution,
                    runtime_abi,
                ),
                panic: super::runtime_reference(RuntimeAbiRole::PanicReporting, runtime_abi),
                entry_failure: super::runtime_reference(
                    RuntimeAbiRole::EntryFailureReporting,
                    runtime_abi,
                ),
            },
            MirHostOperation::StructuredShutdown {
                runtime: super::runtime_reference(RuntimeAbiRole::StructuredShutdown, runtime_abi),
            },
        ] {
            builder
                .push_operation(
                    entry,
                    source.clone(),
                    MirOperationKind::Host(operation),
                    None,
                )
                .unwrap_or_else(|error| panic!("host operation must be valid: {error:?}"));
        }

        builder
            .set_terminator(entry, source, MirTerminatorKind::Return(None))
            .unwrap_or_else(|error| panic!("host return must be valid: {error:?}"));

        assert_eq!(
            builder.finish(entry),
            Err(MirUnitBuildError::InvalidHostSequence)
        );
    }
}
