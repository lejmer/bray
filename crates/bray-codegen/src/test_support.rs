use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};

use bray_base::Cancellation;
use bray_ir::MirTargetFacts;
use bray_runtime_interface::{BinarySymbolName, PanicAbiIdentity, RuntimeAbiVersion};
use bray_symbols::CallableAbi;
use bray_target::test_support::test_target_profile;
use bray_target::{
    CodeModel, NativeTarget, RelocationModel, TargetLayoutContract, TargetProfile,
    TargetValueLayout,
};
use bray_testing::{test_mir_type, test_mir_unit_for_target};

use crate::mapping::{demanded_debug_sources, demanded_types};
use crate::{
    ArtifactContent, BackendArtifactContribution, BackendArtifactId, BackendArtifactKind,
    BackendArtifactRequest, BackendArtifactRequestEntry, BackendArtifactRequirement,
    BackendIdentity, BackendSerializationOptions, CallableAbiMapping, CodegenCallableSignature,
    CodegenDebugLocation, CodegenLinkage, CodegenMappings, CodegenOptions, CodegenRequest,
    CodegenResultMapping, CodegenSourceFile, CodegenSymbolKey, CodegenSymbolMapping, CodegenTarget,
    CodegenTypeKind, CodegenTypeMapping, CodegenUnit, CodegenUnitKey, DebugInformationMode,
    DebugInformationOutputMode, LinkableArtifactKind, LinkableArtifactRequirement,
    OptimizationLevel, SizePreference, TargetAbi, TargetAddressSpace, TargetAddressSpaceKind,
    TargetCallingConvention, TargetCompatibility, TargetContract, TargetDataLayout,
    TargetMachineSelection, TargetScalarKind, TargetScalarLayout, TargetSymbolConvention,
};

/// Complete validated code generation request fixture.
pub struct CodegenRequestFixture {
    unit: CodegenUnit,
    backend: BackendIdentity,
    target: CodegenTarget,
    mappings: CodegenMappings,
    options: CodegenOptions,
    artifacts: BackendArtifactRequest,
    required_artifact: BackendArtifactId,
    optional_artifact: BackendArtifactId,
    cancellation: NeverCancelled,
}

impl CodegenRequestFixture {
    /// Returns a request borrowing this fixture's immutable inputs.
    pub fn request(&self) -> CodegenRequest<'_> {
        self.request_with_cancellation(&self.cancellation)
    }

    /// Returns a request using a caller-provided cancellation observer.
    pub fn request_with_cancellation<'request>(
        &'request self,
        cancellation: &'request dyn Cancellation,
    ) -> CodegenRequest<'request> {
        let Ok(request) = CodegenRequest::try_new(
            &self.unit,
            &self.backend,
            &self.target,
            &self.mappings,
            &self.options,
            &self.artifacts,
            cancellation,
        ) else {
            panic!("test codegen request must be valid");
        };

        request
    }

    /// Returns the fixture's required artifact identity.
    pub const fn required_artifact(&self) -> &BackendArtifactId {
        &self.required_artifact
    }

    /// Returns the fixture's optional artifact identity.
    pub const fn optional_artifact(&self) -> &BackendArtifactId {
        &self.optional_artifact
    }
}

/// Creates a complete validated code generation request fixture.
pub fn codegen_request() -> CodegenRequestFixture {
    codegen_request_for_backend(backend_identity())
}

/// Creates a complete request fixture for the supplied backend identity.
pub fn codegen_request_for_backend(backend: BackendIdentity) -> CodegenRequestFixture {
    codegen_request_for_seed_and_backend(1, backend)
}

/// Creates a complete request fixture for one unit seed and backend identity.
pub fn codegen_request_for_seed_and_backend(
    seed: u8,
    backend: BackendIdentity,
) -> CodegenRequestFixture {
    let target = codegen_target();

    codegen_request_for_seed_target_and_backend(seed, target, backend)
}

/// Creates a complete request fixture for one native target and backend.
pub fn codegen_request_for_target_and_backend(
    target: NativeTarget,
    backend: BackendIdentity,
) -> CodegenRequestFixture {
    codegen_request_for_seed_target_and_backend(1, CodegenTarget::for_native(target), backend)
}

