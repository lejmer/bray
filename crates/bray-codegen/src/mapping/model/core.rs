use std::collections::BTreeSet;
use std::sync::Arc;

use bray_ir::MirSourceAnchor;
use bray_symbols::{CallableAbi, ConstantTermId, ConstantValueData, ConstantValueId, TypeId};

use crate::{
    CodegenCallableMapping, CodegenConstantMapping, CodegenConstantTermMapping,
    CodegenDebugLocation, CodegenInstanceKey, CodegenInstanceTypeMapping,
    CodegenNativeStaticMapping, CodegenOperationMapping, CodegenProductHostMapping,
    CodegenStaticStorageMapping, CodegenSymbolKey, CodegenSymbolMapping, CodegenTarget,
    CodegenTerminatorMapping, CodegenTypeMapping, CodegenUnit, CodegenUnitKey,
};

use super::static_storage::{validate_native_static_mappings, validate_static_storage_mappings};
use super::table_validation::{
    compare_callables, compare_constant_terms, compare_operations, compare_terminators,
    validate_callable_mappings, validate_constant_mappings, validate_operation_mappings,
    validate_terminator_mappings,
};

use super::validation::{
    demanded_debug_sources, mapped_runtime_references, validate_instance_type_structure,
    validate_type_coverage, validate_type_structure,
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
    /// Validates every demanded mapping table for one code generation unit.
    #[expect(
        clippy::too_many_arguments,
        reason = "the constructor validates each independent canonical mapping table explicitly"
    )]
    pub fn try_new(
        unit: &CodegenUnit,
        target: &CodegenTarget,
        types: impl IntoIterator<Item = CodegenTypeMapping>,
        instance_types: impl IntoIterator<Item = CodegenInstanceTypeMapping>,
        symbols: impl IntoIterator<Item = CodegenSymbolMapping>,
        constants: impl IntoIterator<Item = CodegenConstantMapping>,
        constant_terms: impl IntoIterator<Item = CodegenConstantTermMapping>,
        callables: impl IntoIterator<Item = CodegenCallableMapping>,
        operations: impl IntoIterator<Item = CodegenOperationMapping>,
        terminators: impl IntoIterator<Item = CodegenTerminatorMapping>,
        debug_locations: impl IntoIterator<Item = CodegenDebugLocation>,
    ) -> Result<Self, CodegenMappingsBuildError> {
        Self::try_new_with_static_storages(
            unit,
            target,
            types,
            instance_types,
            symbols,
            constants,
            constant_terms,
            callables,
            operations,
            [],
            terminators,
            debug_locations,
        )
    }

    /// Validates every demanded mapping table, including static storage, for one code generation
    /// unit.
    #[expect(
        clippy::too_many_arguments,
        reason = "the constructor validates each independent canonical mapping table explicitly"
    )]
    pub fn try_new_with_static_storages(
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
        terminators: impl IntoIterator<Item = CodegenTerminatorMapping>,
        debug_locations: impl IntoIterator<Item = CodegenDebugLocation>,
    ) -> Result<Self, CodegenMappingsBuildError> {
        Self::try_new_with_storage_mappings(
            unit,
            target,
            types,
            instance_types,
            symbols,
            constants,
            constant_terms,
            callables,
            operations,
            static_storages,
            [],
            terminators,
            debug_locations,
        )
    }

    /// Validates every demanded mapping table, including Bray-owned and native static storage.
    #[expect(
        clippy::too_many_arguments,
        reason = "the constructor validates each independent canonical mapping table explicitly"
    )]
    pub fn try_new_with_storage_mappings(
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
    ) -> Result<Self, CodegenMappingsBuildError> {
        if !target.matches_mir_target(unit.target()) {
            return Err(CodegenMappingsBuildError::TargetMismatch);
        }

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
        constant_terms.sort_unstable_by(compare_constant_terms);
        callables.sort_unstable_by(compare_callables);
        operations.sort_unstable_by(compare_operations);
        static_storages.sort_unstable();
        native_storages.sort_unstable();
        terminators.sort_unstable_by(compare_terminators);
        debug_locations.sort_unstable_by(|left, right| left.anchor().cmp(right.anchor()));

        validate_type_structure(&types)?;
        validate_instance_type_structure(unit, &types, &instance_types)?;

        if symbols
            .windows(2)
            .any(|pair| pair[0].key() == pair[1].key())
        {
            return Err(CodegenMappingsBuildError::DuplicateSymbol);
        }

        if constants.windows(2).any(|pair| {
            pair[0].value() == pair[1].value()
                && pair[0].representation() == pair[1].representation()
        }) {
            return Err(CodegenMappingsBuildError::DuplicateConstant);
        }

        if constant_terms
            .windows(2)
            .any(|pair| compare_constant_terms(&pair[0], &pair[1]).is_eq())
        {
            return Err(CodegenMappingsBuildError::DuplicateConstantTerm);
        }

        if callables
            .windows(2)
            .any(|pair| compare_callables(&pair[0], &pair[1]).is_eq())
        {
            return Err(CodegenMappingsBuildError::DuplicateCallable);
        }

        if operations
            .windows(2)
            .any(|pair| compare_operations(&pair[0], &pair[1]).is_eq())
        {
            return Err(CodegenMappingsBuildError::DuplicateOperation);
        }

        if static_storages.windows(2).any(|pair| {
            pair[0].owner() == pair[1].owner() && pair[0].storage() == pair[1].storage()
        }) {
            return Err(CodegenMappingsBuildError::DuplicateStaticStorage);
        }

        if native_storages.windows(2).any(|pair| {
            pair[0].owner() == pair[1].owner() && pair[0].storage() == pair[1].storage()
        }) {
            return Err(CodegenMappingsBuildError::DuplicateNativeStaticStorage);
        }

        if terminators
            .windows(2)
            .any(|pair| compare_terminators(&pair[0], &pair[1]).is_eq())
        {
            return Err(CodegenMappingsBuildError::DuplicateTerminator);
        }

        validate_symbol_structure(&symbols, target)?;

        if debug_locations
            .windows(2)
            .any(|pair| pair[0].anchor() == pair[1].anchor())
        {
            return Err(CodegenMappingsBuildError::DuplicateDebugLocation);
        }

        let expected_instances: BTreeSet<_> = unit
            .instances()
            .iter()
            .map(|instance| instance.key())
            .chain(unit.external_instances())
            .collect();

        let actual_instances: BTreeSet<_> = symbols
            .iter()
            .filter_map(|symbol| match symbol.key() {
                CodegenSymbolKey::Instance(instance) => Some(instance),
                CodegenSymbolKey::CleanupFrameConstructor(_)
                | CodegenSymbolKey::Runtime(_)
                | CodegenSymbolKey::ProtectedFrame { .. } => None,
            })
            .collect();

        if actual_instances != expected_instances {
            return Err(CodegenMappingsBuildError::InstanceSymbolCoverageMismatch);
        }

        validate_static_storage_mappings(
            unit,
            &expected_instances,
            &symbols,
            &constants,
            &static_storages,
            &native_storages,
        )?;

        validate_native_static_mappings(unit, &native_storages)?;
        validate_callable_mappings(unit, &expected_instances, &symbols, &callables)?;
        validate_operation_mappings(unit, &expected_instances, &symbols, &operations)?;
        validate_terminator_mappings(unit, &terminators)?;

        validate_constant_mappings(
            unit,
            &types,
            &constants,
            &constant_terms,
            &terminators,
            &static_storages,
        )?;

        let expected_runtime_references = mapped_runtime_references(unit, &operations, &symbols);

        let actual_runtime_references: BTreeSet<_> = symbols
            .iter()
            .filter_map(|symbol| match symbol.key() {
                CodegenSymbolKey::Runtime(reference) => Some(*reference),
                CodegenSymbolKey::CleanupFrameConstructor(_)
                | CodegenSymbolKey::Instance(_)
                | CodegenSymbolKey::ProtectedFrame { .. } => None,
            })
            .collect();

        if actual_runtime_references != expected_runtime_references {
            return Err(CodegenMappingsBuildError::RuntimeSymbolCoverageMismatch);
        }

        super::frame_demand::validate_frame_operations(
            unit,
            &operations,
            &static_storages,
            &symbols,
        )?;

        validate_type_coverage(unit, &types, &instance_types, &symbols, &constants)?;

        Ok(Self {
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
        })
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

    /// Attaches the validated loaded-product host mapping.
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
                CodegenSymbolKey::CleanupFrameConstructor(_)
                | CodegenSymbolKey::Runtime(_)
                | CodegenSymbolKey::ProtectedFrame { .. } => std::cmp::Ordering::Greater,
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

    pub(crate) fn covers_debug_sources(&self, unit: &CodegenUnit) -> bool {
        demanded_debug_sources(unit).iter().all(|anchor| {
            self.debug_locations
                .binary_search_by(|location| location.anchor().cmp(anchor))
                .is_ok()
        })
    }
}

