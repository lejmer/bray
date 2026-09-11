use std::collections::BTreeSet;

use bray_runtime_interface::{ProtectedAsyncFrameId, ProtectedFrameOperation};

use crate::{
    CodegenOperationMapping, CodegenStaticStorageMapping, CodegenSymbolKey, CodegenSymbolMapping,
    CodegenUnit,
};

use super::core::CodegenMappingsBuildError;

/// Returns frame operations defined locally or referenced by helpers and static finalization.
pub fn demanded_frame_operations(
    unit: &CodegenUnit,
    operations: &[CodegenOperationMapping],
    static_storages: &[CodegenStaticStorageMapping],
) -> BTreeSet<(ProtectedAsyncFrameId, ProtectedFrameOperation)> {
    let local = unit
        .instances()
        .iter()
        .filter_map(crate::CodegenInstance::protected_frame_identity)
        .flat_map(|frame| {
            ProtectedFrameOperation::ALL
                .into_iter()
                .map(move |operation| (frame, operation))
        });

    let helpers = operations
        .iter()
        .flat_map(|mapping| mapping.helpers())
        .filter_map(|helper| match helper.symbol()? {
            CodegenSymbolKey::ProtectedFrame { frame, operation } => Some((*frame, *operation)),
            _ => None,
        });

    let finalizers = static_storages.iter().filter_map(|storage| {
        Some((
            storage.finalization()?.frame()?,
            ProtectedFrameOperation::MetadataDescription,
        ))
    });

    local.chain(helpers).chain(finalizers).collect()
}

pub(super) fn validate_frame_operations(
    unit: &CodegenUnit,
    operations: &[CodegenOperationMapping],
    static_storages: &[CodegenStaticStorageMapping],
    symbols: &[CodegenSymbolMapping],
) -> Result<(), CodegenMappingsBuildError> {
    let expected = demanded_frame_operations(unit, operations, static_storages);

    let actual: BTreeSet<_> = symbols
        .iter()
        .filter_map(|symbol| match symbol.key() {
            CodegenSymbolKey::ProtectedFrame { frame, operation } => Some((*frame, *operation)),
            _ => None,
        })
        .collect();

    if actual != expected {
        return Err(CodegenMappingsBuildError::FrameSymbolCoverageMismatch);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{demanded_frame_operations, validate_frame_operations};
    use crate::{
        CodegenLinkage, CodegenMappingsBuildError, CodegenSpecialization,
        CodegenStaticFinalization, CodegenStaticInstanceKey, CodegenStaticStorageMapping,
        CodegenSymbolKey, CodegenSymbolMapping,
    };
    use bray_runtime_interface::{
        BinarySymbolName, ProtectedAsyncFrameId, ProtectedFrameOperation,
    };
    use bray_symbols::{
        ConstantValueData, ConstantValueKind, ModulePathKey, PackageIdentity, SemanticValueStore,
        StaticStorageDuration, SymbolKey, SymbolRootKey, TypeData,
    };

    #[test]
    fn imported_static_metadata_requires_exact_frame_symbol_coverage() {
        let fixture = crate::test_support::codegen_request();
        let request = fixture.request();
        let instance = &request.unit().instances()[0];
        let frame = ProtectedAsyncFrameId::new([91; 32]);
        let store = SemanticValueStore::try_new().unwrap();
        let ty = store.intern_type(TypeData::Error).unwrap();

        let value = store
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Error))
            .unwrap();

        let declaration = SymbolKey::module(
            SymbolRootKey::Package(PackageIdentity::try_new("frame.demand.test").unwrap()),
            ModulePathKey::try_new(["statics"]).unwrap(),
        );

        let static_instance = CodegenStaticInstanceKey::new(
            declaration,
            CodegenSpecialization::NonGeneric,
            [],
            instance.key().target().clone(),
            StaticStorageDuration::Product,
        );

        let storage = CodegenStaticStorageMapping::new(
            instance.key().clone(),
            bray_ir::MirStorageId::from_slot(instance.mir().unit(), 0),
            ty,
            static_instance,
            BinarySymbolName::try_new("test_static").unwrap(),
            None,
            true,
            value,
            [],
            Some(CodegenStaticFinalization::new(
                instance.key().clone(),
                Some(frame),
            )),
            None,
        );

        let key = CodegenSymbolKey::ProtectedFrame {
            frame,
            operation: ProtectedFrameOperation::MetadataDescription,
        };

        let symbol = CodegenSymbolMapping::new(
            key,
            BinarySymbolName::try_new("imported_metadata").unwrap(),
            CodegenLinkage::Import,
            request.mappings().symbols()[0].signature().clone(),
        );

        let storages = [storage];
        let expected = BTreeSet::from([(frame, ProtectedFrameOperation::MetadataDescription)]);

        assert_eq!(
            demanded_frame_operations(request.unit(), &[], &storages),
            expected
        );

        assert_eq!(
            validate_frame_operations(request.unit(), &[], &storages, &[symbol.clone()]),
            Ok(())
        );

        assert_eq!(
            validate_frame_operations(request.unit(), &[], &storages, &[]),
            Err(CodegenMappingsBuildError::FrameSymbolCoverageMismatch)
        );

        assert_eq!(
            validate_frame_operations(request.unit(), &[], &[], &[symbol]),
            Err(CodegenMappingsBuildError::FrameSymbolCoverageMismatch)
        );
    }
}
