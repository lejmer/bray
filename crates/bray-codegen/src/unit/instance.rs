use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_ir::{MirTargetFacts, MirUnit, MirUnitKey};
use bray_symbols::{ConcreteGenericSubstitutionId, ImplementationInstanceId};

/// Whether one generated definition is generic and, if so, its exact concrete arguments.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenSpecialization {
    /// A definition with no generic parameters.
    NonGeneric,
    /// A definition instantiated with one canonical concrete substitution.
    Generic(ConcreteGenericSubstitutionId),
}

/// Stable identity of one concrete MIR definition generated for one target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenInstanceKey {
    template: MirUnitKey,
    specialization: CodegenSpecialization,
    witnesses: Arc<[ImplementationInstanceId]>,
    target: MirTargetFacts,
}

impl CodegenInstanceKey {
    /// Creates a concrete identity from its template, specialization, witnesses, and target.
    pub fn new(
        template: MirUnitKey,
        specialization: CodegenSpecialization,
        witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        target: MirTargetFacts,
    ) -> Self {
        Self {
            template,
            specialization,
            witnesses: sorted_unique_shared_slice(witnesses),
            target,
        }
    }

    /// Creates the identity of a non-generic MIR definition without implementation witnesses.
    pub fn non_generic(unit: &MirUnit) -> Self {
        Self::new(
            unit.key().clone(),
            CodegenSpecialization::NonGeneric,
            [],
            unit.target().clone(),
        )
    }

    /// Returns the MIR template identity.
    pub const fn template(&self) -> &MirUnitKey {
        &self.template
    }

    /// Returns the exact generic specialization.
    pub const fn specialization(&self) -> CodegenSpecialization {
        self.specialization
    }

    /// Returns selected implementation witnesses in canonical order.
    pub fn witnesses(&self) -> &[ImplementationInstanceId] {
        &self.witnesses
    }

    /// Returns the target affecting this generated definition.
    pub const fn target(&self) -> &MirTargetFacts {
        &self.target
    }
}

/// Why one concrete definition makes another definition reachable.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenInstanceDependencyKind {
    /// An ordinary generated-definition reference.
    Definition,
    /// A protected child frame composed directly into its parent's await path.
    DirectAwaitedFrame,
    /// A protected child frame started as an independently scheduled task.
    StartedTask,
}

/// One typed edge to a concrete definition required by another definition.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenInstanceDependency {
    kind: CodegenInstanceDependencyKind,
    instance: CodegenInstanceKey,
}

impl CodegenInstanceDependency {
    /// Creates an ordinary generated-definition dependency.
    pub const fn definition(instance: CodegenInstanceKey) -> Self {
        Self::new(CodegenInstanceDependencyKind::Definition, instance)
    }

    /// Creates one dependency with its reachability role.
    pub const fn new(
        kind: CodegenInstanceDependencyKind,
        instance: CodegenInstanceKey,
    ) -> Self {
        Self { kind, instance }
    }

    /// Returns why the concrete definition is required.
    pub const fn kind(&self) -> CodegenInstanceDependencyKind {
        self.kind
    }

    /// Returns the required concrete definition.
    pub const fn instance(&self) -> &CodegenInstanceKey {
        &self.instance
    }
}

/// One immutable concrete definition and its exact generated-definition dependencies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodegenInstance {
    key: CodegenInstanceKey,
    mir: Arc<MirUnit>,
    dependencies: Arc<[CodegenInstanceDependency]>,
}

impl CodegenInstance {
    /// Validates one concrete definition against its MIR template and dependencies.
    pub fn try_new(
        key: CodegenInstanceKey,
        mir: impl Into<Arc<MirUnit>>,
        dependencies: impl IntoIterator<Item = CodegenInstanceDependency>,
    ) -> Result<Self, CodegenInstanceBuildError> {
        let mir = mir.into();

        if key.template() != mir.key() {
            return Err(CodegenInstanceBuildError::TemplateMismatch);
        }

        if key.target() != mir.target() {
            return Err(CodegenInstanceBuildError::TargetMismatch);
        }

        let dependencies = sorted_unique_shared_slice(dependencies);

        if dependencies
            .iter()
            .any(|dependency| dependency.instance().target() != key.target())
        {
            return Err(CodegenInstanceBuildError::DependencyTargetMismatch);
        }

        Ok(Self {
            key,
            mir,
            dependencies,
        })
    }

    /// Creates one non-generic definition with no generated-definition dependencies.
    pub fn non_generic(mir: MirUnit) -> Self {
        let key = CodegenInstanceKey::non_generic(&mir);

        Self {
            key,
            mir: Arc::new(mir),
            dependencies: Arc::from([]),
        }
    }

