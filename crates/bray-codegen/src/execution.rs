use std::sync::Arc;

use bray_execution::{BinarySymbolName, ExecutableHostContract, ProtectedAsyncFrameId};
use bray_ir::MirUnitExecution;

use crate::CodegenUnit;

/// Target symbols emitted for one protected async-frame descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedAsyncFrameSymbols {
    resume: BinarySymbolName,
    task_broadcast: BinarySymbolName,
    lifecycle_resolution: BinarySymbolName,
    completion_move: BinarySymbolName,
    destruction: BinarySymbolName,
}

impl ProtectedAsyncFrameSymbols {
    /// Creates the independently versioned frame-descriptor operation symbols.
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

    /// Returns the frame resume symbol.
    pub const fn resume(&self) -> &BinarySymbolName {
        &self.resume
    }

    /// Returns the phase-one owned-task broadcast symbol.
    pub const fn task_broadcast(&self) -> &BinarySymbolName {
        &self.task_broadcast
    }

    /// Returns the phase-two lifecycle-resolution symbol.
    pub const fn lifecycle_resolution(&self) -> &BinarySymbolName {
        &self.lifecycle_resolution
    }

    /// Returns the completed-result move symbol.
    pub const fn completion_move(&self) -> &BinarySymbolName {
        &self.completion_move
    }

    /// Returns the infallible terminal destruction symbol.
    pub const fn destruction(&self) -> &BinarySymbolName {
        &self.destruction
    }
}

/// Immutable target-specific metadata for one protected async frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedAsyncFrameMetadata {
    frame: ProtectedAsyncFrameId,
    symbols: ProtectedAsyncFrameSymbols,
}

impl ProtectedAsyncFrameMetadata {
    /// Creates target-specific descriptor metadata for one protected frame.
    pub const fn new(frame: ProtectedAsyncFrameId, symbols: ProtectedAsyncFrameSymbols) -> Self {
        Self { frame, symbols }
    }

    /// Returns the stable protected frame identity.
    pub const fn frame(&self) -> ProtectedAsyncFrameId {
        self.frame
    }

    /// Returns generated descriptor-operation symbols.
    pub const fn symbols(&self) -> &ProtectedAsyncFrameSymbols {
        &self.symbols
    }
}

/// Complete execution metadata generated for one codegen unit.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CodegenExecutionMetadata {
    frames: Arc<[ProtectedAsyncFrameMetadata]>,
    executable_host: Option<ExecutableHostContract>,
}

impl CodegenExecutionMetadata {
    /// Validates generated frame descriptors and executable-host metadata against MIR membership.
    pub fn try_new(
        unit: &CodegenUnit,
        frames: impl IntoIterator<Item = ProtectedAsyncFrameMetadata>,
        executable_host: Option<ExecutableHostContract>,
    ) -> Result<Self, CodegenExecutionMetadataBuildError> {
        let expected_frames = expected_frames(unit);
        let expected_host = expected_host(unit)?;
        let mut frames: Vec<_> = frames.into_iter().collect();

        frames.sort_unstable_by_key(ProtectedAsyncFrameMetadata::frame);

        if let Some(pair) = frames
            .windows(2)
            .find(|pair| pair[0].frame() == pair[1].frame())
        {
            return Err(CodegenExecutionMetadataBuildError::DuplicateFrame(
                pair[0].frame(),
            ));
        }

        let actual_frames: Vec<_> = frames
            .iter()
            .map(ProtectedAsyncFrameMetadata::frame)
            .collect();

        if actual_frames != expected_frames {
            return Err(CodegenExecutionMetadataBuildError::FrameCoverageMismatch);
        }

        if executable_host.as_ref() != expected_host {
            return Err(CodegenExecutionMetadataBuildError::ExecutableHostMismatch);
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

        frames == expected_frames(unit) && self.executable_host.as_ref() == host
    }
}

/// A contract violation that prevents publication of codegen execution metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodegenExecutionMetadataBuildError {
    /// More than one MIR unit claims the executable-host role.
    MultipleExecutableHosts,
    /// One protected frame descriptor appears more than once.
    DuplicateFrame(ProtectedAsyncFrameId),
    /// Generated frame descriptors do not exactly cover protected-frame MIR units.
    FrameCoverageMismatch,
    /// Generated host metadata does not exactly match the executable-host MIR unit.
    ExecutableHostMismatch,
}