fn validate_symbol_structure(
    symbols: &[CodegenSymbolMapping],
    target: &CodegenTarget,
) -> Result<(), CodegenMappingsBuildError> {
    let mut names: Vec<_> = symbols
        .iter()
        .flat_map(|symbol| {
            std::iter::once(symbol.name()).chain(
                symbol
                    .native_entry()
                    .map(crate::CodegenNativeEntryMapping::name),
            )
        })
        .collect();

    names.sort_unstable();

    if names.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(CodegenMappingsBuildError::DuplicateBinarySymbolName);
    }

    if symbols.iter().any(|symbol| {
        !target.symbols().supports(symbol.linkage())
            || symbol
                .native_entry()
                .is_some_and(|entry| !target.symbols().supports(entry.linkage()))
    }) {
        return Err(CodegenMappingsBuildError::UnsupportedLinkage);
    }

    if symbols.iter().any(|symbol| {
        symbol.native_entry().is_some_and(|entry| {
            !matches!(symbol.key(), CodegenSymbolKey::Instance(_))
                || symbol.name() == entry.name()
                || symbol.linkage() == crate::CodegenLinkage::Import
                || !matches!(
                    entry.linkage(),
                    crate::CodegenLinkage::Export | crate::CodegenLinkage::Weak
                )
                || matches!(symbol.signature().abi(), CallableAbi::Bray)
        })
    }) {
        return Err(CodegenMappingsBuildError::InvalidNativeEntry);
    }

    Ok(())
}

