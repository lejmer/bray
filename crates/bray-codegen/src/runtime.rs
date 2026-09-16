use std::sync::Arc;

use bray_ir::MirUnitKind;
use bray_runtime_interface::{
    ExecutableHostContract, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
    ProtectedFrameOperation, ProtectedFrameOperations,
};

use crate::{CodegenMappings, CodegenSymbolKey, CodegenUnit};

/// Immutable target-specific metadata for one protected async frame.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProtectedAsyncFrameMetadata {
    frame: ProtectedAsyncFrameId,
    frame_abi: ProtectedFrameAbiVersions,
    operations: ProtectedFrameOperations,
}

impl ProtectedAsyncFrameMetadata {
    /// Creates target-specific descriptor metadata for one protected frame.
    pub const fn new(
        frame: ProtectedAsyncFrameId,
        frame_abi: ProtectedFrameAbiVersions,
        operations: ProtectedFrameOperations,
    ) -> Self {
        Self {
            frame,
            frame_abi,
            operations,
        }
    }

    /// Returns the stable protected frame identity.
    pub const fn frame(&self) -> ProtectedAsyncFrameId {
        self.frame
    }

    /// Returns the generated protected-frame operation ABI versions.
    pub const fn frame_abi(&self) -> ProtectedFrameAbiVersions {
        self.frame_abi
    }

    /// Returns the binary symbol names of the generated descriptor operations.
    pub const fn operations(&self) -> &ProtectedFrameOperations {
        &self.operations
    }
}

/// Complete runtime metadata generated for one codegen unit.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct CodegenRuntimeMetadata {
    frames: Arc<[ProtectedAsyncFrameMetadata]>,
    executable_host: Option<ExecutableHostContract>,
}

impl CodegenRuntimeMetadata {
    /// Validates generated frame descriptors and executable-host metadata against MIR membership.
    pub fn try_new(
        unit: &CodegenUnit,
        frames: impl IntoIterator<Item = ProtectedAsyncFrameMetadata>,
        executable_host: Option<ExecutableHostContract>,
    ) -> Result<Self, CodegenRuntimeMetadataBuildError> {
        let expected_frames = expected_frames(unit);
        let expected_host = expected_host(unit)?;
        let mut frames: Vec<_> = frames.into_iter().collect();

        frames.sort_unstable_by_key(ProtectedAsyncFrameMetadata::frame);

        if let Some(pair) = frames
            .windows(2)
            .find(|pair| pair[0].frame() == pair[1].frame())
        {
            return Err(CodegenRuntimeMetadataBuildError::DuplicateFrame(
                pair[0].frame(),
            ));
        }

        let actual_frames: Vec<_> = frames
            .iter()
            .map(ProtectedAsyncFrameMetadata::frame)
            .collect();

        if actual_frames != expected_frames {
            return Err(CodegenRuntimeMetadataBuildError::FrameCoverageMismatch);
        }

        for metadata in &frames {
            if !frame_metadata_matches(unit, metadata) {
                return Err(CodegenRuntimeMetadataBuildError::FrameAbiMismatch(
                    metadata.frame(),
                ));
            }
        }

        if executable_host.as_ref() != expected_host {
            return Err(CodegenRuntimeMetadataBuildError::ExecutableHostMismatch);
        }

        Ok(Self {
            frames: frames.into(),
            executable_host,
        })
    }

    /// Returns protected frame descriptors in stable frame-identity order.
    pub fn frames(&self) -> &[ProtectedAsyncFrameMetadata] {
        &self.frames
    }

    /// Returns generated executable-host metadata when this unit owns the host stub.
    pub const fn executable_host(&self) -> Option<&ExecutableHostContract> {
        self.executable_host.as_ref()
    }

    pub(crate) fn matches_unit(&self, unit: &CodegenUnit, mappings: &CodegenMappings) -> bool {
        let frames: Vec<_> = self
            .frames
            .iter()
            .map(ProtectedAsyncFrameMetadata::frame)
            .collect();

        let Ok(host) = expected_host(unit) else {
            return false;
        };

        frames == expected_frames(unit)
            && self.frames.iter().all(|metadata| {
                frame_metadata_matches(unit, metadata) && frame_symbols_match(metadata, mappings)
            })
            && self.executable_host.as_ref() == host
    }
}

/// A contract violation that prevents publication of codegen runtime metadata.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CodegenRuntimeMetadataBuildError {
    /// More than one MIR unit claims the executable-host role.
    MultipleExecutableHosts,
    /// One protected frame descriptor appears more than once.
    DuplicateFrame(ProtectedAsyncFrameId),
    /// Generated frame descriptors do not exactly cover protected-frame MIR units.
    FrameCoverageMismatch,
    /// Generated operation entry points use another protected-frame ABI contract.
    FrameAbiMismatch(ProtectedAsyncFrameId),
    /// Generated host metadata does not exactly match the executable-host MIR unit.
    ExecutableHostMismatch,
}

