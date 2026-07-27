use std::num::{NonZeroU16, NonZeroU32};

use bray_base::Cancellation;
use bray_runtime_interface::PanicAbiIdentity;
use bray_symbols::CallableAbi;
use bray_target::test_support::test_target_profile;
use bray_target::{CodeModel, RelocationModel};
use bray_testing::test_mir_unit;

use crate::{
    ArtifactContent, BackendArtifactContribution, BackendArtifactId, BackendArtifactKind,
    BackendArtifactRequest, BackendArtifactRequestEntry, BackendArtifactRequirement,
    BackendIdentity, BackendSerializationOptions, CallableAbiMapping, CodegenLinkage,
    CodegenOptions, CodegenRequest, CodegenTarget, CodegenUnit, CodegenUnitKey,
    DebugInformationMode, DebugInformationOutputMode, LinkableArtifactKind,
    LinkableArtifactRequirement, OptimizationLevel, SizePreference, TargetAbi, TargetAddressSpace,
    TargetAddressSpaceKind, TargetCallingConvention, TargetCompatibility, TargetContract,
    TargetDataLayout, TargetMachineSelection, TargetScalarKind, TargetScalarLayout,
    TargetSymbolConvention,
};

pub(crate) struct CodegenRequestFixture {
    unit: CodegenUnit,
    backend: BackendIdentity,
    target: CodegenTarget,
    options: CodegenOptions,
    artifacts: BackendArtifactRequest,
    required_artifact: BackendArtifactId,
    optional_artifact: BackendArtifactId,
    cancellation: NeverCancelled,
}

impl CodegenRequestFixture {
    pub(crate) fn request(&self) -> CodegenRequest<'_> {
        let Ok(request) = CodegenRequest::try_new(
            &self.unit,
            &self.backend,
            &self.target,
            &self.options,
            &self.artifacts,
            &self.cancellation,
        ) else {
            panic!("test codegen request must be valid");
        };

        request
    }

    pub(crate) const fn required_artifact(&self) -> &BackendArtifactId {
        &self.required_artifact
    }

    pub(crate) const fn optional_artifact(&self) -> &BackendArtifactId {
        &self.optional_artifact
    }
}

pub(crate) fn codegen_request() -> CodegenRequestFixture {
    let unit = codegen_unit(1);

    let required_artifact = BackendArtifactId::new(
        unit.key().clone(),
        BackendArtifactKind::RelocatableObject,
        0,
    );

    let optional_artifact = BackendArtifactId::new(
        unit.key().clone(),
        BackendArtifactKind::RelocatableObject,
        1,
    );

    let entries = [
        BackendArtifactRequestEntry::new(
            required_artifact.clone(),
            BackendArtifactRequirement::Required,
        ),
        BackendArtifactRequestEntry::new(
            optional_artifact.clone(),
            BackendArtifactRequirement::Optional,
        ),
    ];

    let serialization =
        BackendSerializationOptions::new(crate::AssemblySyntaxKind::TargetDefault, false);

    let Ok(artifacts) = BackendArtifactRequest::try_new(
        unit.key().clone(),
        entries,
        DebugInformationOutputMode::Omit,
        Some(LinkableArtifactRequirement::new(
            LinkableArtifactKind::RelocatableObject,
            BackendArtifactRequirement::Required,
        )),
        serialization,
    ) else {
        panic!("test artifact request must be valid");
    };

    CodegenRequestFixture {
        unit,
        backend: backend_identity(),
        target: codegen_target(),
        options: CodegenOptions::new(
            OptimizationLevel::None,
            SizePreference::None,
            DebugInformationMode::None,
        ),
        artifacts,
        required_artifact,
        optional_artifact,
        cancellation: NeverCancelled,
    }
}

struct NeverCancelled;

impl Cancellation for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

pub(crate) fn codegen_unit_key(seed: u8) -> CodegenUnitKey {
    codegen_unit(seed).key().clone()
}

pub(crate) fn artifact_content() -> ArtifactContent {
    let Ok(content) = ArtifactContent::try_memory([1_u8, 2, 3].as_slice()) else {
        panic!("test artifact content must be valid");
    };

    content
}

