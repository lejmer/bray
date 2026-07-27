use std::sync::Arc;

use bray_ir::MirUnitKind;
use bray_runtime_interface::{
    BinarySymbolName, ExecutableHostContract, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
};

use crate::CodegenUnit;

/// Binary symbol names of the operations emitted for one protected async-frame descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedAsyncFrameOperationNames {
    resume: BinarySymbolName,
    task_broadcast: BinarySymbolName,
    lifecycle_resolution: BinarySymbolName,
    completion_move: BinarySymbolName,
    destruction: BinarySymbolName,
}

impl ProtectedAsyncFrameOperationNames {
    /// Creates the independently versioned frame-descriptor operation names.
    pub const fn new(
        resume: BinarySymbolName,
        task_broadcast: BinarySymbolName,
        lifecycle_resolution: BinarySymbolName,
        completion_move: BinarySymbolName,
        destruction: BinarySymbolName,
    ) -> Self {
        Self {
            resume,
            task_broadcast,
            lifecycle_resolution,
            completion_move,
            destruction,
        }
    }

    /// Returns the binary symbol name of the frame-resume operation.
    pub const fn resume(&self) -> &BinarySymbolName {
        &self.resume
    }

    /// Returns the binary symbol name of the phase-one owned-task broadcast operation.
    pub const fn task_broadcast(&self) -> &BinarySymbolName {
        &self.task_broadcast
    }

    /// Returns the binary symbol name of the phase-two lifecycle-resolution operation.
    pub const fn lifecycle_resolution(&self) -> &BinarySymbolName {
        &self.lifecycle_resolution
    }

    /// Returns the binary symbol name of the completed-result move operation.
    pub const fn completion_move(&self) -> &BinarySymbolName {
        &self.completion_move
    }

    /// Returns the binary symbol name of the infallible terminal-destruction operation.
    pub const fn destruction(&self) -> &BinarySymbolName {
        &self.destruction
    }
}

/// Immutable target-specific metadata for one protected async frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedAsyncFrameMetadata {
    frame: ProtectedAsyncFrameId,
    frame_abi: ProtectedFrameAbiVersions,
    operation_names: ProtectedAsyncFrameOperationNames,
}

impl ProtectedAsyncFrameMetadata {
    /// Creates target-specific descriptor metadata for one protected frame.
    pub const fn new(
        frame: ProtectedAsyncFrameId,
        frame_abi: ProtectedFrameAbiVersions,
        operation_names: ProtectedAsyncFrameOperationNames,
    ) -> Self {
        Self {
            frame,
            frame_abi,
            operation_names,
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
    pub const fn operation_names(&self) -> &ProtectedAsyncFrameOperationNames {
        &self.operation_names
    }
}

/// Complete runtime metadata generated for one codegen unit.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
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

    pub(crate) fn matches_unit(&self, unit: &CodegenUnit) -> bool {
        let frames: Vec<_> = self
            .frames
            .iter()
            .map(ProtectedAsyncFrameMetadata::frame)
            .collect();

        let Ok(host) = expected_host(unit) else {
            return false;
        };

        frames == expected_frames(unit)
            && self
                .frames
                .iter()
                .all(|metadata| frame_metadata_matches(unit, metadata))
            && self.executable_host.as_ref() == host
    }
}

/// A contract violation that prevents publication of codegen runtime metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
        .mir_units()
        .iter()
        .filter_map(|unit| match unit.kind() {
            MirUnitKind::ProtectedAsyncFrame(frame) => Some(*frame),
            MirUnitKind::Synchronous | MirUnitKind::ExecutableHost(_) => None,
        })
        .collect();

    frames.sort_unstable();

    frames
}

fn frame_metadata_matches(unit: &CodegenUnit, metadata: &ProtectedAsyncFrameMetadata) -> bool {
    unit.mir_units().iter().any(|unit| {
        unit.frame_descriptor().is_some_and(|descriptor| {
            descriptor.frame() == metadata.frame()
                && descriptor.frame_abi() == metadata.frame_abi()
        })
    })
}

