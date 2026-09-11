use bray_bound_tree::BoundUnitKey;
use bray_ir::{
    MirBlockKind, MirHostOperation, MirOperationKind, MirRuntimeReference, MirSourceAnchor,
    MirTargetContract, MirTerminatorKind, MirUnit, MirUnitBuildError, MirUnitBuilder, MirUnitId,
};
use bray_runtime_interface::{ExecutableHostContract, ExecutableHostEntryId, RuntimeAbiRole};

/// One demanded product-static instance owned by a generated product host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutableHostStatic {
    reference: bray_symbols::StaticReferenceSelection,
    ty: bray_symbols::TypeId,
}

impl ExecutableHostStatic {
    /// Creates one closed product-static host entry.
    pub const fn new(
        reference: bray_symbols::StaticReferenceSelection,
        ty: bray_symbols::TypeId,
    ) -> Self {
        Self { reference, ty }
    }
}

/// Complete synthetic input for lowering a compiler-generated executable host stub.
///
/// The host is not represented as a bound source unit. Its validated contract already names the
/// root mode, private ABI roles, distinguished main-thread lane, and structured shutdown policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutableHostLoweringInput {
    unit: MirUnitId,
    roots: Vec<BoundUnitKey>,
    contract: ExecutableHostContract,
    target: MirTargetContract,
    statics: Vec<ExecutableHostStatic>,
}

impl ExecutableHostLoweringInput {
    /// Creates synthetic lowering input for one selected executable or test product.
    pub fn new(
        unit: MirUnitId,
        roots: impl IntoIterator<Item = BoundUnitKey>,
        contract: ExecutableHostContract,
        target: MirTargetContract,
    ) -> Self {
        Self {
            unit,
            roots: roots.into_iter().collect(),
            contract,
            target,
            statics: Vec::new(),
        }
    }

    /// Supplies product statics in deterministic cleanup order.
    pub fn with_statics(mut self, statics: impl IntoIterator<Item = ExecutableHostStatic>) -> Self {
        self.statics = statics.into_iter().collect();

        self
    }
}

