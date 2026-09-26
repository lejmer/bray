use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use bray_base::StableDigestHasher;
use bray_bound_tree::BoundUnitKey;
use bray_ir::{MirImportedExecutableKey, MirSourceAnchor, MirSourceOrigin, MirUnit, MirUnitKey, MirUnitKind};
use bray_package_interface::{
    ExecutableTemplateEncodeContext, ExecutableTemplateEncodeError, InterfaceConstantTermId,
    InterfaceConstantValueId, InterfaceDependencyContractId, InterfaceGenericSubstitutionId,
    InterfaceImplementationInstanceId, InterfaceSymbolReference, InterfaceTraitApplicationId,
    InterfaceTypeId, encode_codegen_mir,
};
use bray_symbols::{
    AnySymbolId, ConstantTermId, ConstantValueId, DependencyContractTemplateId,
    GenericSubstitutionId, ImplementationInstanceId, InterfaceSymbolId, SemanticValueStore,
    TraitApplicationId, TypeId,
};

use crate::compilation::Compilation;
use crate::compilation::binder::CompilationBindingContext;
use crate::compilation::product::specialization_identity::content::{
    StructuralSemanticValue, structural_semantic_identity,
};
use crate::fact::FactQueryError;

pub(in crate::compilation) fn mir_content_identity(
    compilation: &Compilation,
    mir: &MirUnit,
) -> Result<[u8; 32], FactQueryError> {
    let values = compilation.semantic_value_store()?;
    let binding_context = compilation.binding_context(&compilation.state.cancellation)?;
    let mut context = MirContentContext::new(values, &binding_context);

    let payload = match encode_codegen_mir(mir, &mut context) {
        Ok(payload) => payload,
        Err(ExecutableTemplateEncodeError::Semantic(error)) => return Err(error),
        Err(ExecutableTemplateEncodeError::InvalidUnitKind) => {
            panic!("validated codegen MIR must have an encodable unit kind: {:?}", mir.key())
        }
    };

    let mut digest = StableDigestHasher::new();

    digest.write(b"bray.codegen-mir-content.v1");
    digest.write_u64(u64::try_from(payload.len()).expect("encoded MIR length must fit u64"));
    digest.write(&payload);
    digest.write_u64(u64::try_from(context.references.len()).expect("MIR reference count must fit u64"));

    for identity in &context.references {
        digest.write(identity);
    }

    hash_source_origin(mir.source(), mir, &context, &mut digest)?;

    for block in mir.blocks() {
        hash_source_anchor(block.source(), mir, &context, &mut digest)?;
        hash_source_anchor(block.terminator().source(), mir, &context, &mut digest)?;
    }

    for operation in mir.operations() {
        hash_source_anchor(operation.source(), mir, &context, &mut digest)?;
    }

    for storage in mir.storages() {
        hash_source_anchor(storage.source(), mir, &context, &mut digest)?;
    }

    for value in mir.values() {
        hash_source_anchor(value.source(), mir, &context, &mut digest)?;
    }

    if let MirUnitKind::ExecutableHost(host) = mir.kind() {
        host.hash_with_structural_types(&mut digest, |ty| context.semantic_identity(StructuralSemanticValue::Type(ty)))?;
    }

    Ok(digest.finalize())
}

fn hash_source_origin(
    source: &MirSourceOrigin,
    mir: &MirUnit,
    context: &MirContentContext<'_, '_>,
    digest: &mut StableDigestHasher,
) -> Result<(), FactQueryError> {
    match source {
        MirSourceOrigin::CompilerProvidedCallable(id) => {
            digest.write_u8(0);
            context.semantic_identity(StructuralSemanticValue::Symbol(id.symbol()))?.hash(digest);
        }
        MirSourceOrigin::Source(anchor) => {
            digest.write_u8(1);
            anchor.hash(digest);
        }
        MirSourceOrigin::ExecutableHost(product) => {
            digest.write_u8(2);
            product.hash(digest);
        }
        MirSourceOrigin::GeneratedLifecycle(_) => {
            digest.write_u8(3);
            hash_lifecycle_key(mir, digest);
        }
        MirSourceOrigin::ImportedExecutable(key) => {
            digest.write_u8(4);
            hash_imported_key(key, context, digest)?;
        }
    }

    Ok(())
}