/// A contract violation that prevents creation of code generation mappings.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CodegenMappingsBuildError {
    /// MIR and mapping target contracts do not identify the same target.
    TargetMismatch,
    /// One semantic type appears more than once.
    DuplicateType,
    /// One MIR template type is substituted more than once for the same concrete instance.
    DuplicateInstanceType,
    /// An instance-local type substitution names an unknown instance or concrete type.
    InvalidInstanceType,
    /// A sized type kind lacks a layout or an unsized type kind declares one.
    InvalidTypeLayout,
    /// A callable signature passes an unsized semantic type by value.
    InvalidAbiTypeLayout,
    /// One semantic symbol appears more than once.
    DuplicateSymbol,
    /// One semantic constant value appears more than once.
    DuplicateConstant,
    /// One closed constant term appears more than once.
    DuplicateConstantTerm,
    /// One semantic callable reference appears more than once.
    DuplicateCallable,
    /// One MIR operation appears more than once.
    DuplicateOperation,
    /// One concrete MIR static storage root appears more than once.
    DuplicateStaticStorage,
    /// One concrete MIR native static storage root appears more than once.
    DuplicateNativeStaticStorage,
    /// Concrete MIR static storage roots do not have exact realization coverage.
    StaticStorageCoverageMismatch,
    /// One static storage realization does not match its MIR use or concrete dependencies.
    InvalidStaticStorage,
    /// Concrete MIR native static roots do not have exact mapping coverage.
    NativeStaticStorageCoverageMismatch,
    /// One native static mapping does not match its MIR storage contract.
    InvalidNativeStaticStorage,
    /// One MIR block terminator appears more than once.
    DuplicateTerminator,
    /// Two semantic symbols select the same binary spelling.
    DuplicateBinarySymbolName,
    /// One MIR source anchor appears more than once.
    DuplicateDebugLocation,
    /// One selected linkage is unsupported by the target contract.
    UnsupportedLinkage,
    /// One native entry does not identify a foreign callback definition.
    InvalidNativeEntry,
    /// Concrete local and external definitions do not have exact symbol coverage.
    InstanceSymbolCoverageMismatch,
    /// Callable references do not map exactly to compatible concrete instances.
    CallableCoverageMismatch,
    /// Operations do not have exact ordered helper coverage.
    OperationCoverageMismatch,
    /// Terminators do not have exact helper and literal coverage.
    TerminatorCoverageMismatch,
    /// Demanded constants and closed terms are not completely materialized.
    ConstantCoverageMismatch,
    /// A constant mapping selects a representation incompatible with its semantic value.
    InvalidConstantRepresentation,
    /// Demanded private runtime references do not have exact symbol coverage.
    RuntimeSymbolCoverageMismatch,
    /// Protected-frame descriptors do not have exact operation-symbol coverage.
    FrameSymbolCoverageMismatch,
    /// One directly demanded MIR type has no code generation representation.
    TypeCoverageMismatch,
}

