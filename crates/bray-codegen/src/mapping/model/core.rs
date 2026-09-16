use std::sync::Arc;

use bray_ir::MirSourceAnchor;
use bray_symbols::{ConstantTermId, ConstantValueData, ConstantValueId, TypeId};

use crate::{
    CodegenCallableMapping, CodegenConstantMapping, CodegenConstantTermMapping,
    CodegenDebugLocation, CodegenInstanceKey, CodegenInstanceTypeMapping,
    CodegenNativeStaticMapping, CodegenOperationMapping, CodegenProductHostMapping,
    CodegenStaticStorageMapping, CodegenSymbolKey, CodegenSymbolMapping, CodegenTarget,
    CodegenTerminatorMapping, CodegenTypeMapping, CodegenUnit, CodegenUnitKey,
};

/// Canonical code generation mappings demanded by one concrete code generation unit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenMappings {
    unit: CodegenUnitKey,
    target: CodegenTarget,
    types: Arc<[CodegenTypeMapping]>,
    instance_types: Arc<[CodegenInstanceTypeMapping]>,
    symbols: Arc<[CodegenSymbolMapping]>,
    constants: Arc<[CodegenConstantMapping]>,
    constant_terms: Arc<[CodegenConstantTermMapping]>,
    callables: Arc<[CodegenCallableMapping]>,
    operations: Arc<[CodegenOperationMapping]>,
    static_storages: Arc<[CodegenStaticStorageMapping]>,
    native_storages: Arc<[CodegenNativeStaticMapping]>,
    product_host: Option<CodegenProductHostMapping>,
    terminators: Arc<[CodegenTerminatorMapping]>,
    debug_locations: Arc<[CodegenDebugLocation]>,
}

