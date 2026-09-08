use bray_ir::{
    MirBlockKind, MirSourceAnchor, MirTargetContract, MirTerminatorKind, MirUnit, MirUnitBuilder,
    MirUnitKind,
};
use bray_runtime_interface::{
    BinarySymbolName, ExecutableEntryResult, ExecutableHostContract, ExecutableHostContractBuilder,
    ExecutableHostEntry, PanicAbiIdentity, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
    RootExecution, RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifactId, RuntimeCapability,
    RuntimeContract, RuntimeIdentity, RuntimeRequirements, RuntimeRoleBinding,
    RuntimeRoleImplementation,
};
use bray_symbols::{PackageIdentity, ProductIdentity, SemanticValueStore, TypeData, TypeId};
use bray_target::TargetIdentity;

use crate::test_bound_unit_with_declaration;

/// Builds one valid single-block MIR unit with a deterministic semantic identity.
pub fn test_mir_unit(unit: u32) -> MirUnit {
    test_mir_unit_with_declaration(unit, 0)
}

/// Builds one valid single-block MIR unit for a caller-selected target profile.
pub fn test_mir_unit_for_target(unit: u32, target: MirTargetContract) -> MirUnit {
    test_mir_unit_with_declaration_for_target(unit, 0, target)
}

/// Builds one valid single-block MIR unit with a caller-selected declaration identity.
pub fn test_mir_unit_with_declaration(unit: u32, declaration: u32) -> MirUnit {
    test_mir_unit_with_declaration_for_target(unit, declaration, test_mir_target())
}

/// Builds one valid single-block MIR unit with caller-selected declaration and target identities.
pub fn test_mir_unit_with_declaration_for_target(
    unit: u32,
    declaration: u32,
    target: MirTargetContract,
) -> MirUnit {
    let bound = test_bound_unit_with_declaration(unit, declaration);
    let source = MirSourceAnchor::from(bound.key().source());

    let mut builder = MirUnitBuilder::for_bound(bound.identity(), MirUnitKind::Synchronous, target);

    let Ok(entry) = builder.push_block(source.clone(), MirBlockKind::Ordinary) else {
        panic!("test MIR block must be valid");
    };

    let Ok(()) = builder.set_terminator(entry, source, MirTerminatorKind::Return(None)) else {
        panic!("test MIR terminator must be valid");
    };

    let Ok(unit) = builder.finish(entry) else {
        panic!("test MIR unit must be valid");
    };

    unit
}

/// Returns deterministic target properties for MIR tests.
pub fn test_mir_target() -> MirTargetContract {
    MirTargetContract::new(
        bray_target::test_support::test_target_profile(),
        RuntimeAbiVersion::CURRENT,
    )
}

/// Builds one valid deterministic compiler-generated executable-host contract.
pub fn test_executable_host_contract() -> ExecutableHostContract {
    test_executable_host_contract_for(test_product(), test_mir_target().identity().clone())
}

/// Builds one valid deterministic synchronous host contract for supplied identities.
pub fn test_executable_host_contract_for(
    product: ProductIdentity,
    target: TargetIdentity,
) -> ExecutableHostContract {
    test_host_contract(
        product,
        target,
        RootExecution::Synchronous,
        ExecutableEntryResult::Unit,
        None,
    )
}

/// Builds one valid deterministic synchronous host with a caller-selected result contract.
pub fn test_executable_host_contract_with_result(
    result: ExecutableEntryResult,
) -> ExecutableHostContract {
    test_host_contract(
        test_product(),
        test_mir_target().identity().clone(),
        RootExecution::Synchronous,
        result,
        None,
    )
}

/// Builds one valid deterministic asynchronous executable-host contract.
pub fn test_async_executable_host_contract() -> ExecutableHostContract {
    let Some(runtime) = RuntimeArtifactId::try_new("runtime.test") else {
        panic!("test runtime artifact identity must be valid");
    };

    test_async_executable_host_contract_for(
        test_product(),
        test_mir_target().identity().clone(),
        runtime,
    )
}

/// Builds one valid deterministic asynchronous host contract for supplied identities.
pub fn test_async_executable_host_contract_for(
    product: ProductIdentity,
    target: TargetIdentity,
    runtime: RuntimeArtifactId,
) -> ExecutableHostContract {
    test_async_executable_host_contract_for_frame(
        product,
        target,
        runtime,
        ProtectedAsyncFrameId::new([7; 32]),
    )
}

/// Builds one valid deterministic asynchronous host contract for a supplied frame.
pub fn test_async_executable_host_contract_for_frame(
    product: ProductIdentity,
    target: TargetIdentity,
    runtime: RuntimeArtifactId,
    frame: ProtectedAsyncFrameId,
) -> ExecutableHostContract {
    test_host_contract(
        product,
        target,
        RootExecution::Asynchronous { frame },
        ExecutableEntryResult::Unit,
        Some(runtime),
    )
}

