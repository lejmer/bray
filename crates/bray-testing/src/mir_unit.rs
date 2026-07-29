use bray_ir::{
    MirBlockKind, MirSourceAnchor, MirTargetFacts, MirTerminatorKind, MirUnit, MirUnitBuilder,
    MirUnitKind,
};
use bray_runtime_interface::{
    BinarySymbolName, ExecutableHostContract, ExecutableHostContractBuilder, PanicAbiIdentity,
    RootExecution, RuntimeAbiRole, RuntimeAbiVersion, RuntimeRequirements, RuntimeRoleBinding,
    RuntimeRoleImplementation,
};
use bray_symbols::{PackageIdentity, ProductIdentity, SemanticValueStore, TypeData, TypeId};

use crate::test_bound_unit_with_declaration;

/// Builds one valid single-block MIR unit with a deterministic semantic identity.
pub fn test_mir_unit(unit: u32) -> MirUnit {
    test_mir_unit_with_declaration(unit, 0)
}

/// Builds one valid single-block MIR unit with a caller-selected declaration identity.
pub fn test_mir_unit_with_declaration(unit: u32, declaration: u32) -> MirUnit {
    let bound = test_bound_unit_with_declaration(unit, declaration);
    let source = MirSourceAnchor::from(bound.key().source());

    let mut builder = MirUnitBuilder::for_bound(
        bound.identity(),
        MirUnitKind::Synchronous,
        test_mir_target(),
    );

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

/// Returns deterministic target facts for MIR tests.
pub fn test_mir_target() -> MirTargetFacts {
    MirTargetFacts::new(
        bray_target::test_support::test_target_profile(),
        bray_runtime_interface::RuntimeAbiVersion::new(1, 0),
    )
}

/// Builds one valid deterministic compiler-generated executable-host contract.
pub fn test_executable_host_contract() -> ExecutableHostContract {
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
    ];

    let mut builder = ExecutableHostContractBuilder::new(
        product,
        entry,
        RootExecution::Synchronous,
        test_runtime_requirements(),
    );

    for role in roles {
        let Some(symbol_name) = BinarySymbolName::try_new(format!("role_{role:?}")) else {
            panic!("test role symbol name must be valid");
        };

        builder.push_role_binding(RuntimeRoleBinding::new(
            role,
            symbol_name,
            RuntimeRoleImplementation::CompilerLowering,
        ));
    }

    match builder.finish() {
        Ok(contract) => contract,
        Err(error) => panic!("test executable-host contract must be valid: {error:?}"),
    }
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

fn test_runtime_requirements() -> RuntimeRequirements {
    let target = test_mir_target();

    let Some(panic_abi) = PanicAbiIdentity::try_new("bray.panic.test") else {
        panic!("test panic ABI identity must be valid");
    };

    RuntimeRequirements::new(
        None,
        RuntimeAbiVersion::new(1, 0),
        None,
        // The test contract owns its immutable target identity independently of target facts.
        target.identity().clone(),
        panic_abi,
        [],
        [],
        [],
    )
}