impl CodegenMappings {
    /// Owns and orders every realization mapping for one code generation unit.
    ///
    /// # Panics
    ///
    /// Panics when `target` does not match the target selected by `unit`.
    #[expect(
        clippy::too_many_arguments,
        reason = "the constructor owns each independent mapping table explicitly"
    )]
    pub fn new(
        unit: &CodegenUnit,
        target: &CodegenTarget,
        types: impl IntoIterator<Item = CodegenTypeMapping>,
        instance_types: impl IntoIterator<Item = CodegenInstanceTypeMapping>,
        symbols: impl IntoIterator<Item = CodegenSymbolMapping>,
        constants: impl IntoIterator<Item = CodegenConstantMapping>,
        constant_terms: impl IntoIterator<Item = CodegenConstantTermMapping>,
        callables: impl IntoIterator<Item = CodegenCallableMapping>,
        operations: impl IntoIterator<Item = CodegenOperationMapping>,
        static_storages: impl IntoIterator<Item = CodegenStaticStorageMapping>,
        native_storages: impl IntoIterator<Item = CodegenNativeStaticMapping>,
        terminators: impl IntoIterator<Item = CodegenTerminatorMapping>,
        debug_locations: impl IntoIterator<Item = CodegenDebugLocation>,
    ) -> Self {
        assert!(
            target.matches_mir_target(unit.target()),
            "codegen mapping target must match unit {:?}: mapping target {:?}, MIR target {:?}",
            unit.key(),
            target,
            unit.target()
        );

        let mut types: Vec<_> = types.into_iter().collect();
        let mut instance_types: Vec<_> = instance_types.into_iter().collect();
        let mut symbols: Vec<_> = symbols.into_iter().collect();
        let mut constants: Vec<_> = constants.into_iter().collect();
        let mut constant_terms: Vec<_> = constant_terms.into_iter().collect();
        let mut callables: Vec<_> = callables.into_iter().collect();
        let mut operations: Vec<_> = operations.into_iter().collect();
        let mut static_storages: Vec<_> = static_storages.into_iter().collect();
        let mut native_storages: Vec<_> = native_storages.into_iter().collect();
        let mut terminators: Vec<_> = terminators.into_iter().collect();
        let mut debug_locations: Vec<_> = debug_locations.into_iter().collect();

        types.sort_unstable_by_key(CodegenTypeMapping::ty);
        instance_types.sort_unstable();
        symbols.sort_unstable_by(|left, right| left.key().cmp(right.key()));
        constants.sort_unstable_by_key(|mapping| (mapping.value(), mapping.representation()));
        constant_terms.sort_unstable();
        callables.sort_unstable();
        operations.sort_unstable();
        static_storages.sort_unstable();
        native_storages.sort_unstable();
        terminators.sort_unstable();
        debug_locations.sort_unstable_by(|left, right| left.anchor().cmp(right.anchor()));

        Self {
            // The mappings retain immutable structural request identities independently.
            unit: unit.key().clone(),
            target: target.clone(),
            types: types.into(),
            instance_types: instance_types.into(),
            symbols: symbols.into(),
            constants: constants.into(),
            constant_terms: constant_terms.into(),
            callables: callables.into(),
            operations: operations.into(),
            static_storages: static_storages.into(),
            native_storages: native_storages.into(),
            product_host: None,
            terminators: terminators.into(),
            debug_locations: debug_locations.into(),
        }
    }

    /// Returns the exact code generation unit covered by these mappings.
    pub const fn unit(&self) -> &CodegenUnitKey {
        &self.unit
    }

    /// Returns the exact code generation target covered by these mappings.
    pub const fn target(&self) -> &CodegenTarget {
        &self.target
    }

    /// Returns type mappings in canonical semantic-type order.
    pub fn types(&self) -> &[CodegenTypeMapping] {
        &self.types
    }

    /// Returns instance-local substitutions in canonical instance and template order.
    pub fn instance_types(&self) -> &[CodegenInstanceTypeMapping] {
        &self.instance_types
    }

    /// Returns symbol mappings in canonical semantic-key order.
    pub fn symbols(&self) -> &[CodegenSymbolMapping] {
        &self.symbols
    }

    /// Returns materialized constants in canonical semantic-value order.
    pub fn constants(&self) -> &[CodegenConstantMapping] {
        &self.constants
    }

    /// Returns closed constant-term mappings in canonical term order.
    pub fn constant_terms(&self) -> &[CodegenConstantTermMapping] {
        &self.constant_terms
    }

    /// Returns callable-reference mappings in canonical reference order.
    pub fn callables(&self) -> &[CodegenCallableMapping] {
        &self.callables
    }

    /// Returns operation realization mappings in canonical operation order.
    pub fn operations(&self) -> &[CodegenOperationMapping] {
        &self.operations
    }

    /// Returns static-storage uses in canonical instance and storage order.
    pub fn static_storages(&self) -> &[CodegenStaticStorageMapping] {
        &self.static_storages
    }

    /// Returns native data-symbol uses in canonical instance and storage order.
    pub fn native_storages(&self) -> &[CodegenNativeStaticMapping] {
        &self.native_storages
    }

    /// Returns the loaded-product host mapping shared by every unit in this product.
    pub const fn product_host(&self) -> Option<&CodegenProductHostMapping> {
        self.product_host.as_ref()
    }

    /// Attaches the loaded-product host mapping.
    pub fn with_product_host(mut self, product_host: CodegenProductHostMapping) -> Self {
        self.product_host = Some(product_host);

        self
    }

    /// Returns terminator realization mappings in canonical block order.
    pub fn terminators(&self) -> &[CodegenTerminatorMapping] {
        &self.terminators
    }

    /// Returns source mappings in canonical MIR-anchor order.
    pub fn debug_locations(&self) -> &[CodegenDebugLocation] {
        &self.debug_locations
    }

    /// Returns the demanded mapping for one semantic type.
    pub fn ty(&self, ty: TypeId) -> Option<&CodegenTypeMapping> {
        self.types
            .binary_search_by_key(&ty, CodegenTypeMapping::ty)
            .ok()
            .map(|index| &self.types[index])
    }

    /// Returns the demanded mapping for a type as interpreted by one concrete MIR instance.
    pub fn instance_ty(
        &self,
        instance: &CodegenInstanceKey,
        ty: TypeId,
    ) -> Option<&CodegenTypeMapping> {
        let key = (instance, ty);

        let concrete = self
            .instance_types
            .binary_search_by(|mapping| (mapping.instance(), mapping.template()).cmp(&key))
            .ok()
            .and_then(|index| self.instance_types.get(index))
            .map(CodegenInstanceTypeMapping::concrete)
            .unwrap_or(ty);

        self.ty(concrete)
    }

    /// Returns the demanded mapping for one semantic symbol.
    pub fn symbol(&self, key: &CodegenSymbolKey) -> Option<&CodegenSymbolMapping> {
        self.symbols
            .binary_search_by(|mapping| mapping.key().cmp(key))
            .ok()
            .map(|index| &self.symbols[index])
    }

    /// Returns the binary symbol for one concrete generated definition.
    pub fn instance_symbol(&self, instance: &CodegenInstanceKey) -> Option<&CodegenSymbolMapping> {
        self.symbols
            .binary_search_by(|mapping| match mapping.key() {
                CodegenSymbolKey::Instance(candidate) => candidate.cmp(instance),
                CodegenSymbolKey::Runtime(_) | CodegenSymbolKey::ProtectedFrame { .. } => {
                    std::cmp::Ordering::Greater
                }
            })
            .ok()
            .map(|index| &self.symbols[index])
    }

    /// Returns the semantic representation for one constant value.
    pub fn constant(&self, value: ConstantValueId) -> Option<&CodegenConstantMapping> {
        let start = self
            .constants
            .partition_point(|mapping| mapping.value() < value);

        self.constants[start..]
            .iter()
            .take_while(|mapping| mapping.value() == value)
            .find(|mapping| mapping.semantic_type() == mapping.representation())
    }

    /// Returns immutable semantic data for a demanded constant in any valid use representation.
    pub fn constant_data(&self, value: ConstantValueId) -> Option<&ConstantValueData> {
        let start = self
            .constants
            .partition_point(|mapping| mapping.value() < value);

        self.constants
            .get(start)
            .filter(|mapping| mapping.value() == value)
            .map(CodegenConstantMapping::data)
    }

    /// Returns the materialized mapping for one exact constant use representation.
    pub fn constant_with_representation(
        &self,
        value: ConstantValueId,
        representation: TypeId,
    ) -> Option<&CodegenConstantMapping> {
        self.constants
            .binary_search_by_key(&(value, representation), |mapping| {
                (mapping.value(), mapping.representation())
            })
            .ok()
            .map(|index| &self.constants[index])
    }

    /// Returns the materialized value selected for one closed constant term.
    pub fn constant_term(
        &self,
        owner: &CodegenInstanceKey,
        term: ConstantTermId,
    ) -> Option<ConstantValueId> {
        self.constant_terms
            .binary_search_by(|mapping| {
                mapping
                    .owner()
                    .cmp(owner)
                    .then_with(|| mapping.term().cmp(&term))
            })
            .ok()
            .map(|index| self.constant_terms[index].value())
    }

    /// Returns the concrete instance selected for one semantic callable reference.
    pub fn callable(
        &self,
        owner: &CodegenInstanceKey,
        site: crate::CodegenCallSite,
    ) -> Option<&CodegenCallableMapping> {
        self.callables
            .binary_search_by(|mapping| {
                mapping
                    .owner()
                    .cmp(owner)
                    .then_with(|| mapping.site().cmp(&site))
            })
            .ok()
            .map(|index| &self.callables[index])
    }

    /// Returns ordered helper symbols for one MIR operation.
    pub fn operation(
        &self,
        owner: &CodegenInstanceKey,
        operation: bray_ir::MirOperationId,
    ) -> Option<&CodegenOperationMapping> {
        self.operations
            .binary_search_by(|mapping| {
                mapping
                    .owner()
                    .cmp(owner)
                    .then_with(|| mapping.operation().cmp(&operation))
            })
            .ok()
            .map(|index| &self.operations[index])
    }

    /// Returns the native realization selected for one static storage root.
    pub fn static_storage(
        &self,
        owner: &CodegenInstanceKey,
        storage: bray_ir::MirStorageId,
    ) -> Option<&CodegenStaticStorageMapping> {
        self.static_storages
            .binary_search_by(|mapping| {
                mapping
                    .owner()
                    .cmp(owner)
                    .then_with(|| mapping.storage().cmp(&storage))
            })
            .ok()
            .map(|index| &self.static_storages[index])
    }

    /// Returns the native data-symbol mapping for one exact MIR storage use.
    pub fn native_static_storage(
        &self,
        owner: &CodegenInstanceKey,
        storage: bray_ir::MirStorageId,
    ) -> Option<&CodegenNativeStaticMapping> {
        self.native_storages
            .binary_search_by(|mapping| (mapping.owner(), mapping.storage()).cmp(&(owner, storage)))
            .ok()
            .and_then(|index| self.native_storages.get(index))
    }

    /// Returns extra realization inputs for one block terminator.
    pub fn terminator(
        &self,
        owner: &CodegenInstanceKey,
        block: bray_ir::MirBlockId,
    ) -> Option<&CodegenTerminatorMapping> {
        self.terminators
            .binary_search_by(|mapping| {
                mapping
                    .owner()
                    .cmp(owner)
                    .then_with(|| mapping.block().cmp(&block))
            })
            .ok()
            .map(|index| &self.terminators[index])
    }

    /// Returns the source location selected for one MIR anchor.
    pub fn debug_location(&self, anchor: &MirSourceAnchor) -> Option<&CodegenDebugLocation> {
        self.debug_locations
            .binary_search_by(|location| location.anchor().cmp(anchor))
            .ok()
            .map(|index| &self.debug_locations[index])
    }
}

