use std::collections::BTreeSet;

use bray_codegen::{
    CodegenInstance, CodegenInstanceDependency, CodegenInstanceKey, CodegenSymbolKey,
};
use bray_ir::{MirCleanupPhase, MirHelperReference};
use bray_symbols::TypeData;
use bray_testing::{test_mir_unit, test_mir_unit_for_target, test_mir_unit_with_declaration};

use super::targets::codegen_target;

use crate::compilation::CodegenPreparationError;
use crate::compilation::product::realization::support::dependency_symbol;
use crate::compilation::product::specialization::ConcreteCodegenInstance;
use crate::test_support::compilation;

#[test]
fn lifecycle_helpers_use_distinct_type_aware_instance_dependencies() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let values = compilation
        .semantic_value_store()
        .expect("semantic values must resolve");

    let leaf = values
        .intern_type(TypeData::tuple([]))
        .expect("leaf type must intern");

    let aggregate = values
        .intern_type(TypeData::tuple([leaf]))
        .expect("aggregate type must intern");

    let references = [
        MirHelperReference::Finalize(aggregate),
        MirHelperReference::Destroy(aggregate),
        MirHelperReference::Cleanup {
            phase: MirCleanupPhase::TaskCancellation,
            ty: aggregate,
        },
        MirHelperReference::Cleanup {
            phase: MirCleanupPhase::LifecycleResolution,
            ty: aggregate,
        },
    ];

    let dependencies = references
        .iter()
        .cloned()
        .map(|reference| {
            compilation
                .concrete_codegen_lifecycle(reference, &target)
                .expect("lifecycle instance must realize")
        })
        .collect::<Vec<_>>();

    assert_eq!(
        dependencies
            .iter()
            .map(ConcreteCodegenInstance::key)
            .collect::<BTreeSet<_>>()
            .len(),
        references.len()
    );

    let Some(first_dependency) = dependencies.first() else {
        panic!("lifecycle helpers must produce dependencies");
    };

    let owner_mir = test_mir_unit_for_target(2, first_dependency.key().target().clone());

    let owner = CodegenInstance::try_new(
        CodegenInstanceKey::non_generic(&owner_mir),
        owner_mir,
        dependencies
            .iter()
            .map(|dependency| CodegenInstanceDependency::definition(dependency.key().clone())),
    )
    .expect("generated lifecycle dependencies must validate");

    for (reference, dependency) in references.into_iter().zip(dependencies) {
        assert_eq!(
            dependency_symbol(&owner, dependency.key(), &reference),
            Ok(CodegenSymbolKey::Instance(dependency.key().clone()))
        );
    }
}

#[test]
fn lifecycle_identity_is_stable_and_payload_remains_compilation_local() {
    let first = compilation("module app; func main() {}");
    let second = compilation("module app; func main() {}");

    let first_values = first
        .semantic_value_store()
        .expect("first semantic values must resolve");

    let first_leaf = first_values
        .intern_type(TypeData::Error)
        .expect("first leaf type must intern");

    let first_type = first_values
        .intern_type(TypeData::tuple([first_leaf]))
        .expect("first aggregate type must intern");

    let second_values = second
        .semantic_value_store()
        .expect("second semantic values must resolve");

    let _ = second_values
        .intern_type(TypeData::tuple([]))
        .expect("unrelated type must intern");

    let second_leaf = second_values
        .intern_type(TypeData::Error)
        .expect("second leaf type must intern");

    let second_type = second_values
        .intern_type(TypeData::tuple([second_leaf]))
        .expect("second aggregate type must intern");

    let first = first
        .concrete_codegen_lifecycle(
            MirHelperReference::Destroy(first_type),
            &codegen_target(&first),
        )
        .expect("first lifecycle instance must realize");

    let second = second
        .concrete_codegen_lifecycle(
            MirHelperReference::Destroy(second_type),
            &codegen_target(&second),
        )
        .expect("second lifecycle instance must realize");

    assert_eq!(first.key(), second.key());

    assert_eq!(
        first.generated_lifecycle_reference(),
        Some(&MirHelperReference::Destroy(first_type))
    );

    assert_eq!(
        second.generated_lifecycle_reference(),
        Some(&MirHelperReference::Destroy(second_type))
    );
}

#[test]
fn lifecycle_payload_must_match_the_stable_key_role() {
    let compilation = compilation("module app; func main() {}");
    let target = codegen_target(&compilation);

    let ty = compilation
        .semantic_value_store()
        .expect("semantic values must resolve")
        .intern_type(TypeData::tuple([]))
        .expect("test type must intern");

    let finalize = compilation
        .concrete_codegen_lifecycle(MirHelperReference::Finalize(ty), &target)
        .expect("finalization instance must realize");

    assert!(
        ConcreteCodegenInstance::try_generated_lifecycle(
            finalize.key().clone(),
            MirHelperReference::Destroy(ty),
        )
        .is_none()
    );
}

#[test]
fn declaration_helpers_require_the_exact_concrete_dependency() {
    let owner_mir = test_mir_unit(3);

    let dependency = CodegenInstanceKey::non_generic(&test_mir_unit_with_declaration(4, 5));

    let owner = CodegenInstance::try_new(
        CodegenInstanceKey::non_generic(&owner_mir),
        owner_mir,
        [CodegenInstanceDependency::definition(dependency.clone())],
    )
    .expect("test helper dependency must validate");

    let reference = MirHelperReference::AnonymousCallable(match dependency.template() {
        bray_ir::MirUnitKey::Bound(unit) => {
            bray_ir::MirAnonymousCallableReference::bound(unit.clone())
        }
        bray_ir::MirUnitKey::ExecutableHost(_)
        | bray_ir::MirUnitKey::GeneratedLifecycle(_)
        | bray_ir::MirUnitKey::CompilerProvidedCallable(_)
        | bray_ir::MirUnitKey::ImportedExecutable(_)
        | bray_ir::MirUnitKey::ExternalCallable(_)
        | bray_ir::MirUnitKey::ExternalRuntimeDefault(_) => {
            panic!("test dependency must be bound");
        }
    });

    assert_eq!(
        dependency_symbol(&owner, &dependency, &reference),
        Ok(CodegenSymbolKey::Instance(dependency.clone()))
    );

    let missing = CodegenInstanceKey::non_generic(&test_mir_unit_with_declaration(6, 7));

    assert_eq!(
        dependency_symbol(&owner, &missing, &reference),
        Err(CodegenPreparationError::MissingHelperInstance(reference))
    );
}
