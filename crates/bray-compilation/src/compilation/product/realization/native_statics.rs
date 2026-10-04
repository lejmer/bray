use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

use bray_base::StableDigestHasher;
use bray_codegen::{CodegenInstance, CodegenNativeStaticMapping, CodegenUnit};
use bray_ir::MirStorageKind;
use bray_runtime_interface::BinarySymbolName;
use bray_symbols::{
    ForeignCallableDirection, NativeSymbolContract, StaticReferenceSelection, StaticStorageDuration,
};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::specialization::{ConcreteCodegenInstance, ConcreteCodegenReachability};
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::CancellationToken;

pub(in crate::compilation::product) struct NativeStaticContract {
    pub(in crate::compilation::product) symbol: NativeSymbolContract,
    pub(in crate::compilation::product) direction: ForeignCallableDirection,
    pub(in crate::compilation::product) duration: StaticStorageDuration,
}

impl Compilation {
    pub(super) fn codegen_native_publication_metadata(
        &self,
        instance: &CodegenInstance,
        realization: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<(bool, [u8; 32]), CodegenPreparationError> {
        // Owned static accessors and string constants use weak ODR helpers.
        // MachO cannot give those helpers exact COMDAT selection.
        let helpers_without_comdat = instance.key().target().machine().object_format()
            == bray_target::ObjectFormat::MachO;

        let mut independent = helpers_without_comdat
            && self.codegen_has_string_constants(instance, realization)?;

        let mut dependencies = BTreeSet::new();

        for storage in instance.mir().storages() {
            let owned_reference = match storage.kind() {
                MirStorageKind::NativeStatic(reference) => {
                    let contract = self.native_static_contract(reference, cancellation)?;
                    let mut hasher = StableDigestHasher::new();

                    hasher.write(b"bray.codegen-native-storage-reference");
                    contract.symbol.identity().hash(&mut hasher);
                    dependencies.insert(hasher.finalize());

                    independent |= contract.symbol.presence() == bray_symbols::NativeSymbolPresence::Optional
                        || contract.symbol.binding() == bray_symbols::NativeSymbolBinding::Weak;

                    (contract.direction == ForeignCallableDirection::Export).then_some(reference)
                }
                MirStorageKind::Static(reference) => Some(reference),
                _ => None,
            };

            if let Some(reference) = owned_reference {
                let (selected, _, _) = self.concrete_codegen_static_selection(realization, reference, cancellation)?;

                let mut hasher = StableDigestHasher::new();

                hasher.write(b"bray.codegen-owned-storage-reference");
                hasher.write(&self.static_cleanup_order_key(&selected, cancellation)?);
                dependencies.insert(hasher.finalize());
                independent |= helpers_without_comdat;
            }
        }

        let identity = if dependencies.is_empty() {
            [0; 32]
        } else {
            let mut hasher = StableDigestHasher::new();

            hasher.write(b"bray.codegen-storage-dependencies");
            dependencies.hash(&mut hasher);

            hasher.finalize()
        };

        Ok((independent, identity))
    }

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

    pub(in crate::compilation::product) fn native_static_contract(
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

    pub(in crate::compilation::product) fn optional_native_static_contract(
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