pub(crate) fn contribution(
    fixture: &CodegenRequestFixture,
    id: BackendArtifactId,
) -> BackendArtifactContribution {
    BackendArtifactContribution::new(
        id,
        artifact_content(),
        fixture.request().backend().clone(),
        fixture.request().target().identity().clone(),
        None,
    )
}

fn codegen_unit(seed: u8) -> CodegenUnit {
    let Ok(unit) = CodegenUnit::try_new(1, [test_mir_unit(u32::from(seed))]) else {
        panic!("test codegen unit must be valid");
    };

    unit
}

fn backend_identity() -> BackendIdentity {
    let Some(identity) = BackendIdentity::try_new("llvm", "bray-1", "llvm-22") else {
        panic!("test backend identity must be valid");
    };

    identity
}

pub(crate) fn codegen_target() -> CodegenTarget {
    let contract = target_contract();

    let Ok(selection) = TargetMachineSelection::try_new(
        RelocationModel::PositionIndependent,
        CodeModel::Small,
        "x86-64-v3",
        ["avx", "sse4.2"],
    ) else {
        panic!("test target machine selection must be valid");
    };

    let Ok(target) = CodegenTarget::try_new(
        test_target_profile(),
        "x86_64-unknown-linux-gnu",
        contract,
        selection,
    ) else {
        panic!("test target must be valid");
    };

    target
}

fn target_contract() -> TargetContract {
    let Some(panic_abi) = PanicAbiIdentity::try_new("bray.panic.test") else {
        panic!("test panic ABI identity must be valid");
    };

    TargetContract::new(
        target_data_layout(),
        target_abi(),
        panic_abi,
        target_symbols(),
        target_compatibility(),
    )
}

fn target_data_layout() -> TargetDataLayout {
    let byte = NonZeroU16::new(1).unwrap_or(NonZeroU16::MIN);
    let four_bytes = NonZeroU16::new(4).unwrap_or(NonZeroU16::MIN);
    let aggregate_alignment = NonZeroU32::new(8).unwrap_or(NonZeroU32::MIN);
    let integer_width = NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN);

    let scalar_inputs = [
        (TargetScalarKind::Boolean, byte, byte),
        (
            TargetScalarKind::Integer(integer_width),
            four_bytes,
            four_bytes,
        ),
    ];

    let scalars = scalar_inputs.into_iter().map(|(kind, size, alignment)| {
        let Ok(layout) = TargetScalarLayout::try_new(kind, size, alignment) else {
            panic!("test scalar layout must be valid");
        };

        layout
    });

    let address_spaces = [
        TargetAddressSpace::new(TargetAddressSpaceKind::Default, 0),
        TargetAddressSpace::new(TargetAddressSpaceKind::Function, 0),
    ];

    let Ok(layout) = TargetDataLayout::try_new(scalars, aggregate_alignment, address_spaces) else {
        panic!("test target data layout must be valid");
    };

    layout
}

fn target_abi() -> TargetAbi {
    let conventions = [
        (CallableAbi::Bray, "bray-x86_64"),
        (CallableAbi::C, "sysv64"),
        (CallableAbi::System, "sysv64"),
    ];

    let mappings = conventions.into_iter().map(|(abi, name)| {
        let Some(convention) = TargetCallingConvention::try_new(name) else {
            panic!("test calling convention must be valid");
        };

        CallableAbiMapping::new(abi, convention)
    });

    let Ok(abi) = TargetAbi::try_new(mappings) else {
        panic!("test target ABI must be valid");
    };

    abi
}

fn target_symbols() -> TargetSymbolConvention {
    let Ok(symbols) = TargetSymbolConvention::try_new(
        "",
        ".Lbray.",
        [
            CodegenLinkage::Private,
            CodegenLinkage::Internal,
            CodegenLinkage::External,
            CodegenLinkage::Weak,
        ],
    ) else {
        panic!("test target symbol convention must be valid");
    };

    symbols
}

fn target_compatibility() -> TargetCompatibility {
    let Some(compatibility) = TargetCompatibility::try_new("bray-target-v1", 1, ["elf"]) else {
        panic!("test target compatibility must be valid");
    };

    compatibility
}
