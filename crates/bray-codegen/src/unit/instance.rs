use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bray_base::{StableDigestHasher, shared_slice, sorted_unique_shared_slice};
use bray_ir::{MirTargetContract, MirUnit, MirUnitKey, MirUnitKind};
use bray_runtime_interface::ProtectedAsyncFrameId;
use bray_symbols::SymbolKey;

const CONCRETE_FRAME_IDENTITY_REVISION: u32 = 1;

/// Stable structural digest of one concrete generic type or constant argument.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenValueKey([u8; 32]);

impl CodegenValueKey {
    /// Creates an identity from a compiler-derived structural digest.
    pub const fn new(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// Returns the structural digest bytes.
    pub const fn digest(self) -> [u8; 32] {
        self.0
    }
}

/// One concrete generic argument participating in generated-definition identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenGenericArgument {
    /// A fully resolved semantic type.
    Type(CodegenValueKey),
    /// A fully evaluated constant value.
    Constant(CodegenValueKey),
}

/// Whether one generated definition is generic and, if so, its exact concrete arguments.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenSpecialization {
    /// A definition with no generic parameters.
    NonGeneric,
    /// A definition instantiated with ordered structural arguments.
    Generic(Arc<[CodegenGenericArgument]>),
}

impl CodegenSpecialization {
    /// Creates an ordered concrete generic specialization.
    pub fn generic(arguments: impl IntoIterator<Item = CodegenGenericArgument>) -> Self {
        Self::Generic(shared_slice(arguments))
    }

    /// Returns ordered concrete arguments, or an empty slice for a non-generic definition.
    pub fn arguments(&self) -> &[CodegenGenericArgument] {
        match self {
            Self::NonGeneric => &[],
            Self::Generic(arguments) => arguments,
        }
    }
}

/// Stable structural identity of one selected implementation witness.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenImplementationWitness {
    definition: SymbolKey,
    specialization: CodegenSpecialization,
}

impl CodegenImplementationWitness {
    /// Creates a witness for an implementation definition.
    pub fn try_new(definition: SymbolKey, specialization: CodegenSpecialization) -> Option<Self> {
        if !definition.kind().is_implementation() {
            return None;
        }

        Some(Self {
            definition,
            specialization,
        })
    }

    /// Returns the selected implementation definition.
    pub const fn definition(&self) -> &SymbolKey {
        &self.definition
    }