fn hash_source_anchor(
    source: &MirSourceAnchor,
    mir: &MirUnit,
    context: &MirContentContext<'_, '_>,
    digest: &mut StableDigestHasher,
) -> Result<(), FactQueryError> {
    match source {
        MirSourceAnchor::CompilerProvidedCallable(id) => {
            digest.write_u8(0);
            context.semantic_identity(StructuralSemanticValue::Symbol(id.symbol()))?.hash(digest);
        }
        MirSourceAnchor::Source(anchor) => {
            digest.write_u8(1);
            anchor.hash(digest);
        }
        MirSourceAnchor::ExecutableHost(product) => {
            digest.write_u8(2);
            product.hash(digest);
        }
        MirSourceAnchor::GeneratedLifecycle(_) => {
            digest.write_u8(3);
            hash_lifecycle_key(mir, digest);
        }
        MirSourceAnchor::ImportedExecutable(key) => {
            digest.write_u8(4);
            hash_imported_key(key, context, digest)?;
        }
        MirSourceAnchor::ImportedSource { owner, span, version } => {
            digest.write_u8(5);
            hash_imported_key(owner, context, digest)?;
            span.hash(digest);
            version.hash(digest);
        }
    }

    Ok(())
}

fn hash_lifecycle_key(mir: &MirUnit, digest: &mut StableDigestHasher) {
    let MirUnitKey::GeneratedLifecycle(key) = mir.key() else {
        panic!("generated lifecycle source requires a lifecycle MIR key: {:?}", mir.key());
    };

    key.hash(digest);
}

fn hash_imported_key(
    key: &MirImportedExecutableKey,
    context: &MirContentContext<'_, '_>,
    digest: &mut StableDigestHasher,
) -> Result<(), FactQueryError> {
    context.semantic_identity(StructuralSemanticValue::Symbol(key.owner()))?.hash(digest);
    key.template().hash(digest);
    key.platform_service().hash(digest);

    Ok(())
}

struct MirContentContext<'values, 'compilation> {
    values: &'values SemanticValueStore,
    binding_context: &'values CompilationBindingContext<'compilation>,
    references: Vec<[u8; 32]>,
    semantic: BTreeMap<StructuralSemanticValue, u32>,
    nested: BTreeMap<BoundUnitKey, u32>,
}

impl<'values, 'compilation> MirContentContext<'values, 'compilation> {
    fn new(
        values: &'values SemanticValueStore,
        binding_context: &'values CompilationBindingContext<'compilation>,
    ) -> Self {
        Self {
            values,
            binding_context,
            references: Vec::new(),
            semantic: BTreeMap::new(),
            nested: BTreeMap::new(),
        }
    }

    fn semantic_identity(&self, value: StructuralSemanticValue) -> Result<[u8; 32], FactQueryError> {
        structural_semantic_identity(self.values, self.binding_context, value)
    }

    fn intern_semantic(&mut self, value: StructuralSemanticValue) -> Result<u32, FactQueryError> {
        if let Some(index) = self.semantic.get(&value) {
            return Ok(*index);
        }

        let identity = self.semantic_identity(value)?;
        let index = u32::try_from(self.references.len()).expect("MIR reference table must fit u32");

        self.semantic.insert(value, index);
        self.references.push(identity);

        Ok(index)
    }
}

impl ExecutableTemplateEncodeContext for MirContentContext<'_, '_> {
    type Error = FactQueryError;

    fn type_id(&mut self, id: TypeId) -> Result<InterfaceTypeId, Self::Error> {
        Ok(InterfaceTypeId::new(self.intern_semantic(StructuralSemanticValue::Type(id))?))
    }

    fn constant_value_id(&mut self, id: ConstantValueId) -> Result<InterfaceConstantValueId, Self::Error> {
        Ok(InterfaceConstantValueId::new(self.intern_semantic(StructuralSemanticValue::ConstantValue(id))?))
    }

    fn constant_term_id(&mut self, id: ConstantTermId) -> Result<InterfaceConstantTermId, Self::Error> {
        Ok(InterfaceConstantTermId::new(self.intern_semantic(StructuralSemanticValue::ConstantTerm(id))?))
    }

    fn substitution_id(&mut self, id: GenericSubstitutionId) -> Result<InterfaceGenericSubstitutionId, Self::Error> {
        Ok(InterfaceGenericSubstitutionId::new(self.intern_semantic(StructuralSemanticValue::Substitution(id))?))
    }

    fn trait_application_id(&mut self, id: TraitApplicationId) -> Result<InterfaceTraitApplicationId, Self::Error> {
        Ok(InterfaceTraitApplicationId::new(self.intern_semantic(StructuralSemanticValue::TraitApplication(id))?))
    }

    fn implementation_instance_id(&mut self, id: ImplementationInstanceId) -> Result<InterfaceImplementationInstanceId, Self::Error> {
        Ok(InterfaceImplementationInstanceId::new(self.intern_semantic(StructuralSemanticValue::Implementation(id))?))
    }