fn expected_frames(unit: &CodegenUnit) -> Vec<ProtectedAsyncFrameId> {
    let mut frames: Vec<_> = unit
        .mir_units()
        .iter()
        .filter_map(|unit| match unit.execution() {
            MirUnitExecution::ProtectedAsyncFrame(frame) => Some(*frame),
            MirUnitExecution::Synchronous | MirUnitExecution::ExecutableHost(_) => None,
        })
        .collect();

    frames.sort_unstable();
    frames
}

fn expected_host(
    unit: &CodegenUnit,
) -> Result<Option<&ExecutableHostContract>, CodegenExecutionMetadataBuildError> {
    let mut hosts = unit.mir_units().iter().filter_map(|unit| {
        let MirUnitExecution::ExecutableHost(host) = unit.execution() else {
            return None;
        };

        Some(host)
    });

    let host = hosts.next();

    if hosts.next().is_some() {
        return Err(CodegenExecutionMetadataBuildError::MultipleExecutableHosts);
    }

    Ok(host)
}

#[cfg(test)]
mod tests {
    use bray_execution::{BinarySymbolName, ProtectedAsyncFrameId};
    use bray_ir::{MirSourceOrigin, MirUnitBuilder, MirUnitExecution};
    use bray_testing::{test_bound_unit, test_mir_unit};

    use super::{
        CodegenExecutionMetadata, CodegenExecutionMetadataBuildError, ProtectedAsyncFrameMetadata,
        ProtectedAsyncFrameSymbols,
    };
    use crate::CodegenUnit;

    #[test]
    fn synchronous_units_require_empty_execution_metadata() {
        let Ok(unit) = CodegenUnit::try_new(1, [test_mir_unit(4)]) else {
            panic!("test codegen unit must be valid");
        };

        assert_eq!(
            CodegenExecutionMetadata::try_new(&unit, [], None),
            Ok(CodegenExecutionMetadata::default())
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
            CodegenExecutionMetadata::try_new(&unit, [], None),
            Err(CodegenExecutionMetadataBuildError::FrameCoverageMismatch)
        );

        let descriptor = ProtectedAsyncFrameMetadata::new(frame, frame_symbols());
        let Ok(metadata) = CodegenExecutionMetadata::try_new(&unit, [descriptor], None) else {
            panic!("matching frame descriptor metadata must validate");
        };

        assert_eq!(metadata.frames()[0].frame(), frame);
    }

    fn protected_frame_mir(frame: ProtectedAsyncFrameId) -> bray_ir::MirUnit {
        let bound = test_bound_unit(8);
        let source = bound.key().source();
        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitExecution::ProtectedAsyncFrame(frame),
        );

        let Ok(entry) = builder.push_block(MirSourceOrigin::Source(source)) else {
            panic!("test protected-frame block must validate");
        };

        let Ok(unit) = builder.finish(entry) else {
            panic!("test protected-frame MIR must validate");
        };

        unit
    }

    fn frame_symbols() -> ProtectedAsyncFrameSymbols {
        ProtectedAsyncFrameSymbols::new(
            symbol("frame_resume"),
            symbol("frame_broadcast"),
            symbol("frame_resolve"),
            symbol("frame_move_completion"),
            symbol("frame_destroy"),
        )
    }

    fn symbol(name: &str) -> BinarySymbolName {
        let Some(symbol) = BinarySymbolName::try_new(name) else {
            panic!("test frame symbol must be valid");
        };

        symbol
    }
}
