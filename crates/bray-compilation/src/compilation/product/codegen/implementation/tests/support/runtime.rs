use super::compilation::{
    codegen_compilation_for_sources_target_with_platform_services, test_product_identity,
};
use super::linker::{test_linked_product, test_linker};
use crate::SelectedTarget;

use bray_runtime_interface::{
    BinarySymbolName, PlatformServiceBinding, PlatformServiceRole, ProtectedFrameAbiVersions,
    RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifact, RuntimeArtifactComponentMetadata,
    RuntimeArtifactId, RuntimeArtifactMetadata, RuntimeArtifactPurpose, RuntimeCapability,
    RuntimeContract, RuntimeIdentity, RuntimeRoleBinding, RuntimeRoleImplementation,
};
use bray_symbols::{NativeLinkRequirement, ProductKind};

use bray_testing::TemporaryFile;
use std::sync::Arc;

pub(in super::super) fn runtime_native_plan(
    source: &str,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    Arc<crate::compilation::product::codegen::NativeProductPlan>,
) {
    runtime_native_plan_for_product(source, ProductKind::Executable)
}

pub(in super::super) fn runtime_native_plan_for_product(
    source: &str,
    product_kind: ProductKind,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    Arc<crate::compilation::product::codegen::NativeProductPlan>,
) {
    runtime_native_plan_for_target(source, product_kind, SelectedTarget::baseline())
}

pub(in super::super) fn runtime_native_plan_for_target(
    source: &str,
    product_kind: ProductKind,
    target: SelectedTarget,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    Arc<crate::compilation::product::codegen::NativeProductPlan>,
) {
    runtime_native_plan_for_sources_target(&[source], product_kind, target, &[])
}

pub(in super::super) fn runtime_native_plan_for_sources_target(
    sources: &[&str],
    product_kind: ProductKind,
    target: SelectedTarget,
    native_link_inputs: &[NativeLinkRequirement],
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    Arc<crate::compilation::product::codegen::NativeProductPlan>,
) {
    runtime_native_plan_for_sources_target_with_platform_services(
        sources,
        product_kind,
        target,
        native_link_inputs,
        [],
    )
}

pub(in super::super) fn runtime_native_plan_for_sources_target_with_platform_services(
    sources: &[&str],
    product_kind: ProductKind,
    target: SelectedTarget,
    native_link_inputs: &[NativeLinkRequirement],
    platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    Arc<crate::compilation::product::codegen::NativeProductPlan>,
) {
    runtime_native_plan_for_sources_target_with_platform_overrides(
        sources,
        product_kind,
        target,
        native_link_inputs,
        platform_services,
        [],
        crate::BuildConfiguration::Development,
    )
}

pub(in super::super) fn runtime_native_plan_for_sources_target_with_platform_overrides(
    sources: &[&str],
    product_kind: ProductKind,
    target: SelectedTarget,
    native_link_inputs: &[NativeLinkRequirement],
    platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
    runtime_platform_services: impl IntoIterator<Item = PlatformServiceRole>,
    configuration: crate::BuildConfiguration,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    Arc<crate::compilation::product::codegen::NativeProductPlan>,
) {
    let (backend, compilation) = codegen_compilation_for_sources_target_with_platform_services(
        sources,
        product_kind,
        target,
        native_link_inputs,
        platform_services,
    );

    let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");

    let runtime = runtime_artifact_with_roles_and_platform_services(
        &compilation,
        archive.path(),
        RuntimeAbiRole::ALL,
        runtime_platform_services,
    );

    let product = test_product_identity();

    let plan = compilation
        .native_product_plan(
            product,
            configuration,
            Some(runtime),
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MainThreadLane,
            ],
            Some((&test_linker(), test_linked_product(product_kind))),
        )
        .unwrap_or_else(|error| panic!("runtime native plan must resolve: {error:?}"));

    (backend, plan)
}

pub(in super::super) fn runtime_artifact(
    compilation: &crate::Compilation,
    archive: &std::path::Path,
) -> RuntimeArtifact {
    runtime_artifact_with_roles(compilation, archive, RuntimeAbiRole::ALL)
}