#[cfg(test)]
mod tests {
    use std::num::{NonZeroU16, NonZeroU64};

    use bray_ir::{
        MirAsyncOperation, MirBlockKind, MirOperand, MirOperationKind, MirPlace,
        MirRuntimeReference, MirSourceAnchor, MirStorageKind, MirStoreKind, MirTerminatorKind,
        MirUnitBuilder, MirUnitKind,
    };
    use bray_runtime_interface::{BinarySymbolName, RuntimeAbiRole, RuntimeAbiVersion};
    use bray_symbols::{
        BorrowKind, ConstantValueData, ConstantValueKind, SemanticValueStore, TypeData,
    };
    use bray_target::{TargetLayoutContract, TargetValueLayout};
    use bray_testing::{test_bound_unit, test_mir_target, test_mir_type};

    use super::super::table_validation::valid_constant_representation;
    use super::{CodegenMappings, CodegenMappingsBuildError, demanded_debug_sources};
    use crate::demanded_types;
    use crate::test_support::{codegen_partition_compatibility, codegen_request};
    use crate::{
        CodegenCallableSignature, CodegenConstantMapping, CodegenGenericArgument, CodegenInstance,
        CodegenInstanceKey, CodegenInstanceTypeMapping, CodegenLinkage, CodegenPartitionPolicy,
        CodegenResultMapping, CodegenSpecialization, CodegenSymbolKey, CodegenSymbolMapping,
        CodegenTypeBehavior, CodegenTypeKind, CodegenTypeMapping, CodegenUnit, CodegenValueKey,
        TargetAddressSpaceKind,
    };