    /// Returns the stable concrete identity.
    pub const fn key(&self) -> &CodegenInstanceKey {
        &self.key
    }

    /// Returns the validated MIR template used by this instance.
    pub fn mir(&self) -> &MirUnit {
        &self.mir
    }

    /// Returns exact generated-definition dependencies in canonical order.
    pub fn dependencies(&self) -> &[CodegenInstanceDependency] {
        &self.dependencies
    }
}

/// A contract violation that prevents creation of one concrete code generation instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodegenInstanceBuildError {
    /// The concrete identity names another MIR template.
    TemplateMismatch,
    /// The concrete identity and MIR template target different machines.
    TargetMismatch,
    /// A dependency belongs to another target.
    DependencyTargetMismatch,
}

#[cfg(test)]
mod tests {
    use bray_ir::MirTargetFacts;
    use bray_testing::{test_mir_unit, test_mir_unit_with_declaration};
    use bray_target::{TargetIdentity, TargetProfile};

    use super::{
        CodegenInstance, CodegenInstanceBuildError, CodegenInstanceDependency,
        CodegenInstanceDependencyKind, CodegenInstanceKey, CodegenSpecialization,
    };

    #[test]
    fn instances_validate_template_and_target_identity() {
        let mir = test_mir_unit(4);
        let other = test_mir_unit_with_declaration(8, 1);

        let wrong_template = CodegenInstanceKey::new(
            other.key().clone(),
            CodegenSpecialization::NonGeneric,
            [],
            mir.target().clone(),
        );

        assert_eq!(
            CodegenInstance::try_new(wrong_template, mir.clone(), []),
            Err(CodegenInstanceBuildError::TemplateMismatch)
        );

        let key = CodegenInstanceKey::non_generic(&mir);

        let dependency = CodegenInstanceDependency::definition(
            CodegenInstanceKey::non_generic(&other),
        );

        assert!(CodegenInstance::try_new(key, mir, [dependency]).is_ok());
    }

    #[test]
    fn witnesses_and_dependencies_are_canonical_sets() {
        let mir = test_mir_unit(4);

        let dependency = CodegenInstanceDependency::definition(
            CodegenInstanceKey::non_generic(&test_mir_unit_with_declaration(8, 1)),
        );

        let key = CodegenInstanceKey::non_generic(&mir);

        let Ok(instance) =
            CodegenInstance::try_new(key, mir, [dependency.clone(), dependency.clone()])
        else {
            panic!("matching instance inputs must validate");
        };

        assert_eq!(instance.dependencies(), &[dependency]);
    }

    #[test]
    fn frame_dependencies_preserve_direct_await_and_started_task_roles() {
        let mir = test_mir_unit(4);
        let dependency = CodegenInstanceKey::non_generic(&test_mir_unit_with_declaration(8, 1));

        let direct = CodegenInstanceDependency::new(
            CodegenInstanceDependencyKind::DirectAwaitedFrame,
            dependency.clone(),
        );

        let started = CodegenInstanceDependency::new(
            CodegenInstanceDependencyKind::StartedTask,
            dependency,
        );

        let Ok(instance) = CodegenInstance::try_new(
            CodegenInstanceKey::non_generic(&mir),
            mir,
            [direct.clone(), started.clone()],
        ) else {
            panic!("matching frame dependencies must validate");
        };

        assert_eq!(instance.dependencies(), &[direct, started]);
    }

    #[test]
    fn target_facts_separate_otherwise_equal_concrete_instances() {
        let mir = test_mir_unit(4);
        let original = CodegenInstanceKey::non_generic(&mir);

        let Some(identity) = TargetIdentity::try_new("x86_64-example-other") else {
            panic!("alternate target identity must be valid");
        };

        let Ok(profile) = TargetProfile::try_new(
            identity,
            bray_target::test_support::test_target_machine(),
            bray_target::test_support::test_target_facts(),
        ) else {
            panic!("alternate target profile must be valid");
        };

        let alternate_target =
            MirTargetFacts::new(profile, mir.target().runtime_abi());

        let alternate = CodegenInstanceKey::new(
            mir.key().clone(),
            CodegenSpecialization::NonGeneric,
            [],
            alternate_target,
        );

        assert_ne!(original, alternate);

        assert_eq!(
            CodegenInstance::try_new(alternate, mir, []),
            Err(CodegenInstanceBuildError::TargetMismatch)
        );
    }
}