#[cfg(test)]
mod tests {
    use std::num::{NonZeroU16, NonZeroU64};

    use bray_ir::{
        MirBlockKind, MirSourceAnchor, MirStorageKind, MirTerminatorKind, MirUnitBuilder,
        MirUnitKind,
    };
    use bray_runtime_interface::BinarySymbolName;
    use bray_symbols::{SemanticValueStore, TypeData};
    use bray_target::{TargetLayoutContract, TargetValueLayout};
    use bray_testing::{test_bound_unit, test_mir_target};

    use crate::test_support::{codegen_partition_compatibility, codegen_request};
    use crate::{
        CodegenCallableSignature, CodegenGenericArgument, CodegenInstance, CodegenInstanceKey,
        CodegenInstanceTypeMapping, CodegenLinkage, CodegenMappings, CodegenPartitionPolicy,
        CodegenResultMapping, CodegenSpecialization, CodegenSymbolKey, CodegenSymbolMapping,
        CodegenTypeKind, CodegenTypeMapping, CodegenUnit, CodegenValueKey,
    };

    #[test]
    fn mappings_are_ordered_independently_of_input_order() {
        let fixture = codegen_request();
        let request = fixture.request();
        let mappings = request.mappings();

        let first = CodegenMappings::new(
            request.unit(),
            request.target(),
            mappings.types().iter().cloned(),
            mappings.instance_types().iter().cloned(),
            mappings.symbols().iter().cloned(),
            mappings.constants().iter().cloned(),
            mappings.constant_terms().iter().cloned(),
            mappings.callables().iter().cloned(),
            mappings.operations().iter().cloned(),
            mappings.static_storages().iter().cloned(),
            mappings.native_storages().iter().cloned(),
            mappings.terminators().iter().cloned(),
            mappings.debug_locations().iter().cloned(),
        );

        let second = CodegenMappings::new(
            request.unit(),
            request.target(),
            mappings.types().iter().rev().cloned(),
            mappings.instance_types().iter().rev().cloned(),
            mappings.symbols().iter().rev().cloned(),
            mappings.constants().iter().rev().cloned(),
            mappings.constant_terms().iter().rev().cloned(),
            mappings.callables().iter().rev().cloned(),
            mappings.operations().iter().rev().cloned(),
            mappings.static_storages().iter().rev().cloned(),
            mappings.native_storages().iter().rev().cloned(),
            mappings.terminators().iter().rev().cloned(),
            mappings.debug_locations().iter().rev().cloned(),
        );

        assert_eq!(first, second);
    }