fn expected_frames(unit: &CodegenUnit) -> Vec<ProtectedAsyncFrameId> {
    let mut frames: Vec<_> = unit
        .instances()
        .iter()
        .filter_map(crate::CodegenInstance::protected_frame_identity)
        .collect();

    frames.sort_unstable();

    frames
}

fn frame_metadata_matches(unit: &CodegenUnit, metadata: &ProtectedAsyncFrameMetadata) -> bool {
    unit.instances().iter().any(|instance| {
        instance
            .protected_frame_identity()
            .filter(|frame| *frame == metadata.frame())
            .and_then(|_| instance.mir().frame_descriptor())
            .is_some_and(|descriptor| descriptor.frame_abi() == metadata.frame_abi())
    })
}

fn frame_symbols_match(metadata: &ProtectedAsyncFrameMetadata, mappings: &CodegenMappings) -> bool {
    ProtectedFrameOperation::ALL.into_iter().all(|operation| {
        let key = CodegenSymbolKey::ProtectedFrame {
            frame: metadata.frame(),
            operation,
        };

        mappings
            .symbol(&key)
            .is_some_and(|symbol| symbol.name() == metadata.operations().symbol(operation))
    })
}

fn expected_host(
    unit: &CodegenUnit,
) -> Result<Option<&ExecutableHostContract>, CodegenRuntimeMetadataBuildError> {
    let mut hosts = unit.mir_units().filter_map(|unit| {
        let MirUnitKind::ExecutableHost(host) = unit.kind() else {
            return None;
        };

        Some(host)
    });

    let host = hosts.next();

    if hosts.next().is_some() {
        return Err(CodegenRuntimeMetadataBuildError::MultipleExecutableHosts);
    }

    Ok(host)
}

#[cfg(test)]
mod tests {
    use bray_ir::{
        MirBlockKind, MirFrameDescriptor, MirFrameState, MirFrameStateId, MirSourceAnchor,
        MirTerminatorKind, MirUnitBuilder, MirUnitKind,
    };
    use bray_runtime_interface::{
        BinarySymbolName, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
        ProtectedFrameOperation, ProtectedFrameOperations,
    };
    use bray_testing::{test_bound_unit, test_mir_target, test_mir_type, test_mir_unit};

    use super::{
        CodegenRuntimeMetadata, CodegenRuntimeMetadataBuildError, ProtectedAsyncFrameMetadata,
    };
    use crate::test_support::{codegen_partition_compatibility, codegen_request, codegen_target};
    use crate::{
        CodegenCallableSignature, CodegenLinkage, CodegenMappings, CodegenPartitionPolicy,
        CodegenResultMapping, CodegenSymbolKey, CodegenSymbolMapping, CodegenUnit,
    };

    #[test]
    fn synchronous_units_require_empty_runtime_metadata() {
        let Ok(unit) = CodegenUnit::try_new(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            codegen_partition_compatibility(),
            [test_mir_unit(4)],
        ) else {
            panic!("test codegen unit must be valid");
        };

        assert_eq!(
            CodegenRuntimeMetadata::try_new(&unit, [], None),
            Ok(CodegenRuntimeMetadata::default())
        );
    }

    #[test]
    fn protected_frames_require_exact_descriptor_metadata() {
        let template = ProtectedAsyncFrameId::new([9; 32]);
        let mir = protected_frame_mir(template);

        let Ok(unit) = CodegenUnit::try_new(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            codegen_partition_compatibility(),
            [mir],
        ) else {
            panic!("test codegen unit must be valid");
        };

        let Some(frame) = unit.instances()[0].protected_frame_identity() else {
            panic!("protected-frame instances must have concrete identities");
        };

        assert_eq!(
            CodegenRuntimeMetadata::try_new(&unit, [], None),
            Err(CodegenRuntimeMetadataBuildError::FrameCoverageMismatch)
        );

        let incompatible = ProtectedAsyncFrameMetadata::new(
            frame,
            ProtectedFrameAbiVersions::uniform(bray_runtime_interface::RuntimeAbiVersion::new(
                2, 0,
            )),
            frame_operation_names(),
        );

        assert_eq!(
            CodegenRuntimeMetadata::try_new(&unit, [incompatible], None),
            Err(CodegenRuntimeMetadataBuildError::FrameAbiMismatch(frame))
        );

        let operations = frame_operation_names();

        let descriptor = ProtectedAsyncFrameMetadata::new(
            frame,
            ProtectedFrameAbiVersions::uniform(bray_runtime_interface::RuntimeAbiVersion::new(
                1, 0,
            )),
            operations.clone(),
        );

        let Ok(metadata) = CodegenRuntimeMetadata::try_new(&unit, [descriptor], None) else {
            panic!("matching frame descriptor metadata must validate");
        };

        assert_eq!(metadata.frames()[0].frame(), frame);

        let mappings = frame_mappings(&unit, frame, &operations);

        assert!(metadata.matches_unit(&unit, &mappings));

        let conflicting = ProtectedAsyncFrameMetadata::new(
            frame,
            ProtectedFrameAbiVersions::uniform(bray_runtime_interface::RuntimeAbiVersion::new(
                1, 0,
            )),
            ProtectedFrameOperations::new(
                binary_symbol_name("different_move"),
                binary_symbol_name("frame_state"),
                binary_symbol_name("frame_resume"),
                binary_symbol_name("frame_cancel"),
                binary_symbol_name("frame_broadcast"),
                binary_symbol_name("frame_resolve"),
                binary_symbol_name("frame_move_completion"),
                binary_symbol_name("frame_destroy"),
            ),
        );

        let Ok(conflicting) = CodegenRuntimeMetadata::try_new(&unit, [conflicting], None) else {
            panic!("runtime metadata validation does not own canonical symbol mappings");
        };

        assert!(!conflicting.matches_unit(&unit, &mappings));
    }

