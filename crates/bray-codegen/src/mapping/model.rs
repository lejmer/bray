use std::collections::BTreeSet;
use std::sync::Arc;

use bray_ir::{
    MirAsyncOperation, MirHostOperation, MirOperationKind, MirRuntimeReference, MirSourceAnchor,
    MirTerminatorKind,
};
use bray_runtime_interface::ProtectedFrameOperation;
use bray_symbols::TypeId;

use super::{CodegenDebugLocation, CodegenSymbolKey, CodegenSymbolMapping, CodegenTypeMapping};
use crate::{CodegenTarget, CodegenUnit, CodegenUnitKey};

/// Canonical code generation facts demanded by one concrete code generation unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodegenMappings {
    unit: CodegenUnitKey,
    target: CodegenTarget,
    types: Arc<[CodegenTypeMapping]>,
    symbols: Arc<[CodegenSymbolMapping]>,
    debug_locations: Arc<[CodegenDebugLocation]>,
}

impl CodegenMappings {
    /// Validates and stores demanded mappings in canonical key order.
    pub fn try_new(
        unit: &CodegenUnit,
        target: &CodegenTarget,
        types: impl IntoIterator<Item = CodegenTypeMapping>,
        symbols: impl IntoIterator<Item = CodegenSymbolMapping>,
        debug_locations: impl IntoIterator<Item = CodegenDebugLocation>,
    ) -> Result<Self, CodegenMappingsBuildError> {
        if !target.matches_mir_target(unit.target()) {
            return Err(CodegenMappingsBuildError::TargetMismatch);
        }

        let mut types: Vec<_> = types.into_iter().collect();
        let mut symbols: Vec<_> = symbols.into_iter().collect();
        let mut debug_locations: Vec<_> = debug_locations.into_iter().collect();

        types.sort_unstable_by_key(CodegenTypeMapping::ty);
        symbols.sort_unstable_by(|left, right| left.key().cmp(right.key()));
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
                .flat_map(super::CodegenParameterMapping::demanded_types)
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
    /// Two semantic symbols select the same binary spelling.
    DuplicateBinarySymbolName,
    /// One MIR source anchor appears more than once.
    DuplicateDebugLocation,
    /// One selected linkage is unsupported by the target contract.
    UnsupportedLinkage,
    /// Concrete local and external definitions do not have exact symbol coverage.
    InstanceSymbolCoverageMismatch,
    /// Demanded private runtime references do not have exact symbol coverage.
    RuntimeSymbolCoverageMismatch,
    /// Protected-frame descriptors do not have exact operation-symbol coverage.
    FrameSymbolCoverageMismatch,
    /// One directly demanded MIR type has no physical representation.
    TypeCoverageMismatch,
}

pub(crate) fn demanded_types(unit: &CodegenUnit) -> BTreeSet<TypeId> {
    unit.mir_units()
        .flat_map(|mir| {
            mir.storages()
                .iter()
                .map(bray_ir::MirStorage::ty)
                .chain(mir.values().iter().map(bray_ir::MirValue::ty))
        })
        .collect()
}

pub(crate) fn demanded_debug_sources(unit: &CodegenUnit) -> BTreeSet<MirSourceAnchor> {
    unit.mir_units()
        .flat_map(|mir| {
            mir.blocks()
                .iter()
                .map(bray_ir::MirBlock::source)
                .chain(mir.storages().iter().map(bray_ir::MirStorage::source))
                .chain(mir.values().iter().map(bray_ir::MirValue::source))
                .chain(mir.operations().iter().map(bray_ir::MirOperation::source))
                .chain(mir.blocks().iter().map(|block| block.terminator().source()))
        })
        .cloned()
        .collect()
}

fn demanded_runtime_references(unit: &CodegenUnit) -> BTreeSet<MirRuntimeReference> {
    unit.mir_units()
        .flat_map(|mir| {
            mir.operations()
                .iter()
                .flat_map(|operation| operation_runtime_references(operation.kind()))
                .chain(
                    mir.blocks()
                        .iter()
                        .flat_map(|block| terminator_runtime_references(block.terminator().kind())),
                )
        })
        .flatten()
        .collect()
}

fn operation_runtime_references(operation: &MirOperationKind) -> [Option<MirRuntimeReference>; 2] {
    match operation {
        MirOperationKind::Async(MirAsyncOperation::StartTask {
            allocation, start, ..
        }) => [Some(*allocation), Some(*start)],
        MirOperationKind::Async(
            MirAsyncOperation::ResumeFrame { runtime, .. }
            | MirAsyncOperation::RequestTaskCancellation { runtime, .. }
            | MirAsyncOperation::ObserveCurrentRunCancellation { runtime }
            | MirAsyncOperation::ResolveTask { runtime, .. }
            | MirAsyncOperation::PublishTerminalState { runtime, .. }
            | MirAsyncOperation::ExecuteCleanupBroadcast { runtime, .. }
            | MirAsyncOperation::ExecuteLifecycleResolution { runtime, .. }
            | MirAsyncOperation::TransferCleanupIncident { runtime, .. },
        )
        | MirOperationKind::Host(
            MirHostOperation::ExecuteRoot { runtime, .. }
            | MirHostOperation::RequestRootCancellation { runtime }
            | MirHostOperation::ObserveRootTerminal { runtime }
            | MirHostOperation::ReportCleanupIncidents { runtime }
            | MirHostOperation::StructuredShutdown { runtime },
        ) => [Some(*runtime), None],
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
        | MirOperationKind::Call(_)
        | MirOperationKind::PanicReport(_)
        | MirOperationKind::Finalize(_)
        | MirOperationKind::Destroy(_)
        | MirOperationKind::Cleanup { .. }
        | MirOperationKind::Async(
            MirAsyncOperation::CreateFrame { .. }
            | MirAsyncOperation::MoveInactiveFrame { .. }
            | MirAsyncOperation::ComposeAwaitedFrame { .. }
            | MirAsyncOperation::CommitAwaitedCompletion { .. }
            | MirAsyncOperation::DestroyTerminalTask { .. },
        ) => [None, None],
    }
}

fn terminator_runtime_references(
    terminator: &MirTerminatorKind,
) -> [Option<MirRuntimeReference>; 2] {
    match terminator {
        MirTerminatorKind::Suspend {
            registration, wake, ..
        } => [Some(*registration), Some(*wake)],
        MirTerminatorKind::Goto(_)
        | MirTerminatorKind::Branch { .. }
        | MirTerminatorKind::PatternBranch { .. }
        | MirTerminatorKind::Iterate { .. }
        | MirTerminatorKind::Switch { .. }
        | MirTerminatorKind::Return(_)
        | MirTerminatorKind::Unreachable
        | MirTerminatorKind::ForwardRunResult { .. }
        | MirTerminatorKind::BeginCleanup(_)
        | MirTerminatorKind::ContinueCleanup(_)
        | MirTerminatorKind::Panic { .. }
        | MirTerminatorKind::CancelCurrentRun { .. } => [None, None],
    }
}

#[cfg(test)]
mod tests {
    use bray_ir::{
        MirAsyncOperation, MirBlockKind, MirOperationKind, MirRuntimeReference, MirSourceAnchor,
        MirTerminatorKind, MirUnitBuilder, MirUnitKind,
    };
    use bray_runtime_interface::{BinarySymbolName, RuntimeAbiRole, RuntimeAbiVersion};
    use bray_testing::{test_bound_unit, test_mir_target, test_mir_type};

    use super::{
        CodegenMappings, CodegenMappingsBuildError, demanded_debug_sources, demanded_types,
    };
    use crate::test_support::codegen_request;
    use crate::{
        CodegenCallableSignature, CodegenLinkage, CodegenResultMapping, CodegenSymbolKey,
        CodegenSymbolMapping, CodegenTypeMapping, CodegenUnit,
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
            mappings.debug_locations().iter().cloned(),
        );

        let second = CodegenMappings::try_new(
            request.unit(),
            request.target(),
            mappings.types().iter().rev().cloned(),
            mappings.symbols().iter().rev().cloned(),
            mappings.debug_locations().iter().rev().cloned(),
        );

        assert_eq!(first, second);
        assert!(mappings.covers_debug_sources(request.unit()));
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
                mappings.debug_locations().iter().cloned(),
            ),
            Err(CodegenMappingsBuildError::InstanceSymbolCoverageMismatch)
        );

        let Ok(without_debug) = CodegenMappings::try_new(
            request.unit(),
            request.target(),
            mappings.types().iter().cloned(),
            mappings.symbols().iter().cloned(),
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