    #[test]
    fn generic_instances_map_the_same_template_type_independently() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic values must initialize: {error:?}"));

        let open = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("open test type must intern: {error:?}"));

        let first_type = values
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("first test type must intern: {error:?}"));

        let second_type = values
            .intern_type(TypeData::tuple([first_type]))
            .unwrap_or_else(|error| panic!("second test type must intern: {error:?}"));

        let bound = test_bound_unit(71);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            test_mir_target(),
        );

        let _ = builder
            .push_storage(source.clone(), MirStorageKind::Local, open)
            .unwrap_or_else(|error| panic!("test storage must validate: {error:?}"));

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap_or_else(|error| panic!("test block must validate: {error:?}"));

        builder.set_terminator(entry, source, MirTerminatorKind::Return(None));

        let mir = builder.finish(entry);

        let first_key = CodegenInstanceKey::new(
            mir.key().clone(),
            CodegenSpecialization::generic([CodegenGenericArgument::Type(CodegenValueKey::new(
                [1; 32],
            ))]),
            [],
            mir.target().clone(),
        );

        let second_key = CodegenInstanceKey::new(
            mir.key().clone(),
            CodegenSpecialization::generic([CodegenGenericArgument::Type(CodegenValueKey::new(
                [2; 32],
            ))]),
            [],
            mir.target().clone(),
        );

        let first = CodegenInstance::try_new(first_key.clone(), mir.clone(), [])
            .unwrap_or_else(|error| panic!("first instance must validate: {error:?}"));

        let second = CodegenInstance::try_new(second_key.clone(), mir, [])
            .unwrap_or_else(|error| panic!("second instance must validate: {error:?}"));

        let unit = CodegenUnit::try_from_instances(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            codegen_partition_compatibility(),
            [first, second],
        )
        .unwrap_or_else(|error| panic!("test codegen unit must validate: {error:?}"));

        let fixture = codegen_request();
        let target = fixture.request().target();

        let first_layout = TargetValueLayout::new(
            4,
            NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN),
            TargetLayoutContract::Default,
        );

        let second_layout = TargetValueLayout::new(
            8,
            NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN),
            TargetLayoutContract::Default,
        );

        let types = [
            CodegenTypeMapping::new(
                first_type,
                first_layout,
                CodegenTypeKind::SignedInteger(NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN)),
            ),
            CodegenTypeMapping::new(
                second_type,
                second_layout,
                CodegenTypeKind::UnsignedInteger(NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN)),
            ),
        ];

        let symbols = unit
            .instances()
            .iter()
            .enumerate()
            .map(|(index, instance)| {
                let name = BinarySymbolName::try_new(format!("generic_{index}"))
                    .unwrap_or_else(|| panic!("test symbol name must validate"));

                CodegenSymbolMapping::new(
                    CodegenSymbolKey::Instance(instance.key().clone()),
                    name,
                    CodegenLinkage::Internal,
                    CodegenCallableSignature::new(
                        [],
                        CodegenResultMapping::Void,
                        bray_symbols::CallableAbi::Bray,
                        false,
                    ),
                )
            });

        let mappings = CodegenMappings::new(
            &unit,
            target,
            types,
            [
                CodegenInstanceTypeMapping::new(first_key.clone(), open, first_type),
                CodegenInstanceTypeMapping::new(second_key.clone(), open, second_type),
            ],
            symbols,
            [],
            [],
            [],
            [],
            [],
            [],
            [],
            [],
        );

        assert_eq!(
            mappings
                .instance_ty(&first_key, open)
                .map(CodegenTypeMapping::ty),
            Some(first_type)
        );

        assert_eq!(
            mappings
                .instance_ty(&second_key, open)
                .map(CodegenTypeMapping::ty),
            Some(second_type)
        );
    }
}