    #[test]
    fn mappings_are_canonical_independently_of_input_order() {
        let fixture = codegen_request();
        let request = fixture.request();
        let mappings = request.mappings();

        let first = CodegenMappings::try_new(
            request.unit(),
            request.target(),
            mappings.types().iter().cloned(),
            mappings.instance_types().iter().cloned(),
            mappings.symbols().iter().cloned(),
            mappings.constants().iter().cloned(),
            mappings.constant_terms().iter().cloned(),
            mappings.callables().iter().cloned(),
            mappings.operations().iter().cloned(),
            mappings.terminators().iter().cloned(),
            mappings.debug_locations().iter().cloned(),
        );

        let second = CodegenMappings::try_new(
            request.unit(),
            request.target(),
            mappings.types().iter().rev().cloned(),
            mappings.instance_types().iter().rev().cloned(),
            mappings.symbols().iter().rev().cloned(),
            mappings.constants().iter().rev().cloned(),
            mappings.constant_terms().iter().rev().cloned(),
            mappings.callables().iter().rev().cloned(),
            mappings.operations().iter().rev().cloned(),
            mappings.terminators().iter().rev().cloned(),
            mappings.debug_locations().iter().rev().cloned(),
        );

        assert_eq!(first, second);
        assert!(mappings.covers_debug_sources(request.unit()));
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

        builder
            .set_terminator(entry, source, MirTerminatorKind::Return(None))
            .unwrap_or_else(|error| panic!("test terminator must validate: {error:?}"));

        let mir = builder
            .finish(entry)
            .unwrap_or_else(|error| panic!("test MIR must validate: {error:?}"));

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

        let mappings = CodegenMappings::try_new(
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
        )
        .unwrap_or_else(|error| panic!("instance type mappings must validate: {error:?}"));

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

    #[test]
    fn mappings_reject_unsolicited_constant_realizations() {
        let fixture = codegen_request();
        let request = fixture.request();
        let mappings = request.mappings();

        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("test semantic store must be available");
        };

        let Ok(ty) = store.intern_type(TypeData::Error) else {
            panic!("test type must intern");
        };

        let data = ConstantValueData::new(ty, ConstantValueKind::Error);

        let Ok(value) = store.intern_constant_value(data.clone()) else {
            panic!("test constant must intern");
        };

        assert_eq!(
            CodegenMappings::try_new(
                request.unit(),
                request.target(),
                mappings.types().iter().cloned(),
                mappings.instance_types().iter().cloned(),
                mappings.symbols().iter().cloned(),
                [CodegenConstantMapping::new(value, data)],
                [],
                [],
                [],
                [],
                mappings.debug_locations().iter().cloned(),
            ),
            Err(CodegenMappingsBuildError::ConstantCoverageMismatch)
        );
    }

    #[test]
    fn constant_representations_are_limited_to_owned_values_and_string_borrows() {
        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store must initialize: {error:?}"));

        let string = store
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("string type must intern: {error:?}"));

