use bray_codegen::CodegenTarget;
use bray_ir::{MirHelperReference, MirOperationKind, MirUnit, MirUnitId};

use crate::{CancellationToken, Compilation};

pub(super) fn generated_lifecycle(
    compilation: &Compilation,
    target: &CodegenTarget,
    reference: MirHelperReference,
    unit: u32,
) -> MirUnit {
    let instance = compilation
        .concrete_codegen_lifecycle(reference, target)
        .expect("lifecycle instance must realize");

    let reference = instance
        .generated_lifecycle_reference()
        .expect("generated lifecycle payload must be retained");

    compilation
        .codegen_generated_lifecycle_mir(
            instance.key(),
            reference,
            MirUnitId::new(unit),
            &CancellationToken::new(),
        )
        .expect("generated lifecycle MIR must realize")
}

pub(super) fn reachable_cleanup_operations(
    mir: &MirUnit,
    entry: bray_ir::MirBlockId,
) -> Vec<&MirOperationKind> {
    let mut pending = vec![entry];
    let mut visited = std::collections::BTreeSet::new();
    let mut operations = std::collections::BTreeMap::new();

    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }

        let block = mir.block(id).expect("cleanup block must exist");

        for id in block.operations() {
            let operation = mir
                .operation(*id)
                .expect("cleanup operation must exist")
                .kind();

            if matches!(
                operation,
                MirOperationKind::Cleanup { .. }
                    | MirOperationKind::Finalize(_)
                    | MirOperationKind::Destroy(_)
            ) {
                operations.insert(*id, operation);
            }
        }

        block
            .terminator()
            .kind()
            .for_each_successor(|target| pending.push(target));
    }

    operations.into_values().collect()
}

pub(super) fn standard_memory_dependency() -> crate::DependencyInterfaceInput {
    use crate::test_support::source_input;

    use crate::{
        Compilation, CompilationRequest, DependencyInterfaceInput, PackageInterfaceExportRequest,
    };

    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind,
        InterfaceValidationLimits, InterfaceValidationPolicy, PackageImplementationArtifact,
        PackageInterfaceIdentity, ValidatedPackageInterface, encode_package_interface,
    };

    use std::sync::Arc;

    let package =
        bray_symbols::PackageIdentity::try_new("std").expect("standard package identity is valid");

    let product =
        InterfaceProductIdentity::try_new("library").expect("library product identity is valid");

    let identity = PackageInterfaceIdentity::try_new(
        package.clone(),
        crate::test_support::package_version(),
        product.clone(),
        InterfaceProductKind::Library,
        "public",
    )
    .expect("standard interface identity is valid");

    let provider = Compilation::load(
        CompilationRequest::with_options(
            package.clone(),
            vec![
                source_input(
                    include_str!("../../../../../../../../standard-library/std/src/std.bray"),
                    0,
                ),
                source_input(
                    include_str!("../../../../../../../../standard-library/std/src/memory.bray"),
                    1,
                ),
            ],
            crate::CompilationOptions::new(
                crate::WorkerBudget::serial(),
                bray_symbols::ProductKind::Library,
                crate::SelectedTarget::baseline(),
            ),
        )
        .with_standard_library_source_authority()
        .with_package_interface_export(PackageInterfaceExportRequest::new(
            identity,
            InterfaceLanguageRevision::new(0),
        )),
    )
    .expect("actual standard memory provider must load");

    let bundle = provider
        .package_interface_export_bundle()
        .as_ref()
        .expect("provider export must resolve")
        .as_ref()
        .expect("provider export must succeed");

    let encoded = encode_package_interface(bundle).expect("standard interface must encode");
    let policy = InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0));

    let validated = ValidatedPackageInterface::try_new(encoded.bytes(), policy)
        .expect("standard interface must validate");

    let implementation = PackageImplementationArtifact::try_new(
        &validated,
        bundle.surface(),
        bundle.semantics(),
        bundle.implementation_configuration().clone(),
        [],
        bundle.executable_templates().iter().cloned(),
        [],
        [],
        InterfaceValidationLimits::default(),
    )
    .expect("standard portable implementation must retain exported templates");

    DependencyInterfaceInput::new(
        package,
        product,
        "std.brayi",
        encoded.shared_bytes(),
        policy,
    )
    .with_implementation_artifact("std.brayimpl", Arc::new(implementation))
}
