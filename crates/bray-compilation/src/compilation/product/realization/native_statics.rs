use bray_codegen::{CodegenNativeStaticMapping, CodegenUnit};
use bray_ir::MirStorageKind;
use bray_runtime_interface::BinarySymbolName;
use bray_symbols::{
    ForeignCallableDirection, NativeSymbolContract, StaticReferenceSelection, StaticStorageDuration,
};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::specialization::ConcreteCodegenReachability;
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::CancellationToken;

pub(super) struct NativeStaticContract {
    pub(super) symbol: NativeSymbolContract,
    pub(super) direction: ForeignCallableDirection,
    pub(super) duration: StaticStorageDuration,
}

impl Compilation {
    pub(super) fn codegen_static_reference<'a>(
        &self,
        kind: &'a MirStorageKind,
        cancellation: &CancellationToken,
    ) -> Result<Option<&'a StaticReferenceSelection>, CodegenPreparationError> {
        match kind {
            MirStorageKind::Static(reference) => Ok(Some(reference)),
            MirStorageKind::NativeStatic(reference) => {
                let contract = self.optional_native_static_contract(reference, cancellation)?;

                Ok(contract
                    .is_some_and(|contract| contract.direction == ForeignCallableDirection::Export)
                    .then_some(reference))
            }
            _ => Ok(None),
        }
    }

    pub(super) fn codegen_native_static_storages(
        &self,
        unit: &CodegenUnit,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenNativeStaticMapping>, CodegenPreparationError> {
        let mut mappings = Vec::new();

        for instance in unit.instances() {
            let owner = reachability.instance(instance.key()).ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Instance(instance.key().clone()),
                    ProductDataKind::ConcreteInstance,
                )
            })?;

            for (storage, data) in instance.mir().storages_with_ids() {
                let MirStorageKind::NativeStatic(reference) = data.kind() else {
                    continue;
                };

                let contract = self.native_static_contract(reference, cancellation)?;

                let (selected, _, _) =
                    self.concrete_codegen_static_selection(owner, reference, cancellation)?;

                let template = self.static_instance_template(reference.template().declaration())?;

                let pointee_type = self.resolve_codegen_type(
                    template.value().declared_type(),
                    selected.substitution(),
                    cancellation,
                )?;

                let name = contract
                    .symbol
                    .identity()
                    .name()
                    .and_then(BinarySymbolName::try_new)
                    .ok_or(CodegenPreparationError::InvalidSymbolName)?;

                mappings.push(CodegenNativeStaticMapping::new(
                    instance.key().clone(),
                    storage,
                    data.ty(),
                    pointee_type,
                    name,
                    contract.direction,
                    contract.symbol.binding(),
                    contract.symbol.presence(),
                    contract.duration,
                ));
            }
        }

        Ok(mappings)
    }

    pub(super) fn native_static_contract(
        &self,
        reference: &StaticReferenceSelection,
        cancellation: &CancellationToken,
    ) -> Result<NativeStaticContract, CodegenPreparationError> {
        self.optional_native_static_contract(reference, cancellation)?
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::StaticReference(reference.clone()),
                    ProductDataKind::NativeStaticContract,
                )
                .into()
            })
    }

    pub(super) fn optional_native_static_contract(
        &self,
        reference: &StaticReferenceSelection,
        cancellation: &CancellationToken,
    ) -> Result<Option<NativeStaticContract>, CodegenPreparationError> {
        let declaration = reference.template().declaration();
        let source = self.foreign_static_contract_with_cancellation(declaration, cancellation)?;

        if let Some(contract) = source.value() {
            let template = self.static_instance_template(declaration)?;

            return Ok(Some(NativeStaticContract {
                symbol: contract.symbol().clone(),
                direction: contract.direction(),
                duration: template.value().duration(),
            }));
        }

        let Some(boundary) =
            self.imported_native_boundary_with_cancellation(declaration.into(), cancellation)?
        else {
            return Ok(None);
        };

        let bray_package_interface::InterfaceNativeBoundaryKind::Static { duration, .. } =
            boundary.kind()
        else {
            return Err(ProductQueryFailure::NativeBoundaryKindMismatch {
                reference: reference.clone(),
                actual: boundary.kind(),
            }
            .into());
        };

        Ok(Some(NativeStaticContract {
            symbol: boundary.symbol().clone(),
            direction: boundary.direction(),
            duration,
        }))
    }
}