    fn protected_frame_mir(frame: ProtectedAsyncFrameId) -> bray_ir::MirUnit {
        let bound = test_bound_unit(8);
        let source = bound.key().source();

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::ProtectedAsyncFrame(frame),
            test_mir_target(),
        );

        let source = MirSourceAnchor::from(source);

        let Ok(entry) = builder.push_block(source.clone(), MirBlockKind::Ordinary) else {
            panic!("test protected-frame block must validate");
        };

        builder.set_terminator(entry, source, MirTerminatorKind::Return(None));

        let descriptor = frame_descriptor(frame, entry);

        builder.set_frame_descriptor(descriptor);

        builder.finish(entry)
    }

    fn frame_descriptor(
        frame: ProtectedAsyncFrameId,
        entry: bray_ir::MirBlockId,
    ) -> MirFrameDescriptor {
        let state = MirFrameState::new(MirFrameStateId::new(0), entry, [], []);

        match MirFrameDescriptor::new(
            frame,
            bray_runtime_interface::RuntimeAbiVersion::new(1, 0),
            ProtectedFrameAbiVersions::uniform(bray_runtime_interface::RuntimeAbiVersion::new(
                1, 0,
            )),
            test_mir_type(),
            [state],
        ) {
            Ok(descriptor) => descriptor,
            Err(error) => panic!("test frame descriptor must be valid: {error:?}"),
        }
    }

    fn frame_operation_names() -> ProtectedFrameOperations {
        ProtectedFrameOperations::new(
            binary_symbol_name("frame_move_before_start"),
            binary_symbol_name("frame_state"),
            binary_symbol_name("frame_resume"),
            binary_symbol_name("frame_cancel"),
            binary_symbol_name("frame_broadcast"),
            binary_symbol_name("frame_resolve"),
            binary_symbol_name("frame_move_completion"),
            binary_symbol_name("frame_destroy"),
        )
    }

    fn frame_mappings(
        unit: &CodegenUnit,
        frame: ProtectedAsyncFrameId,
        operations: &ProtectedFrameOperations,
    ) -> CodegenMappings {
        let target = codegen_target();

        let signature = CodegenCallableSignature::new(
            [],
            CodegenResultMapping::Void,
            bray_symbols::CallableAbi::Bray,
            false,
        );

        let instance = CodegenSymbolMapping::new(
            CodegenSymbolKey::Instance(unit.instances()[0].key().clone()),
            binary_symbol_name("frame_entry"),
            CodegenLinkage::Internal,
            signature.clone(),
        );

        let frame_operations = ProtectedFrameOperation::ALL.into_iter().map(|operation| {
            CodegenSymbolMapping::new(
                CodegenSymbolKey::ProtectedFrame { frame, operation },
                operations.symbol(operation).clone(),
                CodegenLinkage::Internal,
                signature.clone(),
            )
        });

        let symbols = [instance].into_iter().chain(frame_operations);
        let fixture = codegen_request();
        let type_mapping = &fixture.request().mappings().types()[0];

        let Some(descriptor) = unit.instances()[0].mir().frame_descriptor() else {
            panic!("test protected-frame MIR must retain its descriptor");
        };

        let types = [crate::CodegenTypeMapping::new(
            descriptor.result_type(),
            type_mapping
                .layout()
                .unwrap_or_else(|| panic!("test runtime type must be sized")),
            type_mapping.kind().clone(),
        )];

        CodegenMappings::new(
            unit,
            &target,
            types,
            [],
            symbols,
            [],
            [],
            [],
            [],
            [],
            [],
            [],
            [],
        )
    }

    fn binary_symbol_name(name: &str) -> BinarySymbolName {
        let Some(symbol_name) = BinarySymbolName::try_new(name) else {
            panic!("test frame operation name must be valid");
        };

        symbol_name
    }
}
