use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_ir::{
    MirAsyncOperation, MirCallTarget, MirCallableReference, MirFrameInitializer, MirOperationKind,
    MirSourceAnchor, MirTerminatorKind,
};
use bray_runtime_interface::ProtectedFrameOperation;
use bray_symbols::{ConstantTermId, ConstantValueId, TypeId};

use crate::{
    CodegenCallableMapping, CodegenConstantMapping, CodegenConstantTermMapping,
    CodegenDebugLocation, CodegenOperationMapping, CodegenParameterMapping, CodegenSymbolKey,
    CodegenSymbolMapping, CodegenTarget, CodegenTerminatorMapping, CodegenTypeMapping,
    CodegenUnit, CodegenUnitKey,
};

use super::super::demand::{child_constants, demanded_constant_terms, demanded_constants};
use super::validation::{
    demanded_debug_sources, demanded_runtime_references, demanded_types,
};

/// Canonical code generation facts demanded by one concrete code generation unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodegenMappings {
    unit: CodegenUnitKey,
    target: CodegenTarget,
    types: Arc<[CodegenTypeMapping]>,
    symbols: Arc<[CodegenSymbolMapping]>,
    constants: Arc<[CodegenConstantMapping]>,
    constant_terms: Arc<[CodegenConstantTermMapping]>,
    callables: Arc<[CodegenCallableMapping]>,
    operations: Arc<[CodegenOperationMapping]>,
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
        symbols: impl IntoIterator<Item = CodegenSymbolMapping>,
        constants: impl IntoIterator<Item = CodegenConstantMapping>,
        constant_terms: impl IntoIterator<Item = CodegenConstantTermMapping>,
        callables: impl IntoIterator<Item = CodegenCallableMapping>,
        operations: impl IntoIterator<Item = CodegenOperationMapping>,
        terminators: impl IntoIterator<Item = CodegenTerminatorMapping>,
        debug_locations: impl IntoIterator<Item = CodegenDebugLocation>,
    ) -> Result<Self, CodegenMappingsBuildError> {
        if !target.matches_mir_target(unit.target()) {
            return Err(CodegenMappingsBuildError::TargetMismatch);
        }

        let mut types: Vec<_> = types.into_iter().collect();
        let mut symbols: Vec<_> = symbols.into_iter().collect();
        let mut constants: Vec<_> = constants.into_iter().collect();
        let mut constant_terms: Vec<_> = constant_terms.into_iter().collect();
        let mut callables: Vec<_> = callables.into_iter().collect();
        let mut operations: Vec<_> = operations.into_iter().collect();
        let mut terminators: Vec<_> = terminators.into_iter().collect();
        let mut debug_locations: Vec<_> = debug_locations.into_iter().collect();

        types.sort_unstable_by_key(CodegenTypeMapping::ty);
        symbols.sort_unstable_by(|left, right| left.key().cmp(right.key()));
        constants.sort_unstable_by_key(CodegenConstantMapping::value);
        constant_terms.sort_unstable_by_key(|mapping| mapping.term());
        callables.sort_unstable_by_key(CodegenCallableMapping::reference);
        operations.sort_unstable_by_key(CodegenOperationMapping::operation);
        terminators.sort_unstable_by_key(CodegenTerminatorMapping::block);
        debug_locations.sort_unstable_by(|left, right| left.anchor().cmp(right.anchor()));

        if types.windows(2).any(|pair| pair[0].ty() == pair[1].ty()) {
            return Err(CodegenMappingsBuildError::DuplicateType);
        }

        if symbols
            .windows(2)
            .any(|pair| pair[0].key() == pair[1].key())
        {
            return Err(CodegenMappingsBuildError::DuplicateSymbol);
        }

        if constants
            .windows(2)
            .any(|pair| pair[0].value() == pair[1].value())
        {
            return Err(CodegenMappingsBuildError::DuplicateConstant);
        }

        if constant_terms
            .windows(2)
            .any(|pair| pair[0].term() == pair[1].term())
        {
            return Err(CodegenMappingsBuildError::DuplicateConstantTerm);
        }

        if callables
            .windows(2)
            .any(|pair| pair[0].reference() == pair[1].reference())
        {
            return Err(CodegenMappingsBuildError::DuplicateCallable);
        }

        if operations
            .windows(2)
            .any(|pair| pair[0].operation() == pair[1].operation())
        {
            return Err(CodegenMappingsBuildError::DuplicateOperation);
        }

        if terminators
            .windows(2)
            .any(|pair| pair[0].block() == pair[1].block())
        {
            return Err(CodegenMappingsBuildError::DuplicateTerminator);
        }

        let mut symbol_names: Vec<_> = symbols.iter().map(CodegenSymbolMapping::name).collect();

        symbol_names.sort_unstable();

        if symbol_names.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(CodegenMappingsBuildError::DuplicateBinarySymbolName);
        }

        if debug_locations
            .windows(2)
            .any(|pair| pair[0].anchor() == pair[1].anchor())
        {
            return Err(CodegenMappingsBuildError::DuplicateDebugLocation);
        }

        if symbols
            .iter()
            .any(|symbol| !target.symbols().supports(symbol.linkage()))
        {
            return Err(CodegenMappingsBuildError::UnsupportedLinkage);
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
                CodegenSymbolKey::Runtime(_) | CodegenSymbolKey::ProtectedFrame { .. } => None,
            })
            .collect();

        if actual_instances != expected_instances {
            return Err(CodegenMappingsBuildError::InstanceSymbolCoverageMismatch);
        }

        validate_callable_mappings(unit, &expected_instances, &symbols, &callables)?;
        validate_operation_mappings(unit, &symbols, &operations)?;
        validate_terminator_mappings(unit, &symbols, &terminators)?;
        validate_constant_mappings(unit, &constants, &constant_terms, &terminators)?;

        let expected_runtime_references = demanded_runtime_references(unit);

        let actual_runtime_references: BTreeSet<_> = symbols
            .iter()
            .filter_map(|symbol| match symbol.key() {
                CodegenSymbolKey::Runtime(reference) => Some(*reference),
                CodegenSymbolKey::Instance(_) | CodegenSymbolKey::ProtectedFrame { .. } => None,
            })
            .collect();

        if actual_runtime_references != expected_runtime_references {
            return Err(CodegenMappingsBuildError::RuntimeSymbolCoverageMismatch);
        }

        let expected_frame_operations: BTreeSet<_> = unit
            .instances()
            .iter()
            .filter_map(crate::CodegenInstance::protected_frame_identity)
            .flat_map(|frame| {
                ProtectedFrameOperation::ALL
                    .into_iter()
                    .map(move |operation| (frame, operation))
            })
            .collect();

        let actual_frame_operations: BTreeSet<_> = symbols
            .iter()
            .filter_map(|symbol| match symbol.key() {
                CodegenSymbolKey::ProtectedFrame { frame, operation } => Some((*frame, *operation)),
                CodegenSymbolKey::Instance(_) | CodegenSymbolKey::Runtime(_) => None,
            })
            .collect();

        if actual_frame_operations != expected_frame_operations {
            return Err(CodegenMappingsBuildError::FrameSymbolCoverageMismatch);
        }

        let demanded_types = demanded_types(unit);

        if demanded_types.iter().any(|ty| {
            types
                .binary_search_by_key(ty, CodegenTypeMapping::ty)
                .is_err()
        }) {
            return Err(CodegenMappingsBuildError::TypeCoverageMismatch);
        }

        if symbols.iter().any(|symbol| {
            let signature = symbol.signature();

            signature
                .parameters()
                .iter()
                .flat_map(CodegenParameterMapping::demanded_types)
                .chain(signature.result().demanded_types())
                .flatten()
                .any(|ty| {
                    types
                        .binary_search_by_key(&ty, CodegenTypeMapping::ty)
                        .is_err()
                })
        }) {
            return Err(CodegenMappingsBuildError::TypeCoverageMismatch);
        }

        Ok(Self {
            // The mappings retain immutable structural request identities independently.
            unit: unit.key().clone(),
            target: target.clone(),
            types: types.into(),
            symbols: symbols.into(),
            constants: constants.into(),
            constant_terms: constant_terms.into(),
            callables: callables.into(),
            operations: operations.into(),
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

    /// Returns the demanded mapping for one semantic symbol.
    pub fn symbol(&self, key: &CodegenSymbolKey) -> Option<&CodegenSymbolMapping> {
        self.symbols
            .binary_search_by(|mapping| mapping.key().cmp(key))
            .ok()
            .map(|index| &self.symbols[index])
    }

    /// Returns the materialized mapping for one constant value.
    pub fn constant(&self, value: ConstantValueId) -> Option<&CodegenConstantMapping> {
        self.constants
            .binary_search_by_key(&value, CodegenConstantMapping::value)
            .ok()
            .map(|index| &self.constants[index])
    }

    /// Returns the materialized value selected for one closed constant term.
    pub fn constant_term(&self, term: ConstantTermId) -> Option<ConstantValueId> {
        self.constant_terms
            .binary_search_by_key(&term, |mapping| mapping.term())
            .ok()
            .map(|index| self.constant_terms[index].value())
    }

    /// Returns the concrete instance selected for one semantic callable reference.
    pub fn callable(&self, reference: MirCallableReference) -> Option<&CodegenCallableMapping> {
        self.callables
            .binary_search_by_key(&reference, CodegenCallableMapping::reference)
            .ok()
            .map(|index| &self.callables[index])
    }

    /// Returns ordered helper symbols for one MIR operation.
    pub fn operation(&self, operation: bray_ir::MirOperationId) -> Option<&CodegenOperationMapping> {
        self.operations
            .binary_search_by_key(&operation, CodegenOperationMapping::operation)
            .ok()
            .map(|index| &self.operations[index])
    }

    /// Returns extra realization facts for one block terminator.
    pub fn terminator(&self, block: bray_ir::MirBlockId) -> Option<&CodegenTerminatorMapping> {
        self.terminators
            .binary_search_by_key(&block, CodegenTerminatorMapping::block)
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

/// A contract violation that prevents creation of code generation mappings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodegenMappingsBuildError {
    /// MIR and mapping target contracts do not identify the same target.
    TargetMismatch,
    /// One semantic type appears more than once.
    DuplicateType,
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
    /// One MIR block terminator appears more than once.
    DuplicateTerminator,
    /// Two semantic symbols select the same binary spelling.
    DuplicateBinarySymbolName,
    /// One MIR source anchor appears more than once.
    DuplicateDebugLocation,
    /// One selected linkage is unsupported by the target contract.
    UnsupportedLinkage,
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
    /// Demanded private runtime references do not have exact symbol coverage.
    RuntimeSymbolCoverageMismatch,
    /// Protected-frame descriptors do not have exact operation-symbol coverage.
    FrameSymbolCoverageMismatch,
    /// One directly demanded MIR type has no physical representation.
    TypeCoverageMismatch,
}

fn validate_callable_mappings(
    unit: &CodegenUnit,
    instances: &BTreeSet<&crate::CodegenInstanceKey>,
    symbols: &[CodegenSymbolMapping],
    mappings: &[CodegenCallableMapping],
) -> Result<(), CodegenMappingsBuildError> {
    let expected = demanded_callable_references(unit);

    let actual: BTreeSet<_> = mappings
        .iter()
        .map(CodegenCallableMapping::reference)
        .collect();

    if actual != expected
        || mappings.iter().any(|mapping| {
            if !instances.contains(mapping.instance()) {
                return true;
            }

            let key = CodegenSymbolKey::Instance(mapping.instance().clone());

            symbols
                .binary_search_by(|symbol| symbol.key().cmp(&key))
                .ok()
                .is_none_or(|index| {
                    symbols[index].signature().abi() != mapping.reference().abi()
                })
        })
    {
        return Err(CodegenMappingsBuildError::CallableCoverageMismatch);
    }

    Ok(())
}

fn validate_operation_mappings(
    unit: &CodegenUnit,
    symbols: &[CodegenSymbolMapping],
    mappings: &[CodegenOperationMapping],
) -> Result<(), CodegenMappingsBuildError> {
    let expected: BTreeMap<_, _> = unit
        .mir_units()
        .flat_map(bray_ir::MirUnit::operations_with_ids)
        .filter_map(|(id, operation)| {
            operation_helper_count(operation.kind())
                .filter(|count| *count > 0)
                .map(|count| (id, count))
        })
        .collect();

    if mappings.len() != expected.len()
        || mappings.iter().any(|mapping| {
            expected
                .get(&mapping.operation())
                .is_none_or(|count| *count != mapping.helpers().len())
                || mapping
                    .helpers()
                    .iter()
                    .any(|helper| !contains_symbol(symbols, helper))
        })
    {
        return Err(CodegenMappingsBuildError::OperationCoverageMismatch);
    }

    Ok(())
}

fn validate_terminator_mappings(
    unit: &CodegenUnit,
    symbols: &[CodegenSymbolMapping],
    mappings: &[CodegenTerminatorMapping],
) -> Result<(), CodegenMappingsBuildError> {
    let expected: BTreeMap<_, _> = unit
        .mir_units()
        .flat_map(bray_ir::MirUnit::blocks_with_ids)
        .filter_map(|(id, block)| {
            let (helpers, constants) = terminator_mapping_shape(block.terminator().kind());

            (helpers > 0 || constants > 0).then_some((id, (helpers, constants)))
        })
        .collect();

    if mappings.len() != expected.len()
        || mappings.iter().any(|mapping| {
            expected
                .get(&mapping.block())
                .is_none_or(|(helpers, constants)| {
                    *helpers != mapping.helpers().len()
                        || *constants != mapping.constants().len()
                })
                || mapping
                    .helpers()
                    .iter()
                    .any(|helper| !contains_symbol(symbols, helper))
        })
    {
        return Err(CodegenMappingsBuildError::TerminatorCoverageMismatch);
    }

    Ok(())
}

fn validate_constant_mappings(
    unit: &CodegenUnit,
    mappings: &[CodegenConstantMapping],
    terms: &[CodegenConstantTermMapping],
    terminators: &[CodegenTerminatorMapping],
) -> Result<(), CodegenMappingsBuildError> {
    let demands = demanded_constants(unit);
    let mut expected_values = demands.values().clone();

    if demands.types().iter().any(|(value, types)| {
        types.len() != 1
            || mappings
                .binary_search_by_key(value, CodegenConstantMapping::value)
                .ok()
                .is_none_or(|index| !types.contains(&mappings[index].data().ty()))
    }) {
        return Err(CodegenMappingsBuildError::ConstantCoverageMismatch);
    }

    expected_values.extend(
        terminators
            .iter()
            .flat_map(CodegenTerminatorMapping::constants),
    );

    let expected_terms = demanded_constant_terms(unit);
    let actual_terms: BTreeSet<_> = terms.iter().map(|mapping| mapping.term()).collect();

    if expected_terms != actual_terms {
        return Err(CodegenMappingsBuildError::ConstantCoverageMismatch);
    }

    expected_values.extend(terms.iter().map(|mapping| mapping.value()));

    let actual_values: BTreeSet<_> = mappings
        .iter()
        .map(CodegenConstantMapping::value)
        .collect();

    for mapping in mappings {
        expected_values.extend(child_constants(mapping.data().kind()));
    }

    if expected_values != actual_values {
        return Err(CodegenMappingsBuildError::ConstantCoverageMismatch);
    }

    Ok(())
}

fn contains_symbol(symbols: &[CodegenSymbolMapping], key: &CodegenSymbolKey) -> bool {
    symbols
        .binary_search_by(|symbol| symbol.key().cmp(key))
        .is_ok()
}

fn operation_helper_count(operation: &MirOperationKind) -> Option<usize> {
    match operation {
        MirOperationKind::AnonymousCallable(_) => Some(1),
        MirOperationKind::Construct(construction) => {
            let defaults = construction.runtime_default_count();
            let type_form = usize::from(construction.invokes_type_form());

            Some(defaults + type_form)
        }
        MirOperationKind::Generator(_)
        | MirOperationKind::Finalize(_)
        | MirOperationKind::Destroy(_)
        | MirOperationKind::Cleanup { .. } => Some(1),
        MirOperationKind::Async(
            MirAsyncOperation::CreateFrame { .. }
            | MirAsyncOperation::MoveInactiveFrame { .. }
            | MirAsyncOperation::ComposeAwaitedFrame { .. }
            | MirAsyncOperation::CommitAwaitedCompletion { .. }
            | MirAsyncOperation::DestroyTerminalTask { .. },
        ) => Some(1),
        MirOperationKind::Store { .. }
        | MirOperationKind::Borrow { .. }
        | MirOperationKind::Unary { .. }
        | MirOperationKind::Binary { .. }
        | MirOperationKind::Aggregate(_)
        | MirOperationKind::Convert { .. }
        | MirOperationKind::PatternProjection { .. }
        | MirOperationKind::Call(_)
        | MirOperationKind::PanicReport(_)
        | MirOperationKind::Async(_)
        | MirOperationKind::Host(_) => None,
    }
}

fn terminator_mapping_shape(terminator: &MirTerminatorKind) -> (usize, usize) {
    if terminator.requires_pattern_literal_mapping() {
        (0, 1)
    } else {
        (0, 0)
    }
}

fn demanded_callable_references(unit: &CodegenUnit) -> BTreeSet<MirCallableReference> {
    unit.mir_units()
        .flat_map(|mir| {
            mir.operations()
                .iter()
                .filter_map(|operation| operation_callable_reference(operation.kind()))
                .chain(
                    mir.blocks()
                        .iter()
                        .filter_map(|block| terminator_callable_reference(block.terminator().kind())),
                )
        })
        .collect()
}

fn operation_callable_reference(operation: &MirOperationKind) -> Option<MirCallableReference> {
    match operation {
        MirOperationKind::Call(call)
        | MirOperationKind::Async(MirAsyncOperation::CreateFrame {
            initializer: MirFrameInitializer::Callable(call),
            ..
        }) => call_target_reference(call.target()),
        MirOperationKind::AnonymousCallable(_)
        | MirOperationKind::Store { .. }
        | MirOperationKind::Borrow { .. }
        | MirOperationKind::Unary { .. }
        | MirOperationKind::Binary { .. }
        | MirOperationKind::Aggregate(_)
        | MirOperationKind::Construct(_)
        | MirOperationKind::Convert { .. }
        | MirOperationKind::PatternProjection { .. }
        | MirOperationKind::Generator(_)
        | MirOperationKind::PanicReport(_)
        | MirOperationKind::Finalize(_)
        | MirOperationKind::Destroy(_)
        | MirOperationKind::Cleanup { .. }
        | MirOperationKind::Async(_)
        | MirOperationKind::Host(_) => None,
    }
}

fn terminator_callable_reference(terminator: &MirTerminatorKind) -> Option<MirCallableReference> {
    match terminator {
        MirTerminatorKind::Iterate { next, .. } => Some(*next),
        MirTerminatorKind::Goto(_)
        | MirTerminatorKind::Branch { .. }
        | MirTerminatorKind::PatternBranch { .. }
        | MirTerminatorKind::Switch { .. }
        | MirTerminatorKind::Return(_)
        | MirTerminatorKind::Unreachable
        | MirTerminatorKind::Suspend { .. }
        | MirTerminatorKind::ForwardRunResult { .. }
        | MirTerminatorKind::BeginCleanup(_)
        | MirTerminatorKind::ContinueCleanup(_)
        | MirTerminatorKind::Panic { .. }
        | MirTerminatorKind::CancelCurrentRun { .. } => None,
    }
}

fn call_target_reference(target: &MirCallTarget) -> Option<MirCallableReference> {
    match target {
        MirCallTarget::Direct(reference) => Some(*reference),
        MirCallTarget::Indirect { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use bray_ir::{
        MirAsyncOperation, MirBlockKind, MirOperationKind, MirRuntimeReference, MirSourceAnchor,
        MirTerminatorKind, MirUnitBuilder, MirUnitKind,
    };
    use bray_runtime_interface::{BinarySymbolName, RuntimeAbiRole, RuntimeAbiVersion};
    use bray_symbols::{ConstantValueData, ConstantValueKind, SemanticValueStore, TypeData};
    use bray_testing::{test_bound_unit, test_mir_target, test_mir_type};

    use super::{
        CodegenMappings, CodegenMappingsBuildError, demanded_debug_sources, demanded_types,
    };
    use crate::test_support::codegen_request;
    use crate::{
        CodegenCallableSignature, CodegenConstantMapping, CodegenLinkage, CodegenResultMapping,
        CodegenSymbolKey, CodegenSymbolMapping, CodegenTypeMapping, CodegenUnit,
    };

    #[test]
    fn mappings_are_canonical_independently_of_fact_order() {
        let fixture = codegen_request();
        let request = fixture.request();
        let mappings = request.mappings();

        let first = CodegenMappings::try_new(
            request.unit(),
            request.target(),
            mappings.types().iter().cloned(),
            mappings.symbols().iter().cloned(),
            mappings.constants().iter().cloned(),
            mappings.constant_terms().iter().copied(),
            mappings.callables().iter().cloned(),
            mappings.operations().iter().cloned(),
            mappings.terminators().iter().cloned(),
            mappings.debug_locations().iter().cloned(),
        );

        let second = CodegenMappings::try_new(
            request.unit(),
            request.target(),
            mappings.types().iter().rev().cloned(),
            mappings.symbols().iter().rev().cloned(),
            mappings.constants().iter().rev().cloned(),
            mappings.constant_terms().iter().rev().copied(),
            mappings.callables().iter().rev().cloned(),
            mappings.operations().iter().rev().cloned(),
            mappings.terminators().iter().rev().cloned(),
            mappings.debug_locations().iter().rev().cloned(),
        );

        assert_eq!(first, second);
        assert!(mappings.covers_debug_sources(request.unit()));
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
                direct_symbols,
                mappings.constants().iter().cloned(),
                mappings.constant_terms().iter().copied(),
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
                [],
                mappings.constants().iter().cloned(),
                mappings.constant_terms().iter().copied(),
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
            mappings.symbols().iter().cloned(),
            mappings.constants().iter().cloned(),
            mappings.constant_terms().iter().copied(),
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
            RuntimeAbiVersion::new(1, 0),
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

            CodegenTypeMapping::new(ty, mapping.layout(), mapping.kind().clone())
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
                unsolicited,
                base_mappings.constants().iter().cloned(),
                base_mappings.constant_terms().iter().copied(),
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

        let Ok(unit) = CodegenUnit::try_new(1, [mir]) else {
            panic!("test runtime code generation unit must be valid");
        };

        (unit, result_type)
    }
}