    /// Returns the implementation's concrete generic specialization.
    pub const fn specialization(&self) -> &CodegenSpecialization {
        &self.specialization
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct CodegenInstanceData {
    template: MirUnitKey,
    specialization: CodegenSpecialization,
    witnesses: Arc<[CodegenImplementationWitness]>,
    contextual_self_witness: Option<CodegenImplementationWitness>,
    target: MirTargetContract,
}

/// Stable identity of one concrete MIR definition generated for one target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenInstanceKey(Arc<CodegenInstanceData>);

impl CodegenInstanceKey {
    /// Derives this instance's concrete frame identity from its lowered frame template.
    pub fn protected_frame_identity(
        &self,
        template: ProtectedAsyncFrameId,
    ) -> ProtectedAsyncFrameId {
        let mut hasher = StableDigestHasher::new();

        hasher.write(b"bray.concrete-protected-async-frame");
        hasher.write_u32(CONCRETE_FRAME_IDENTITY_REVISION);
        template.hash(&mut hasher);
        self.hash(&mut hasher);

        ProtectedAsyncFrameId::new(hasher.finalize())
    }
    /// Retains exact substitutions, witnesses, and target under a specialized template identity.
    pub fn with_template(&self, template: MirUnitKey) -> Self {
        Self(Arc::new(CodegenInstanceData {
            template,
            specialization: self.0.specialization.clone(),
            witnesses: Arc::clone(&self.0.witnesses),
            contextual_self_witness: self.0.contextual_self_witness.clone(),
            target: self.0.target.clone(),
        }))
    }

    /// Creates a concrete identity from its template, specialization, witnesses, and target.
    pub fn new(
        template: MirUnitKey,
        specialization: CodegenSpecialization,
        witnesses: impl IntoIterator<Item = CodegenImplementationWitness>,
        target: MirTargetContract,
    ) -> Self {
        Self(Arc::new(CodegenInstanceData {
            template,
            specialization,
            witnesses: sorted_unique_shared_slice(witnesses),
            contextual_self_witness: None,
            target,
        }))
    }

    /// Selects the implementation witness that supplies contextual `Self` for a trait default body.
    pub fn try_with_contextual_self_witness(
        self,
        witness: CodegenImplementationWitness,
    ) -> Option<Self> {
        if !self.0.witnesses.contains(&witness) {
            return None;
        }

        Some(Self(Arc::new(CodegenInstanceData {
            template: self.0.template.clone(),
            specialization: self.0.specialization.clone(),
            witnesses: Arc::clone(&self.0.witnesses),
            contextual_self_witness: Some(witness),
            target: self.0.target.clone(),
        })))
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
    pub fn template(&self) -> &MirUnitKey {
        &self.0.template
    }

    /// Returns the exact generic specialization.
    pub fn specialization(&self) -> &CodegenSpecialization {
        &self.0.specialization
    }

    /// Returns selected implementation witnesses in canonical order.
    pub fn witnesses(&self) -> &[CodegenImplementationWitness] {
        &self.0.witnesses
    }

    /// Returns the implementation witness that supplies contextual `Self`, when required.
    pub fn contextual_self_witness(&self) -> Option<&CodegenImplementationWitness> {
        self.0.contextual_self_witness.as_ref()
    }

    /// Returns the target affecting this generated definition.
    pub fn target(&self) -> &MirTargetContract {
        &self.0.target
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
    pub const fn new(kind: CodegenInstanceDependencyKind, instance: CodegenInstanceKey) -> Self {
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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

    /// Returns the concrete hidden frame identity when this instance owns a protected frame.
    pub fn protected_frame_identity(&self) -> Option<ProtectedAsyncFrameId> {
        let MirUnitKind::ProtectedAsyncFrame(template) = self.mir.kind() else {
            return None;
        };

        Some(self.key.protected_frame_identity(*template))
    }
}

/// A contract violation that prevents creation of one concrete code generation instance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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
    use bray_ir::{
        MirBlockKind, MirFrameDescriptor, MirFrameState, MirFrameStateId, MirSourceAnchor,
        MirTargetContract, MirTerminatorKind, MirUnitBuilder, MirUnitKind,
    };
    use bray_runtime_interface::{
        ProtectedAsyncFrameId, ProtectedFrameAbiVersions, RuntimeAbiVersion,
    };
    use bray_symbols::{SemanticValueStore, TypeData};
    use bray_target::{TargetIdentity, TargetProfile};
    use bray_testing::{test_mir_unit, test_mir_unit_with_declaration};

    use super::{
        CodegenGenericArgument, CodegenInstance, CodegenInstanceBuildError,
        CodegenInstanceDependency, CodegenInstanceDependencyKind, CodegenInstanceKey,
        CodegenSpecialization, CodegenValueKey,
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

        let dependency =
            CodegenInstanceDependency::definition(CodegenInstanceKey::non_generic(&other));

        assert!(CodegenInstance::try_new(key, mir, [dependency]).is_ok());
    }

    #[test]
    fn witnesses_and_dependencies_are_canonical_sets() {
        let mir = test_mir_unit(4);

        let dependency = CodegenInstanceDependency::definition(CodegenInstanceKey::non_generic(
            &test_mir_unit_with_declaration(8, 1),
        ));

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

        let started =
            CodegenInstanceDependency::new(CodegenInstanceDependencyKind::StartedTask, dependency);

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
    fn target_properties_separate_otherwise_equal_concrete_instances() {
        let mir = test_mir_unit(4);
        let original = CodegenInstanceKey::non_generic(&mir);

        let Some(identity) = TargetIdentity::try_new("x86_64-example-other") else {
            panic!("alternate target identity must be valid");
        };

        let Ok(profile) = TargetProfile::try_new(
            identity,
            bray_target::test_support::test_target_machine(),
            bray_target::test_support::test_target_properties(),
        ) else {
            panic!("alternate target profile must be valid");
        };

        let alternate_target = MirTargetContract::new(profile, mir.target().runtime_abi());

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

    #[test]
    fn structural_specializations_ignore_semantic_store_and_interning_order() {
        let Ok(first_store) = SemanticValueStore::try_new() else {
            panic!("first semantic value store must be available");
        };

        let Ok(second_store) = SemanticValueStore::try_new() else {
            panic!("second semantic value store must be available");
        };

        let Ok(first_type) = first_store.intern_type(TypeData::Error) else {
            panic!("first test type must intern");
        };

        let _ = second_store.intern_type(TypeData::tuple([]));

        let Ok(second_type) = second_store.intern_type(TypeData::Error) else {
            panic!("second test type must intern");
        };

        assert_ne!(first_type, second_type);

        let mir = test_mir_unit(4);
        let argument = CodegenGenericArgument::Type(CodegenValueKey::new([7; 32]));

        let first = CodegenInstanceKey::new(
            mir.key().clone(),
            CodegenSpecialization::generic([argument]),
            [],
            mir.target().clone(),
        );

        let second = CodegenInstanceKey::new(
            mir.key().clone(),
            CodegenSpecialization::generic([argument]),
            [],
            mir.target().clone(),
        );

        assert_eq!(first, second);
    }

    #[test]
    fn async_specializations_receive_distinct_concrete_frame_identities() {
        let template = ProtectedAsyncFrameId::new([9; 32]);
        let mir = protected_frame_mir(template);
        let first_argument = CodegenGenericArgument::Type(CodegenValueKey::new([1; 32]));
        let second_argument = CodegenGenericArgument::Type(CodegenValueKey::new([2; 32]));

        let first_key = CodegenInstanceKey::new(
            mir.key().clone(),
            CodegenSpecialization::generic([first_argument]),
            [],
            mir.target().clone(),
        );

        let second_key = CodegenInstanceKey::new(
            mir.key().clone(),
            CodegenSpecialization::generic([second_argument]),
            [],
            mir.target().clone(),
        );

        let Ok(first) = CodegenInstance::try_new(first_key, mir.clone(), []) else {
            panic!("first async specialization must validate");
        };

        let Ok(second) = CodegenInstance::try_new(second_key, mir, []) else {
            panic!("second async specialization must validate");
        };

        assert_ne!(
            first.protected_frame_identity(),
            second.protected_frame_identity()
        );
    }

    fn protected_frame_mir(frame: ProtectedAsyncFrameId) -> bray_ir::MirUnit {
        let bound = bray_testing::test_bound_unit(8);
        let source = bound.key().source();

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::ProtectedAsyncFrame(frame),
            bray_testing::test_mir_target(),
        );

        let source = MirSourceAnchor::from(source);

        let Ok(entry) = builder.push_block(source.clone(), MirBlockKind::Ordinary) else {
            panic!("test protected-frame block must validate");
        };

        let Ok(()) = builder.set_terminator(entry, source, MirTerminatorKind::Return(None)) else {
            panic!("test protected-frame terminator must validate");
        };

        let state = MirFrameState::new(
            MirFrameStateId::new(0),
            entry,
            bray_ir::MirFrameExecutionState::new([], []),
        );

        let Ok(descriptor) = MirFrameDescriptor::try_new(
            frame,
            RuntimeAbiVersion::CURRENT,
            ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::CURRENT),
            bray_testing::test_mir_type(),
            [state],
        ) else {
            panic!("test frame descriptor must validate");
        };

        if let Err(error) = builder.set_frame_descriptor(descriptor) {
            panic!("test frame descriptor must commit: {error:?}");
        }

        match builder.finish(entry) {
            Ok(unit) => unit,
            Err(error) => panic!("test protected-frame MIR must validate: {error:?}"),
        }
    }
}