fn test_host_contract(
    product: ProductIdentity,
    target: TargetIdentity,
    root: RootExecution,
    entry_result: ExecutableEntryResult,
    runtime: Option<RuntimeArtifactId>,
) -> ExecutableHostContract {
    let Some(entry) = BinarySymbolName::try_new("_bray_host_start") else {
        panic!("test host entry symbol name must be valid");
    };

    let roles = [
        RuntimeAbiRole::RootExecution,
        RuntimeAbiRole::RootCancellationRequest,
        RuntimeAbiRole::CleanupIncidentReporting,
        RuntimeAbiRole::RootTerminalObservation,
        RuntimeAbiRole::RootCompletionResolution,
        RuntimeAbiRole::PanicReporting,
        RuntimeAbiRole::EntryFailureResolution,
        RuntimeAbiRole::StructuredShutdown,
    ];

    let host_entry = match root {
        RootExecution::Synchronous => ExecutableHostEntry::synchronous(entry_result),
        RootExecution::Asynchronous { frame } => ExecutableHostEntry::asynchronous(
            frame,
            BinarySymbolName::try_new("__bray_test_root_frame_adapter")
                .unwrap_or_else(|| panic!("test frame adapter symbol must be valid")),
            entry_result,
        ),
    };

    let mut builder = ExecutableHostContractBuilder::new(
        product,
        entry,
        host_entry,
        test_runtime_requirements(target.clone(), runtime.is_some()),
    );

    for role in roles {
        builder.push_role_binding(test_role_binding(
            role,
            RuntimeRoleImplementation::CompilerLowering,
        ));
    }

    if let Some(runtime) = runtime {
        builder.select_runtime(test_runtime_contract(target, runtime));
    }

    match builder.finish() {
        Ok(contract) => contract,
        Err(error) => panic!("test executable-host contract must be valid: {error:?}"),
    }
}

fn test_product() -> ProductIdentity {
    let Some(package) = PackageIdentity::try_new("example.app") else {
        panic!("test package identity must be valid");
    };

    let Some(product) = ProductIdentity::try_new(package, "application") else {
        panic!("test product identity must be valid");
    };

    product
}

/// Returns a canonical error type for MIR tests.
pub fn test_mir_type() -> TypeId {
    let Ok(store) = SemanticValueStore::try_new() else {
        panic!("test semantic value store must be available");
    };

    match store.intern_type(TypeData::Error) {
        Ok(ty) => ty,
        Err(error) => panic!("test MIR type must be valid: {error:?}"),
    }
}

fn test_runtime_requirements(target: TargetIdentity, is_async: bool) -> RuntimeRequirements {
    let Some(panic_abi) = PanicAbiIdentity::try_new("bray.panic.test") else {
        panic!("test panic ABI identity must be valid");
    };

    let runtime = is_async.then(test_runtime_identity);

    let frame_abi =
        is_async.then(|| ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::CURRENT));

    let roles = is_async
        .then_some([
            RuntimeAbiRole::MainThreadLaneStartup,
            RuntimeAbiRole::MainThreadLaneDrive,
        ])
        .into_iter()
        .flatten();

    let capabilities = is_async
        .then_some([
            RuntimeCapability::CooperativeExecution,
            RuntimeCapability::MainThreadLane,
        ])
        .into_iter()
        .flatten();

    RuntimeRequirements::new(
        runtime,
        RuntimeAbiVersion::CURRENT,
        frame_abi,
        target,
        panic_abi,
        roles,
        capabilities,
        [],
    )
}

fn test_runtime_contract(target: TargetIdentity, artifact: RuntimeArtifactId) -> RuntimeContract {
    let Some(panic_abi) = PanicAbiIdentity::try_new("bray.panic.test") else {
        panic!("test panic ABI identity must be valid");
    };

    RuntimeContract::try_new(
        test_runtime_identity(),
        artifact,
        RuntimeAbiVersion::CURRENT,
        ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::CURRENT),
        target,
        panic_abi,
        [
            RuntimeCapability::CooperativeExecution,
            RuntimeCapability::MainThreadLane,
        ],
        [
            test_role_binding(
                RuntimeAbiRole::MainThreadLaneStartup,
                RuntimeRoleImplementation::BrayRuntime,
            ),
            test_role_binding(
                RuntimeAbiRole::MainThreadLaneDrive,
                RuntimeRoleImplementation::BrayRuntime,
            ),
            test_role_binding(
                RuntimeAbiRole::TestEntrySelection,
                RuntimeRoleImplementation::BrayRuntime,
            ),
        ],
    )
    .unwrap_or_else(|error| panic!("test runtime contract must be valid: {error:?}"))
}

fn test_runtime_identity() -> RuntimeIdentity {
    RuntimeIdentity::try_new("bray.runtime.test")
        .unwrap_or_else(|| panic!("test runtime identity must be valid"))
}

fn test_role_binding(
    role: RuntimeAbiRole,
    implementation: RuntimeRoleImplementation,
) -> RuntimeRoleBinding {
    let Some(symbol_name) = BinarySymbolName::try_new(format!("role_{role:?}")) else {
        panic!("test runtime role symbol name must be valid");
    };

    RuntimeRoleBinding::new(role, symbol_name, implementation)
}