        let borrowed_string = store
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Shared,
                target: string,
            })
            .unwrap_or_else(|error| panic!("borrowed string type must intern: {error:?}"));

        let unrelated = store
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("unrelated type must intern: {error:?}"));

        let layout = TargetValueLayout::new(
            8,
            NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN),
            TargetLayoutContract::Default,
        );

        let mut types = vec![
            CodegenTypeMapping::new(string, layout, CodegenTypeKind::aggregate([]))
                .with_behavior(Some(CodegenTypeBehavior::String)),
            CodegenTypeMapping::new(
                borrowed_string,
                layout,
                CodegenTypeKind::Pointer {
                    target: string,
                    address_space: TargetAddressSpaceKind::Default,
                },
            ),
            CodegenTypeMapping::new(
                unrelated,
                layout,
                CodegenTypeKind::UnsignedInteger(NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN)),
            ),
        ];

        types.sort_unstable_by_key(CodegenTypeMapping::ty);

        let data = ConstantValueData::new(string, ConstantValueKind::string("text"));

        let value = store
            .intern_constant_value(data.clone())
            .unwrap_or_else(|error| panic!("string value must intern: {error:?}"));

        let owned = CodegenConstantMapping::new(value, data.clone());

        let borrowed =
            CodegenConstantMapping::with_representation(value, data.clone(), borrowed_string);

        let invalid = CodegenConstantMapping::with_representation(value, data, unrelated);

        assert!(valid_constant_representation(&owned, &types));
        assert!(valid_constant_representation(&borrowed, &types));
        assert!(!valid_constant_representation(&invalid, &types));
    }

    #[test]
    fn mappings_reject_constants_retagged_as_unrelated_demanded_types() {
        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic store must initialize: {error:?}"));

        let string = store
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("string type must intern: {error:?}"));

        let unrelated = store
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("unrelated type must intern: {error:?}"));

        let data = ConstantValueData::new(string, ConstantValueKind::string("text"));

        let value = store
            .intern_constant_value(data.clone())
            .unwrap_or_else(|error| panic!("string value must intern: {error:?}"));

        let bound = test_bound_unit(396);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            test_mir_target(),
        );

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap_or_else(|error| panic!("constant test block must build: {error:?}"));

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Local, unrelated)
            .unwrap_or_else(|error| panic!("constant test storage must build: {error:?}"));

        builder
            .push_operation(
                entry,
                source.clone(),
                MirOperationKind::Store {
                    kind: MirStoreKind::Initialize,
                    destination: MirPlace::new(storage, [], unrelated),
                    value: MirOperand::Constant {
                        value,
                        ty: unrelated,
                    },
                },
                None,
            )
            .unwrap_or_else(|error| panic!("constant test store must build: {error:?}"));

        builder
            .set_terminator(entry, source, MirTerminatorKind::Return(None))
            .unwrap_or_else(|error| panic!("constant test return must build: {error:?}"));

        let mir = builder
            .finish(entry)
            .unwrap_or_else(|error| panic!("constant test MIR must validate: {error:?}"));

        let unit = CodegenUnit::try_new(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            codegen_partition_compatibility(),
            [mir],
        )
        .unwrap_or_else(|error| panic!("constant test unit must validate: {error:?}"));

        let fixture = codegen_request();
        let request = fixture.request();
        let target = request.target();

        let layout = TargetValueLayout::new(
            8,
            NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN),
            TargetLayoutContract::Default,
        );

        let types = [
            CodegenTypeMapping::new(string, layout, CodegenTypeKind::aggregate([]))
                .with_behavior(Some(CodegenTypeBehavior::String)),
            CodegenTypeMapping::new(
                unrelated,
                layout,
                CodegenTypeKind::UnsignedInteger(NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN)),
            ),
        ];

        let name = BinarySymbolName::try_new("invalid_constant_representation")
            .unwrap_or_else(|| panic!("constant test symbol must validate"));

        let symbol = CodegenSymbolMapping::new(
            CodegenSymbolKey::Instance(unit.instances()[0].key().clone()),
            name,
            CodegenLinkage::Internal,
            CodegenCallableSignature::new(
                [],
                CodegenResultMapping::Void,
                bray_symbols::CallableAbi::Bray,
                false,
            ),
        );

        assert_eq!(
            CodegenMappings::try_new(
                &unit,
                target,
                types,
                [],
                [symbol],
                [CodegenConstantMapping::with_representation(
                    value, data, unrelated,
                )],
                [],
                [],
                [],
                [],
                [],
            ),
            Err(CodegenMappingsBuildError::InvalidConstantRepresentation)
        );
    }

    #[test]
    fn mappings_reject_layouts_that_disagree_with_sizedness() {
        let fixture = codegen_request();
        let request = fixture.request();
        let mappings = request.mappings();
        let invalid_type = mappings.types()[0].ty();

        let types = mappings.types().iter().map(|mapping| {
            if mapping.ty() == invalid_type {
                CodegenTypeMapping::new_unsized(invalid_type, mapping.kind().clone())
            } else {
                mapping.clone()
            }
        });

        assert_eq!(
            CodegenMappings::try_new(
                request.unit(),
                request.target(),
                types,
                mappings.instance_types().iter().cloned(),
                mappings.symbols().iter().cloned(),
                mappings.constants().iter().cloned(),
                mappings.constant_terms().iter().cloned(),
                mappings.callables().iter().cloned(),
                mappings.operations().iter().cloned(),
                mappings.terminators().iter().cloned(),
                mappings.debug_locations().iter().cloned(),
            ),
            Err(CodegenMappingsBuildError::InvalidTypeLayout)
        );
    }

    #[test]
    fn mappings_reject_unsized_direct_abi_values() {
        let fixture = codegen_request();
        let request = fixture.request();
        let mappings = request.mappings();
        let ty = mappings.types()[0].ty();

        let types = mappings.types().iter().map(|mapping| {
            if mapping.ty() == ty {
                CodegenTypeMapping::new_unsized(ty, CodegenTypeKind::UnsizedSlice { element: ty })
            } else {
                mapping.clone()
            }
        });

        let symbols = mappings.symbols().iter().map(|mapping| {
            CodegenSymbolMapping::new(
                mapping.key().clone(),
                mapping.name().clone(),
                mapping.linkage(),
                CodegenCallableSignature::new(
                    [crate::CodegenParameterMapping::direct(ty, None, [])],
                    CodegenResultMapping::Void,
                    mapping.signature().abi(),
                    false,
                ),
            )
        });

        assert_eq!(
            CodegenMappings::try_new(
                request.unit(),
                request.target(),
                types,
                mappings.instance_types().iter().cloned(),
                symbols,
                mappings.constants().iter().cloned(),
                mappings.constant_terms().iter().cloned(),
                mappings.callables().iter().cloned(),
                mappings.operations().iter().cloned(),
                mappings.terminators().iter().cloned(),
                mappings.debug_locations().iter().cloned(),
            ),
            Err(CodegenMappingsBuildError::InvalidAbiTypeLayout)
        );
    }

    #[test]
    fn mappings_require_exact_instance_and_direct_type_coverage() {
        let fixture = codegen_request();
        let request = fixture.request();
        let mappings = request.mappings();
        let direct_type = mappings.types()[0].ty();

        let direct_symbols = mappings.symbols().iter().map(|mapping| {
            CodegenSymbolMapping::new(
                mapping.key().clone(),
                mapping.name().clone(),
                mapping.linkage(),
                CodegenCallableSignature::new(
                    [],
                    CodegenResultMapping::direct(direct_type, None, []),
                    mapping.signature().abi(),
                    false,
                ),
            )
        });

        assert_eq!(
            CodegenMappings::try_new(
                request.unit(),
                request.target(),
                [],
                mappings.instance_types().iter().cloned(),
                direct_symbols,
                mappings.constants().iter().cloned(),
                mappings.constant_terms().iter().cloned(),
                mappings.callables().iter().cloned(),
                mappings.operations().iter().cloned(),
                mappings.terminators().iter().cloned(),
                mappings.debug_locations().iter().cloned(),
            ),
            Err(CodegenMappingsBuildError::TypeCoverageMismatch)
        );

        assert_eq!(
            CodegenMappings::try_new(
                request.unit(),
                request.target(),
                mappings.types().iter().cloned(),
                mappings.instance_types().iter().cloned(),
                [],
                mappings.constants().iter().cloned(),
                mappings.constant_terms().iter().cloned(),
                mappings.callables().iter().cloned(),
                mappings.operations().iter().cloned(),
                mappings.terminators().iter().cloned(),
                mappings.debug_locations().iter().cloned(),
            ),
            Err(CodegenMappingsBuildError::InstanceSymbolCoverageMismatch)
        );

        let Ok(without_debug) = CodegenMappings::try_new(
            request.unit(),
            request.target(),
            mappings.types().iter().cloned(),
            mappings.instance_types().iter().cloned(),
            mappings.symbols().iter().cloned(),
            mappings.constants().iter().cloned(),
            mappings.constant_terms().iter().cloned(),
            mappings.callables().iter().cloned(),
            mappings.operations().iter().cloned(),
            mappings.terminators().iter().cloned(),
            [],
        ) else {
            panic!("debug mappings are optional until debug output is requested");
        };

        assert!(!without_debug.covers_debug_sources(request.unit()));
    }

    #[test]
    fn runtime_roles_use_exact_typed_symbol_mappings() {
        let reference = MirRuntimeReference::new(
            RuntimeAbiRole::CurrentRunCancellationObservation,
            RuntimeAbiVersion::CURRENT,
        );

        let (unit, result_type) = runtime_unit(reference);

        let fixture = codegen_request();
        let request = fixture.request();
        let target = request.target();
        let base_mappings = request.mappings();

        let Some(name) = BinarySymbolName::try_new("__bray_runtime_cancellation_observation")
        else {
            panic!("test runtime symbol name must be valid");
        };

        let runtime = CodegenSymbolMapping::new(
            CodegenSymbolKey::Runtime(reference),
            name,
            CodegenLinkage::Import,
            CodegenCallableSignature::new(
                [],
                CodegenResultMapping::direct(result_type, None, []),
                bray_symbols::CallableAbi::Bray,
                false,
            ),
        );

        let base_instance = &base_mappings.symbols()[0];

        let instance = CodegenSymbolMapping::new(
            CodegenSymbolKey::Instance(unit.instances()[0].key().clone()),
            base_instance.name().clone(),
            base_instance.linkage(),
            CodegenCallableSignature::new(
                [],
                CodegenResultMapping::direct(result_type, None, []),
                bray_symbols::CallableAbi::Bray,
                false,
            ),
        );

        let types = demanded_types(&unit).into_iter().map(|ty| {
            let mapping = &base_mappings.types()[0];

            match mapping.layout() {
                Some(layout) => CodegenTypeMapping::new(ty, layout, mapping.kind().clone()),
                None => CodegenTypeMapping::new_unsized(ty, mapping.kind().clone()),
            }
        });

        let debug_locations = demanded_debug_sources(&unit).into_iter().map(|anchor| {
            let location = &base_mappings.debug_locations()[0];

            crate::CodegenDebugLocation::new(
                anchor,
                location.file().clone(),
                location.line(),
                location.column(),
            )
        });

        let mapped = match CodegenMappings::try_new(
            &unit,
            target,
            types,
            [],
            [instance, runtime.clone()],
            [],
            [],
            [],
            [],
            [],
            debug_locations,
        ) {
            Ok(mapped) => mapped,
            Err(error) => panic!("typed runtime mapping must validate: {error:?}"),
        };

        let key = CodegenSymbolKey::Runtime(reference);

        let Some(symbol) = mapped.symbol(&key) else {
            panic!("runtime role must resolve by typed identity");
        };

        assert_eq!(
            symbol.name().as_str(),
            "__bray_runtime_cancellation_observation"
        );

        let unsolicited = base_mappings.symbols().iter().cloned().chain([runtime]);

        assert_eq!(
            CodegenMappings::try_new(
                request.unit(),
                target,
                base_mappings.types().iter().cloned(),
                base_mappings.instance_types().iter().cloned(),
                unsolicited,
                base_mappings.constants().iter().cloned(),
                base_mappings.constant_terms().iter().cloned(),
                base_mappings.callables().iter().cloned(),
                base_mappings.operations().iter().cloned(),
                base_mappings.terminators().iter().cloned(),
                base_mappings.debug_locations().iter().cloned(),
            ),
            Err(CodegenMappingsBuildError::RuntimeSymbolCoverageMismatch)
        );
    }

    fn runtime_unit(reference: MirRuntimeReference) -> (CodegenUnit, bray_symbols::TypeId) {
        let bound = test_bound_unit(12);
        let source = MirSourceAnchor::from(bound.key().source());
        let result_type = test_mir_type();

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            test_mir_target(),
        );

        let Ok(entry) = builder.push_block(source.clone(), MirBlockKind::Ordinary) else {
            panic!("test runtime block must be valid");
        };

        let operation = MirOperationKind::Async(MirAsyncOperation::ObserveCurrentRunCancellation {
            runtime: reference,
        });

        if let Err(error) =
            builder.push_operation(entry, source.clone(), operation, Some(result_type))
        {
            panic!("test runtime operation must be valid: {error:?}");
        }

        let Ok(()) = builder.set_terminator(entry, source, MirTerminatorKind::Return(None)) else {
            panic!("test runtime terminator must be valid");
        };

        let Ok(mir) = builder.finish(entry) else {
            panic!("test runtime MIR must be valid");
        };

        let Ok(unit) = CodegenUnit::try_new(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            codegen_partition_compatibility(),
            [mir],
        ) else {
            panic!("test runtime code generation unit must be valid");
        };

        (unit, result_type)
    }
}
