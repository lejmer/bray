use bray_ir::{MirTargetFacts, MirUnitBuilder, MirUnitId};
use bray_runtime_interface::ExecutableHostContract;

/// Complete synthetic input for lowering a compiler-generated executable host stub.
///
/// The host is not represented as a bound source unit. Its validated contract already names the
/// root mode, private ABI roles, distinguished main-thread lane, and structured shutdown policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutableHostLoweringInput {
    unit: MirUnitId,
    contract: ExecutableHostContract,
    target: MirTargetFacts,
}

impl ExecutableHostLoweringInput {
    /// Creates synthetic lowering input for one selected executable or test product.
    pub const fn new(
        unit: MirUnitId,
        contract: ExecutableHostContract,
        target: MirTargetFacts,
    ) -> Self {
        Self {
            unit,
            contract,
            target,
        }
    }

    /// Returns the compilation-local identity reserved for the generated host unit.
    pub const fn unit(&self) -> MirUnitId {
        self.unit
    }

    /// Returns the validated product-host execution contract.
    pub const fn contract(&self) -> &ExecutableHostContract {
        &self.contract
    }

    /// Returns target facts selected for lowering this host.
    pub const fn target(&self) -> &MirTargetFacts {
        &self.target
    }

    /// Transfers the synthetic input into a MIR builder with generated-product provenance.
    pub fn into_builder(self) -> MirUnitBuilder {
        MirUnitBuilder::for_executable_host(self.unit, self.contract, self.target)
    }
}

#[cfg(test)]
mod tests {
    use bray_ir::{
        MirBlockKind, MirSourceAnchor, MirTerminatorKind, MirUnitExecution, MirUnitId, MirUnitKey,
    };
    use bray_runtime_interface::{
        BinarySymbolName, ExecutableHostContract, ExecutableHostContractBuilder, RootExecution,
        RuntimeAbiRole, RuntimeAbiVersion, RuntimeRoleBinding, RuntimeRoleImplementation,
    };
    use bray_symbols::{PackageIdentity, ProductIdentity};
    use bray_testing::test_mir_target;

    use super::ExecutableHostLoweringInput;

    #[test]
    fn host_input_creates_generated_mir_without_a_bound_unit() {
        let host = host_contract();
        let product = host.product().clone();
        let input =
            ExecutableHostLoweringInput::new(MirUnitId::new(90), host.clone(), test_mir_target());
        let mut builder = input.into_builder();

        let source = MirSourceAnchor::executable_host(product.clone());

        let Ok(entry) = builder.push_block(source.clone(), MirBlockKind::Ordinary) else {
            panic!("generated host block must match its product origin");
        };

        let Ok(()) = builder.set_terminator(entry, source, MirTerminatorKind::Return(None)) else {
            panic!("generated host terminator must validate");
        };

        let Ok(unit) = builder.finish(entry) else {
            panic!("generated host MIR must validate");
        };

        assert_eq!(unit.key(), &MirUnitKey::ExecutableHost(product));
        assert_eq!(unit.execution(), &MirUnitExecution::ExecutableHost(host));
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
            RuntimeAbiVersion::new(1, 0),
        );

        for role in roles {
            builder.push_role_binding(role);
        }

        let Ok(host) = builder.finish() else {
            panic!("test host contract must be valid");
        };

        host
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