fn codegen_request_for_seed_target_and_backend(
    seed: u8,
    target: CodegenTarget,
    backend: BackendIdentity,
) -> CodegenRequestFixture {
    let unit = codegen_unit(seed, &target);
    let mappings = codegen_mappings(&unit, &target);

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

    let serialization = BackendSerializationOptions::new(crate::AssemblySyntaxKind::TargetDefault);

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
        backend,
        target,
        mappings,
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

fn codegen_mappings(unit: &CodegenUnit, target: &CodegenTarget) -> CodegenMappings {
    let mut types = demanded_types(unit);

    let alignment = NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN);
    let width = NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN);

    let has_mir_types = !types.is_empty();
    let result = types.first().copied().unwrap_or_else(test_mir_type);

    let signature_result = if has_mir_types {
        CodegenResultMapping::direct(result, None, [])
    } else {
        CodegenResultMapping::Void
    };

    types.insert(result);

    let types = types.into_iter().map(|ty| {
        if has_mir_types {
            CodegenTypeMapping::new(
                ty,
                TargetValueLayout::new(4, alignment, TargetLayoutContract::Default),
                CodegenTypeKind::SignedInteger(width),
            )
        } else {
            CodegenTypeMapping::new(
                ty,
                TargetValueLayout::new(0, NonZeroU64::MIN, TargetLayoutContract::Default),
                CodegenTypeKind::Unit,
            )
        }
    });

    let symbols = unit
        .instances()
        .iter()
        .map(|instance| {
            (
                CodegenSymbolKey::Instance(instance.key().clone()),
                CodegenLinkage::Internal,
            )
        })
        .chain(unit.external_instances().iter().map(|instance| {
            (
                CodegenSymbolKey::Instance(instance.clone()),
                CodegenLinkage::Import,
            )
        }))
        .enumerate()
        .map(|(ordinal, (key, linkage))| {
            instance_symbol(key, linkage, ordinal, signature_result.clone())
        });

    let Some(file) = CodegenSourceFile::try_new("test.bray") else {
        panic!("test source file must be valid");
    };

    let one = NonZeroU32::MIN;

    let debug_locations = demanded_debug_sources(unit)
        .into_iter()
        .map(|anchor| CodegenDebugLocation::new(anchor, file.clone(), one, one));

    match CodegenMappings::try_new(
        unit,
        target,
        types,
        symbols,
        [],
        [],
        [],
        [],
        [],
        debug_locations,
    ) {
        Ok(mappings) => mappings,
        Err(error) => panic!("test code generation mappings must be valid: {error:?}"),
    }
}

fn instance_symbol(
    key: CodegenSymbolKey,
    linkage: CodegenLinkage,
    ordinal: usize,
    result: CodegenResultMapping,
) -> CodegenSymbolMapping {
    let Some(name) = BinarySymbolName::try_new(format!("bray_test_{ordinal}")) else {
        panic!("test binary symbol name must be valid");
    };

    CodegenSymbolMapping::new(
        key,
        name,
        linkage,
        CodegenCallableSignature::new([], result, CallableAbi::Bray, false),
    )
}

struct NeverCancelled;

impl Cancellation for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// Creates a deterministic test code generation unit identity.
pub fn codegen_unit_key(seed: u8) -> CodegenUnitKey {
    codegen_unit(seed, &codegen_target()).key().clone()
}

/// Creates non-empty in-memory test artifact content.
pub fn artifact_content() -> ArtifactContent {
    let Ok(content) = ArtifactContent::try_memory([1_u8, 2, 3].as_slice()) else {
        panic!("test artifact content must be valid");
    };

    content
}

/// Creates a contribution owned by the fixture's backend and target.
pub fn contribution(
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

fn codegen_unit(seed: u8, target: &CodegenTarget) -> CodegenUnit {
    let mir_target = MirTargetFacts::new(target.profile().clone(), RuntimeAbiVersion::new(1, 0));

    let Ok(unit) = CodegenUnit::try_new(1, [test_mir_unit_for_target(u32::from(seed), mir_target)])
    else {
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

/// Creates a validated x86-64 ELF code generation target.
pub fn codegen_target() -> CodegenTarget {
    codegen_target_with_profile(test_target_profile(), "x86_64-unknown-linux-gnu")
}

/// Creates a test code generation target from the supplied profile and triple.
pub fn codegen_target_with_profile(profile: TargetProfile, triple: &str) -> CodegenTarget {
    let contract = target_contract();

    let Ok(selection) = TargetMachineSelection::try_new(
        RelocationModel::PositionIndependent,
        CodeModel::Small,
        "x86-64-v3",
        ["avx", "sse4.2"],
    ) else {
        panic!("test target machine selection must be valid");
    };

    let Ok(target) = CodegenTarget::try_new(profile, triple, contract, selection) else {
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
            CodegenLinkage::LinkOnce,
            CodegenLinkage::Common,
            CodegenLinkage::Import,
            CodegenLinkage::Export,
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