pub(in super::super) fn runtime_artifact_with_roles(
    compilation: &crate::Compilation,
    archive: &std::path::Path,
    roles: impl IntoIterator<Item = RuntimeAbiRole>,
) -> RuntimeArtifact {
    runtime_artifact_with_roles_and_platform_services(compilation, archive, roles, [])
}

fn runtime_artifact_with_roles_and_platform_services(
    compilation: &crate::Compilation,
    archive: &std::path::Path,
    roles: impl IntoIterator<Item = RuntimeAbiRole>,
    platform_services: impl IntoIterator<Item = PlatformServiceRole>,
) -> RuntimeArtifact {
    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap_or_else(|error| panic!("test target must validate: {error:?}"));

    let identity = RuntimeIdentity::try_new("bray.runtime.test")
        .unwrap_or_else(|| panic!("test runtime identity must be valid"));

    let artifact = RuntimeArtifactId::try_new("bray.runtime.test.x86_64")
        .unwrap_or_else(|| panic!("test runtime artifact identity must be valid"));

    let roles: Vec<_> = roles
        .into_iter()
        .filter(|role| role.native_symbol().is_some())
        .collect();

    let bindings = roles.iter().copied().map(|role| {
        let symbol = BinarySymbolName::try_new(
            role.native_symbol()
                .expect("selected native role has a symbol"),
        )
        .unwrap_or_else(|| panic!("test runtime role symbol must be valid"));

        RuntimeRoleBinding::new(role, symbol, RuntimeRoleImplementation::BrayRuntime)
    });

    let version = RuntimeAbiVersion::new(1, 0);

    let capabilities = [
        RuntimeCapability::CooperativeExecution,
        RuntimeCapability::LocalLanes,
        RuntimeCapability::MainThreadLane,
        RuntimeCapability::PerformanceObservation,
    ];

    let contract = RuntimeContract::try_new(
        identity,
        artifact,
        version,
        ProtectedFrameAbiVersions::uniform(version),
        target.identity().clone(),
        target.panic_abi().clone(),
        capabilities,
        bindings,
    )
    .unwrap_or_else(|error| panic!("test runtime contract must validate: {error:?}"));

    let product_component = RuntimeArtifactId::try_new("runtime.product")
        .unwrap_or_else(|| panic!("test component identity must be valid"));

    let components = [
        RuntimeArtifactComponentMetadata::try_new(
            product_component.clone(),
            RuntimeArtifactPurpose::Product,
            roles
                .iter()
                .copied()
                .filter(|role| role.available_to_product()),
            capabilities,
        )
        .unwrap_or_else(|error| panic!("test component must validate: {error:?}")),
        RuntimeArtifactComponentMetadata::try_new(
            RuntimeArtifactId::try_new("runtime.test")
                .unwrap_or_else(|| panic!("test component identity must be valid")),
            RuntimeArtifactPurpose::TestRunner,
            roles.iter().copied(),
            capabilities,
        )
        .unwrap_or_else(|error| panic!("test component must validate: {error:?}"))
        .with_platform_services(platform_services),
    ];

    let directory = archive.parent().unwrap_or_else(|| std::path::Path::new(""));

    let indexes = RuntimeArtifactPurpose::ALL.map(|purpose| {
        let symbols = contract
            .role_bindings()
            .iter()
            .filter(|binding| {
                purpose == RuntimeArtifactPurpose::TestRunner
                    || binding.role().available_to_product()
            })
            .map(|binding| binding.symbol_name().as_str());

        bray_testing::test_runtime_native_index(
            directory,
            bray_target::NativeTarget::for_identity(target.identity())
                .expect("test target must be native"),
            purpose,
            symbols,
            archive,
        )
    });

    let metadata = RuntimeArtifactMetadata::try_new(
        contract,
        components,
        indexes.iter().map(|(reference, _)| reference.clone()),
    )
    .unwrap_or_else(|error| panic!("test runtime metadata must validate: {error:?}"));

    RuntimeArtifact::try_new(
        metadata,
        directory.to_path_buf(),
        indexes.map(|(_, index)| index),
    )
    .unwrap_or_else(|error| panic!("test runtime artifact must validate: {error:?}"))
}