/// Lowers one validated product host contract into explicit executable-host MIR.
pub fn lower_executable_host(
    input: ExecutableHostLoweringInput,
) -> Result<MirUnit, MirUnitBuildError> {
    let ExecutableHostLoweringInput {
        unit,
        roots,
        contract,
        target,
        statics,
    } = input;

    // The generated source anchor owns the same immutable product identity as the host contract.
    let source = MirSourceAnchor::executable_host(contract.product().clone());
    let runtime_abi = target.runtime_abi();

    if roots.len() != contract.entries().len() {
        return Err(MirUnitBuildError::InvalidHostSequence);
    }

    // The finished MIR owns the contract while lowering still reads each entry below.
    let mut builder = MirUnitBuilder::for_executable_host(unit, contract.clone(), target);

    let entry = builder.push_block(source.clone(), MirBlockKind::Ordinary)?;

    if contract
        .requirements()
        .requires_role(RuntimeAbiRole::ProductHostControl)
        && contract
            .requirements()
            .requires_role(RuntimeAbiRole::MainThreadLaneStartup)
    {
        builder.push_operation(
            entry,
            source.clone(),
            MirOperationKind::Host(MirHostOperation::BeginExecution {
                startup: runtime_reference(RuntimeAbiRole::MainThreadLaneStartup, runtime_abi),
                control: runtime_reference(RuntimeAbiRole::ProductHostControl, runtime_abi),
            }),
            None,
        )?;
    }

    for static_instance in statics.into_iter().rev() {
        let storage = builder.push_storage(
            source.clone(),
            bray_ir::MirStorageKind::Static(static_instance.reference),
            static_instance.ty,
        )?;

        builder.push_operation(
            entry,
            source.clone(),
            MirOperationKind::Host(MirHostOperation::MaterializeStatic {
                place: bray_ir::MirPlace::new(storage, [], static_instance.ty),
            }),
            None,
        )?;
    }

    for (index, (root, contract_entry)) in
        roots.into_iter().zip(contract.entries().iter()).enumerate()
    {
        let entry_index = ExecutableHostEntryId::new(
            u32::try_from(index).map_err(|_| MirUnitBuildError::InvalidHostSequence)?,
        );

        let execution = contract_entry.root();

        let root_role = if execution == bray_runtime_interface::RootExecution::Synchronous
            && contract
                .requirements()
                .requires_role(RuntimeAbiRole::SynchronousRootExecution)
        {
            RuntimeAbiRole::SynchronousRootExecution
        } else {
            RuntimeAbiRole::RootExecution
        };

        let entry_error = match contract_entry.result() {
            bray_runtime_interface::ExecutableEntryResult::Fallible { error, .. } => Some(error),
            bray_runtime_interface::ExecutableEntryResult::Unit
            | bray_runtime_interface::ExecutableEntryResult::I32 => None,
        };

        if contract
            .requirements()
            .requires_role(RuntimeAbiRole::TestEntrySelection)
        {
            builder.push_operation(
                entry,
                source.clone(),
                MirOperationKind::Host(MirHostOperation::SelectTestEntry {
                    entry: entry_index,
                    runtime: runtime_reference(RuntimeAbiRole::TestEntrySelection, runtime_abi),
                }),
                None,
            )?;
        }

        for operation in [
            MirHostOperation::ExecuteRoot {
                entry: entry_index,
                root,
                execution,
                runtime: runtime_reference(root_role, runtime_abi),
            },
            MirHostOperation::ObserveRootTerminal {
                entry: entry_index,
                runtime: runtime_reference(RuntimeAbiRole::RootTerminalObservation, runtime_abi),
            },
            MirHostOperation::ResolveRootTerminal {
                entry: entry_index,
                error: entry_error,
                completion: runtime_reference(
                    RuntimeAbiRole::RootCompletionResolution,
                    runtime_abi,
                ),
                panic: runtime_reference(RuntimeAbiRole::PanicReporting, runtime_abi),
                entry_failure: runtime_reference(
                    RuntimeAbiRole::EntryFailureResolution,
                    runtime_abi,
                ),
            },
        ] {
            builder.push_operation(
                entry,
                source.clone(),
                MirOperationKind::Host(operation),
                None,
            )?;
        }
    }

    builder.push_operation(
        entry,
        source.clone(),
        MirOperationKind::Host(MirHostOperation::BeginStaticCleanup),
        None,
    )?;

    builder.push_operation(
        entry,
        source.clone(),
        MirOperationKind::Host(MirHostOperation::ReportCleanupIncidents {
            runtime: runtime_reference(RuntimeAbiRole::CleanupIncidentReporting, runtime_abi),
        }),
        None,
    )?;

    builder.push_operation(
        entry,
        source.clone(),
        MirOperationKind::Host(MirHostOperation::StructuredShutdown {
            runtime: runtime_reference(RuntimeAbiRole::StructuredShutdown, runtime_abi),
        }),
        None,
    )?;

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
        ExecutableEntryResult, ExecutableHostEntryId, RootExecution, RuntimeAbiRole,
        RuntimeRoleImplementation,
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
            [root.clone()],
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
            Some(MirOperationKind::Host(
                MirHostOperation::ExecuteRoot {
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
                MirOperationKind::Host(MirHostOperation::BeginStaticCleanup),
                MirOperationKind::Host(MirHostOperation::ReportCleanupIncidents { .. }),
                MirOperationKind::Host(MirHostOperation::StructuredShutdown { .. }),
            ]
        ));
    }

    #[test]
    fn asynchronous_host_retains_the_distinguished_main_thread_lane_contract() {
        let host = test_async_executable_host_contract();

        assert!(matches!(
            host.entries()[0].root(),
            RootExecution::Asynchronous { .. }
        ));

        assert!(
            host.role_binding(RuntimeAbiRole::TestEntrySelection)
                .is_some()
        );

        assert!(
            !host
                .requirements()
                .requires_role(RuntimeAbiRole::TestEntrySelection)
        );

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
            [bray_testing::test_bound_unit(92).key().clone()],
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
    fn provider_host_admission_is_required_before_root_execution() {
        use bray_runtime_interface::{
            BinarySymbolName, ExecutableHostContractBuilder, RuntimeContract, RuntimeRequirements,
            RuntimeRoleBinding,
        };

        let base = test_async_executable_host_contract();
        let required = base.requirements();
        let runtime = base.runtime().unwrap();

        let control = RuntimeRoleBinding::new(
            RuntimeAbiRole::ProductHostControl,
            BinarySymbolName::try_new("test_product_control").unwrap(),
            RuntimeRoleImplementation::BrayRuntime,
        );

        let runtime = RuntimeContract::try_new(
            runtime.identity().clone(),
            runtime.artifact().clone(),
            runtime.abi_version(),
            runtime.frame_abi(),
            runtime.target().clone(),
            runtime.panic_abi().clone(),
            runtime.capabilities().iter().copied(),
            runtime.role_bindings().iter().cloned().chain([control]),
        )
        .unwrap();

        let requirements = RuntimeRequirements::new(
            required.runtime().cloned(),
            required.abi_version(),
            required.frame_abi(),
            required.target().clone(),
            required.panic_abi().clone(),
            required
                .roles()
                .iter()
                .copied()
                .chain([RuntimeAbiRole::ProductHostControl]),
            required.capabilities().iter().copied(),
            [],
        );

        let mut contract = ExecutableHostContractBuilder::new(
            base.product().clone(),
            base.native_entry().clone(),
            base.entries()[0].clone(),
            requirements,
        );

        contract.select_runtime(runtime);

        for role in RuntimeAbiRole::ALL {
            if let Some(binding) = base.role_binding(role) {
                if binding.implementation() != RuntimeRoleImplementation::BrayRuntime {
                    contract.push_role_binding(binding.clone());
                }
            }
        }

        let host = contract.finish().unwrap();

        let unit = super::lower_executable_host(ExecutableHostLoweringInput::new(
            MirUnitId::new(94),
            [bray_testing::test_bound_unit(94).key().clone()],
            host.clone(),
            test_mir_target(),
        ))
        .unwrap();

        let operations = unit
            .operations()
            .iter()
            .map(|operation| operation.kind().clone())
            .collect::<Vec<_>>();

        assert!(matches!(
            operations[0],
            MirOperationKind::Host(MirHostOperation::BeginExecution { .. })
        ));

        for reordered in [false, true] {
            let mut operations = operations.clone();

            if reordered {
                operations.swap(0, 1);
            } else {
                operations.remove(0);
            }

            let source = MirSourceAnchor::executable_host(host.product().clone());

            let mut builder = MirUnitBuilder::for_executable_host(
                MirUnitId::new(94),
                host.clone(),
                test_mir_target(),
            );

            let entry = builder
                .push_block(source.clone(), MirBlockKind::Ordinary)
                .unwrap();

            for operation in operations {
                builder
                    .push_operation(entry, source.clone(), operation, None)
                    .unwrap();
            }

            builder
                .set_terminator(entry, source, MirTerminatorKind::Return(None))
                .unwrap();

            assert_eq!(
                builder.finish(entry),
                Err(MirUnitBuildError::InvalidHostSequence)
            );
        }
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
            [bray_testing::test_bound_unit(93).key().clone()],
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
                entry: ExecutableHostEntryId::new(0),
                root: bray_testing::test_bound_unit(91).key().clone(),
                execution: RootExecution::Synchronous,
                runtime: super::runtime_reference(RuntimeAbiRole::RootExecution, runtime_abi),
            },
            MirHostOperation::ObserveRootTerminal {
                entry: ExecutableHostEntryId::new(0),
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
                entry: ExecutableHostEntryId::new(0),
                error: None,
                completion: super::runtime_reference(
                    RuntimeAbiRole::RootCompletionResolution,
                    runtime_abi,
                ),
                panic: super::runtime_reference(RuntimeAbiRole::PanicReporting, runtime_abi),
                entry_failure: super::runtime_reference(
                    RuntimeAbiRole::EntryFailureResolution,
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