fn expected_host(
    unit: &CodegenUnit,
) -> Result<Option<&ExecutableHostContract>, CodegenRuntimeMetadataBuildError> {
    let mut hosts = unit.mir_units().iter().filter_map(|unit| {
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
        MirBlockKind, MirFrameDescriptor, MirFrameStateFacts, MirFrameStateId, MirSourceAnchor,
        MirTerminatorKind, MirUnitBuilder, MirUnitKind,
    };
    use bray_runtime_interface::{
        BinarySymbolName, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
    };
    use bray_testing::{test_bound_unit, test_mir_target, test_mir_type, test_mir_unit};

    use super::{
        CodegenRuntimeMetadata, CodegenRuntimeMetadataBuildError, ProtectedAsyncFrameMetadata,
        ProtectedAsyncFrameOperationNames,
    };
    use crate::CodegenUnit;

    #[test]
    fn synchronous_units_require_empty_runtime_metadata() {
        let Ok(unit) = CodegenUnit::try_new(1, [test_mir_unit(4)]) else {
            panic!("test codegen unit must be valid");
        };

        assert_eq!(
            CodegenRuntimeMetadata::try_new(&unit, [], None),
            Ok(CodegenRuntimeMetadata::default())
        );
    }

    #[test]
    fn protected_frames_require_exact_descriptor_metadata() {
        let frame = ProtectedAsyncFrameId::new([9; 32]);
        let mir = protected_frame_mir(frame);

        let Ok(unit) = CodegenUnit::try_new(1, [mir]) else {
            panic!("test codegen unit must be valid");
        };

        assert_eq!(
            CodegenRuntimeMetadata::try_new(&unit, [], None),
            Err(CodegenRuntimeMetadataBuildError::FrameCoverageMismatch)
        );

        let incompatible = ProtectedAsyncFrameMetadata::new(
            frame,
            ProtectedFrameAbiVersions::uniform(
                bray_runtime_interface::RuntimeAbiVersion::new(2, 0),
            ),
            frame_operation_names(),
        );

        assert_eq!(
            CodegenRuntimeMetadata::try_new(&unit, [incompatible], None),
            Err(CodegenRuntimeMetadataBuildError::FrameAbiMismatch(frame))
        );

        let descriptor = ProtectedAsyncFrameMetadata::new(
            frame,
            ProtectedFrameAbiVersions::uniform(
                bray_runtime_interface::RuntimeAbiVersion::new(1, 0),
            ),
            frame_operation_names(),
        );

        let Ok(metadata) = CodegenRuntimeMetadata::try_new(&unit, [descriptor], None) else {
            panic!("matching frame descriptor metadata must validate");
        };

        assert_eq!(metadata.frames()[0].frame(), frame);
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

        let Ok(()) = builder.set_terminator(entry, source, MirTerminatorKind::Return(None)) else {
            panic!("test protected-frame terminator must validate");
        };

        let descriptor = frame_descriptor(frame, entry);

        if let Err(error) = builder.set_frame_descriptor(descriptor) {
            panic!("test frame descriptor must commit: {error:?}");
        }

        let Ok(unit) = builder.finish(entry) else {
            panic!("test protected-frame MIR must validate");
        };

        unit
    }

    fn frame_descriptor(
        frame: ProtectedAsyncFrameId,
        entry: bray_ir::MirBlockId,
    ) -> MirFrameDescriptor {
        let state = MirFrameStateFacts::new(MirFrameStateId::new(0), entry, [], None, []);

        match MirFrameDescriptor::try_new(
            frame,
            bray_runtime_interface::RuntimeAbiVersion::new(1, 0),
            ProtectedFrameAbiVersions::uniform(
                bray_runtime_interface::RuntimeAbiVersion::new(1, 0),
            ),
            test_mir_type(),
            [state],
        ) {
            Ok(descriptor) => descriptor,
            Err(error) => panic!("test frame descriptor must be valid: {error:?}"),
        }
    }

    fn frame_operation_names() -> ProtectedAsyncFrameOperationNames {
        ProtectedAsyncFrameOperationNames::new(
            binary_symbol_name("frame_resume"),
            binary_symbol_name("frame_broadcast"),
            binary_symbol_name("frame_resolve"),
            binary_symbol_name("frame_move_completion"),
            binary_symbol_name("frame_destroy"),
        )
    }

    fn binary_symbol_name(name: &str) -> BinarySymbolName {
        let Some(symbol_name) = BinarySymbolName::try_new(name) else {
            panic!("test frame operation name must be valid");
        };

        symbol_name
    }
}