    fn dependency_contract_id(&mut self, id: DependencyContractTemplateId) -> Result<InterfaceDependencyContractId, Self::Error> {
        Ok(InterfaceDependencyContractId::new(self.intern_semantic(StructuralSemanticValue::DependencyContract(id))?))
    }

    fn symbol_reference(&mut self, id: AnySymbolId) -> Result<InterfaceSymbolReference, Self::Error> {
        let index = self.intern_semantic(StructuralSemanticValue::Symbol(id))?;
        let symbol = InterfaceSymbolId::try_from_index(index as usize).expect("MIR symbol table index must fit u32");

        Ok(InterfaceSymbolReference::Local(symbol))
    }

    fn nested_executable_id(&mut self, key: &BoundUnitKey) -> Result<bray_ir::MirExecutableTemplateId, Self::Error> {
        if let Some(index) = self.nested.get(key) {
            return Ok(bray_ir::MirExecutableTemplateId::new(*index));
        }

        let mut identity = StableDigestHasher::new();

        key.hash(&mut identity);

        let index = u32::try_from(self.references.len()).expect("MIR reference table must fit u32");

        self.nested.insert(key.clone(), index);
        self.references.push(identity.finalize());

        Ok(bray_ir::MirExecutableTemplateId::new(index))
    }
}

#[cfg(test)]
mod tests {
    use bray_codegen::{CodegenPartitionPolicy, CodegenUnit};
    use bray_ir::{MirBlockKind, MirSourceAnchor, MirStorageKind, MirTerminatorKind, MirUnit, MirUnitBuilder, MirUnitKind};
    use bray_symbols::{TypeData, TypeId};
    use bray_testing::{test_bound_unit_with_declaration, test_mir_target};

    use super::mir_content_identity;
    use crate::test_support::compilation;

    #[test]
    fn equal_mir_ignores_semantic_store_and_interning_order() {
        let first = compilation("module app; func main() {}");
        let second = compilation("module app; func main() {}");
        let first_values = first.semantic_value_store().expect("first semantic store must exist");
        let second_values = second.semantic_value_store().expect("second semantic store must exist");

        let first_type = first_values.intern_type(TypeData::tuple([])).expect("first type must intern");
        second_values.intern_type(TypeData::Error).expect("unrelated type must intern");
        let second_type = second_values.intern_type(TypeData::tuple([])).expect("second type must intern");

        assert_ne!(first_type.store_id(), second_type.store_id());
        assert_ne!(first_type.slot(), second_type.slot());

        let first_mir = mir_with_storages(first_type, 1);
        let second_mir = mir_with_storages(second_type, 1);
        let changed_mir = mir_with_storages(second_type, 2);

        let first_identity = mir_content_identity(&first, &first_mir).expect("first MIR identity must resolve");
        let second_identity = mir_content_identity(&second, &second_mir).expect("second MIR identity must resolve");
        let changed_identity = mir_content_identity(&second, &changed_mir).expect("changed MIR identity must resolve");

        assert_eq!(first_identity, second_identity);
        assert_ne!(second_identity, changed_identity);

        let first_unit = CodegenUnit::try_new(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            bray_codegen::test_support::codegen_partition_compatibility(),
            [first_mir],
            |mir| mir_content_identity(&first, mir).expect("first unit identity must resolve"),
        ).expect("first unit must validate");

        let second_unit = CodegenUnit::try_new(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            bray_codegen::test_support::codegen_partition_compatibility(),
            [second_mir],
            |mir| mir_content_identity(&second, mir).expect("second unit identity must resolve"),
        ).expect("second unit must validate");

        let changed_unit = CodegenUnit::try_new(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            bray_codegen::test_support::codegen_partition_compatibility(),
            [changed_mir],
            |mir| mir_content_identity(&second, mir).expect("changed unit identity must resolve"),
        ).expect("changed unit must validate");

        assert_eq!(first_unit.key().content_identity(), second_unit.key().content_identity());
        assert_ne!(second_unit.key().content_identity(), changed_unit.key().content_identity());
    }

    fn mir_with_storages(ty: TypeId, count: usize) -> MirUnit {
        let bound = test_bound_unit_with_declaration(4, 0);
        let source = MirSourceAnchor::from(bound.key().source());
        let mut builder = MirUnitBuilder::for_bound(bound.identity(), MirUnitKind::Synchronous, test_mir_target());
        let entry = builder.push_block(source.clone(), MirBlockKind::Ordinary).expect("test block must validate");

        for _ in 0..count {
            builder.push_storage(source.clone(), MirStorageKind::Local, ty).expect("test storage must validate");
        }

        builder.set_terminator(entry, source, MirTerminatorKind::Return(None));

        builder.finish(entry)
    }
}
