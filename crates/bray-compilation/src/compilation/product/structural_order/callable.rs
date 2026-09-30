use super::encoding::OrderKey;
use super::{
    encoding::{sequence, term},
    values::OrderEncoder,
};
use crate::compilation::CodegenPreparationError;
use bray_symbols::{
    CallableAbi, CallableConstness, CallableParameterMode, CallablePhaseBehavior, CallablePosition,
    CallableTrust, CallableTypeData, CurrentRunCancellation, ExecutionProperty,
    LifecycleObligationKind,
};

impl OrderEncoder<'_, '_> {
    pub(super) fn callable(
        &self,
        callable: &CallableTypeData,
    ) -> Result<OrderKey, CodegenPreparationError> {
        let parameters = callable
            .parameters()
            .iter()
            .map(|parameter| {
                Ok(term(
                    "parameter",
                    [
                        parameter.name().as_str().as_bytes().to_vec().into(),
                        match parameter.position() {
                            CallablePosition::NamedOnly => b"named_only".to_vec().into(),
                            CallablePosition::PositionalOrNamed => {
                                b"positional_or_named".to_vec().into()
                            }
                        },
                        match parameter.mode() {
                            CallableParameterMode::Immutable => b"immutable".to_vec().into(),
                            CallableParameterMode::Mutable => b"mutable".to_vec().into(),
                        },
                        self.ty(parameter.ty())?,
                    ],
                ))
            })
            .collect::<Result<Vec<_>, CodegenPreparationError>>()?;

        let invocation = self.phase(callable.phase_behaviors().invocation())?;

        let deferred = callable
            .phase_behaviors()
            .deferred_execution()
            .map(|phase| self.phase(phase))
            .transpose()?;

        Ok(term(
            "callable",
            [
                sequence(parameters),
                vec![u8::from(callable.is_variadic())].into(),
                self.ty(callable.result())?,
                match callable.constness() {
                    CallableConstness::Runtime => b"runtime".to_vec().into(),
                    CallableConstness::Constant => b"constant".to_vec().into(),
                },
                match callable.trust() {
                    CallableTrust::Safe => b"safe".to_vec().into(),
                    CallableTrust::Trusted => b"trusted".to_vec().into(),
                },
                match callable.abi() {
                    CallableAbi::Bray => b"bray".to_vec().into(),
                    CallableAbi::C => b"c".to_vec().into(),
                    CallableAbi::System => b"system".to_vec().into(),
                },
                invocation,
                sequence(deferred),
            ],
        ))
    }

    fn phase(&self, phase: &CallablePhaseBehavior) -> Result<OrderKey, CodegenPreparationError> {
        let symbols = |ids: Vec<bray_symbols::AnySymbolId>| {
            let mut keys = ids
                .into_iter()
                .map(|id| self.symbol(id))
                .collect::<Result<Vec<_>, _>>()?;

            keys.sort_unstable();

            Ok::<_, CodegenPreparationError>(sequence(keys))
        };

        let mut properties = phase
            .execution_properties()
            .iter()
            .map(|value| match value {
                ExecutionProperty::Pure => b"pure".to_vec().into(),
                ExecutionProperty::Total => b"total".to_vec().into(),
            })
            .collect::<Vec<_>>();

        properties.sort_unstable();

        let trusted = phase
            .trusted_capabilities()
            .iter()
            .map(|requirement| {
                Ok(sequence([
                    requirement.ordinal().raw().to_be_bytes().to_vec().into(),
                    self.symbol(requirement.capability().into())?,
                ]))
            })
            .collect::<Result<Vec<_>, CodegenPreparationError>>()?;

        let mut lifecycle = phase
            .lifecycle_obligations()
            .iter()
            .map(|value| lifecycle(*value))
            .collect::<Vec<_>>();

        lifecycle.sort_unstable();

        let values = self.compilation.semantic_value_store()?;
        let dependencies = values.dependency_contract_template_data(phase.dependency_contract());

        Ok(term(
            "phase",
            [
                sequence(properties),
                symbols(
                    phase
                        .effects()
                        .iter()
                        .map(|value| value.declaration())
                        .collect(),
                )?,
                symbols(
                    phase
                        .capabilities()
                        .iter()
                        .map(|value| value.declaration())
                        .collect(),
                )?,
                sequence(trusted),
                symbols(
                    phase
                        .execution_requirements()
                        .iter()
                        .map(|value| value.declaration())
                        .collect(),
                )?,
                sequence(lifecycle),
                self.dependencies(dependencies.requirements())?,
                vec![u8::from(
                    phase.current_run_cancellation() == CurrentRunCancellation::MayEnter,
                )]
                .into(),
            ],
        ))
    }
}

pub(super) fn lifecycle(value: LifecycleObligationKind) -> OrderKey {
    match value {
        LifecycleObligationKind::Destruction => b"destruction".to_vec().into(),
        LifecycleObligationKind::Finalization => b"finalization".to_vec().into(),
        LifecycleObligationKind::Cancellation => b"cancellation".to_vec().into(),
        LifecycleObligationKind::Joining => b"joining".to_vec().into(),
    }
}
