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
    let mut builder = MirUnitBuilder::for_executable_host(unit, contract, target);

    let entry = builder.push_block(source.clone(), MirBlockKind::Ordinary)?;

    let operations = [
        MirHostOperation::ExecuteRoot {
            root,
            execution,
            runtime: runtime_reference(RuntimeAbiRole::RootExecution, runtime_abi),
        },
        MirHostOperation::RequestRootCancellation {
            runtime: runtime_reference(RuntimeAbiRole::RootCancellationRequest, runtime_abi),
        },
        MirHostOperation::ObserveRootTerminal {
            runtime: runtime_reference(RuntimeAbiRole::RootTerminalObservation, runtime_abi),
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
        BinarySymbolName, ExecutableHostContract, ExecutableHostContractBuilder, PanicAbiIdentity,
        RootExecution, RuntimeAbiRole, RuntimeAbiVersion, RuntimeRequirements, RuntimeRoleBinding,
        RuntimeRoleImplementation,
    };
    use bray_symbols::{PackageIdentity, ProductIdentity};
    use bray_testing::test_mir_target;

    use super::ExecutableHostLoweringInput;

    #[test]
    fn host_input_creates_generated_mir_without_a_bound_unit() {
        let host = host_contract();
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
                MirOperationKind::Host(MirHostOperation::RequestRootCancellation { .. }),
                MirOperationKind::Host(MirHostOperation::ObserveRootTerminal { .. }),
                MirOperationKind::Host(MirHostOperation::ReportCleanupIncidents { .. }),
                MirOperationKind::Host(MirHostOperation::StructuredShutdown { .. }),
            ]
        ));
    }

    #[test]
    fn executable_host_validation_rejects_reordered_shutdown_operations() {
        let host = host_contract();
        let source = MirSourceAnchor::executable_host(host.product().clone());
        let target = test_mir_target();
        let runtime_abi = target.runtime_abi();

        let mut builder =
            MirUnitBuilder::for_executable_host(MirUnitId::new(91), host, target);

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
            MirHostOperation::RequestRootCancellation {
                runtime: super::runtime_reference(
                    RuntimeAbiRole::RootCancellationRequest,
                    runtime_abi,
                ),
            },
            MirHostOperation::ReportCleanupIncidents {
                runtime: super::runtime_reference(
                    RuntimeAbiRole::CleanupIncidentReporting,
                    runtime_abi,
                ),
            },
            MirHostOperation::StructuredShutdown {
                runtime: super::runtime_reference(
                    RuntimeAbiRole::StructuredShutdown,
                    runtime_abi,
                ),
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

    fn host_contract() -> ExecutableHostContract {
        let Some(package) = PackageIdentity::try_new("example.app") else {
            panic!("test package identity must be valid");
        };

        let Some(product) = ProductIdentity::try_new(package, "application") else {
            panic!("test product identity must be valid");
        };

        let Some(entry) = BinarySymbolName::try_new("_bray_host_start") else {
            panic!("test host entry symbol name must be valid");
        };

        let roles = [
            RuntimeAbiRole::RootExecution,
            RuntimeAbiRole::RootCancellationRequest,
            RuntimeAbiRole::CleanupIncidentReporting,
            RuntimeAbiRole::RootTerminalObservation,
            RuntimeAbiRole::StructuredShutdown,
        ]
        .into_iter()
        .map(role_binding);

        let mut builder = ExecutableHostContractBuilder::new(
            product,
            entry,
            RootExecution::Synchronous,
            runtime_requirements(),
        );

        for role in roles {
            builder.push_role_binding(role);
        }

        let Ok(host) = builder.finish() else {
            panic!("test host contract must be valid");
        };

        host
    }

    fn runtime_requirements() -> RuntimeRequirements {
        let target = test_mir_target();

        let Some(panic_abi) = PanicAbiIdentity::try_new("bray.panic.test") else {
            panic!("test panic ABI identity must be valid");
        };

        RuntimeRequirements::new(
            None,
            RuntimeAbiVersion::new(1, 0),
            None,
            target.identity().clone(),
            panic_abi,
            [],
            [],
            [],
        )
    }

    fn role_binding(role: RuntimeAbiRole) -> RuntimeRoleBinding {
        let Some(symbol_name) = BinarySymbolName::try_new(format!("role_{role:?}")) else {
            panic!("test role symbol name must be valid");
        };

        RuntimeRoleBinding::new(
            role,
            symbol_name,
            RuntimeRoleImplementation::CompilerLowering,
        )
    }
}
