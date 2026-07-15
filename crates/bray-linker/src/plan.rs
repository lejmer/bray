use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::{NonEmptySharedStr, sorted_unique_shared_slice};
use bray_symbols::ProductIdentity;

use crate::{
    DebugLinkPolicy, LinkInput, LinkInputId, LinkPolicy, LinkTarget, LinkedArtifactKind,
    LinkedArtifactRequirement, LinkedProductKind, LinkerDriverIdentity, PlannedLinkedArtifact,
    StagingDestinationId,
};

/// Canonical native symbol spelling selected before driver translation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkSymbolName(NonEmptySharedStr);

impl LinkSymbolName {
    /// Creates a symbol name unless its canonical spelling is empty.
    pub fn try_new(name: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(name).map(Self)
    }

    /// Returns the canonical target symbol spelling.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Search-path category retained for target-specific driver translation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LinkSearchPathKind {
    /// Native library search path.
    Library,
    /// Platform framework search path.
    Framework,
}

/// One validated source-ordered linker search path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LinkSearchPath {
    kind: LinkSearchPathKind,
    path: PathBuf,
}

impl LinkSearchPath {
    /// Creates a search path when its filesystem path is non-empty.
    pub fn try_new(
        kind: LinkSearchPathKind,
        path: impl Into<PathBuf>,
    ) -> Result<Self, LinkSearchPathBuildError> {
        let path = path.into();

        if path.as_os_str().is_empty() {
            return Err(LinkSearchPathBuildError::EmptyPath);
        }

        Ok(Self { kind, path })
    }

    /// Returns the search-path category.
    pub const fn kind(&self) -> LinkSearchPathKind {
        self.kind
    }

    /// Returns the exact driver-facing filesystem path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A contract violation that prevents search-path construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkSearchPathBuildError {
    /// The filesystem path is empty.
    EmptyPath,
}

/// Task-local construction state for one immutable validated link plan.
#[derive(Debug)]
pub struct LinkPlanBuilder {
    product: ProductIdentity,
    product_kind: LinkedProductKind,
    target: LinkTarget,
    driver: LinkerDriverIdentity,
    inputs: Vec<LinkInput>,
    outputs: Vec<PlannedLinkedArtifact>,
    entry_point: Option<LinkSymbolName>,
    exported_symbols: Vec<LinkSymbolName>,
    retained_symbols: Vec<LinkSymbolName>,
    search_paths: Vec<LinkSearchPath>,
    policy: LinkPolicy,
}

impl LinkPlanBuilder {
    /// Starts a link plan from already selected product, target, and driver identities.
    pub fn new(
        product: ProductIdentity,
        product_kind: LinkedProductKind,
        target: LinkTarget,
        driver: LinkerDriverIdentity,
        policy: LinkPolicy,
    ) -> Self {
        Self {
            product,
            product_kind,
            target,
            driver,
            inputs: Vec::new(),
            outputs: Vec::new(),
            entry_point: None,
            exported_symbols: Vec::new(),
            retained_symbols: Vec::new(),
            search_paths: Vec::new(),
            policy,
        }
    }

    /// Appends one input in language-defined linker order.
    pub fn push_input(&mut self, input: LinkInput) {
        self.inputs.push(input);
    }

    /// Adds one emitter-owned linked-output destination.
    pub fn push_output(&mut self, output: PlannedLinkedArtifact) {
        self.outputs.push(output);
    }

    /// Selects the canonical native entry point.
    pub fn set_entry_point(&mut self, entry_point: LinkSymbolName) {
        self.entry_point = Some(entry_point);
    }

    /// Adds one exported native symbol.
    pub fn push_exported_symbol(&mut self, symbol: LinkSymbolName) {
        self.exported_symbols.push(symbol);
    }

    /// Adds one native symbol that dead stripping must retain.
    pub fn push_retained_symbol(&mut self, symbol: LinkSymbolName) {
        self.retained_symbols.push(symbol);
    }

    /// Appends one library or framework search path in driver-visible order.
    pub fn push_search_path(&mut self, search_path: LinkSearchPath) {
        self.search_paths.push(search_path);
    }

    /// Replaces the target-independent link policy.
    pub fn set_policy(&mut self, policy: LinkPolicy) {
        self.policy = policy;
    }

    /// Validates and freezes the complete link plan.
    pub fn finish(self) -> Result<LinkPlan, LinkPlanBuildError> {
        LinkPlan::try_from_builder(self)
    }
}

/// Complete immutable typed description of one native link or archive operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkPlan {
    product: ProductIdentity,
    product_kind: LinkedProductKind,
    target: LinkTarget,
    driver: LinkerDriverIdentity,
    inputs: Arc<[LinkInput]>,
    outputs: Arc<[PlannedLinkedArtifact]>,
    entry_point: Option<LinkSymbolName>,
    exported_symbols: Arc<[LinkSymbolName]>,
    retained_symbols: Arc<[LinkSymbolName]>,
    search_paths: Arc<[LinkSearchPath]>,
    policy: LinkPolicy,
}

impl LinkPlan {
    fn try_from_builder(mut builder: LinkPlanBuilder) -> Result<Self, LinkPlanBuildError> {
        validate_inputs(&builder.inputs)?;
        validate_search_paths(&builder.search_paths)?;
        validate_entry_point(builder.product_kind, builder.entry_point.as_ref())?;

        builder
            .outputs
            .sort_unstable_by_key(|output| output.destination().id());

        validate_outputs(builder.product_kind, builder.policy, &builder.outputs)?;

        Ok(Self {
            product: builder.product,
            product_kind: builder.product_kind,
            target: builder.target,
            driver: builder.driver,
            inputs: builder.inputs.into(),
            outputs: builder.outputs.into(),
            entry_point: builder.entry_point,
            exported_symbols: sorted_unique_shared_slice(builder.exported_symbols),
            retained_symbols: sorted_unique_shared_slice(builder.retained_symbols),
            search_paths: builder.search_paths.into(),
            policy: builder.policy,
        })
    }

    /// Returns the selected package product.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns the native product category.
    pub const fn product_kind(&self) -> LinkedProductKind {
        self.product_kind
    }

    /// Returns the backend-neutral target contract.
    pub const fn target(&self) -> &LinkTarget {
        &self.target
    }

    /// Returns the selected linker-driver identity.
    pub const fn driver(&self) -> &LinkerDriverIdentity {
        &self.driver
    }

    /// Returns inputs in exact driver-visible source order.
    pub fn inputs(&self) -> &[LinkInput] {
        &self.inputs
    }

    /// Returns one input by stable identity.
    pub fn input(&self, id: LinkInputId) -> Option<&LinkInput> {
        self.inputs.iter().find(|input| input.id() == id)
    }

    /// Returns staged outputs in canonical staging-identity order.
    pub fn outputs(&self) -> &[PlannedLinkedArtifact] {
        &self.outputs
    }

    /// Returns one planned output by emitter-owned staging identity.
    pub fn output(&self, id: StagingDestinationId) -> Option<&PlannedLinkedArtifact> {
        self.outputs
            .binary_search_by_key(&id, |output| output.destination().id())
            .ok()
            .map(|index| &self.outputs[index])
    }

    /// Returns the selected native entry point when the product has one.
    pub const fn entry_point(&self) -> Option<&LinkSymbolName> {
        self.entry_point.as_ref()
    }

    /// Returns exported symbols in canonical deterministic order.
    pub fn exported_symbols(&self) -> &[LinkSymbolName] {
        &self.exported_symbols
    }

    /// Returns retained symbols in canonical deterministic order.
    pub fn retained_symbols(&self) -> &[LinkSymbolName] {
        &self.retained_symbols
    }

    /// Returns library and framework search paths in driver-visible order.
    pub fn search_paths(&self) -> &[LinkSearchPath] {
        &self.search_paths
    }

    /// Returns the selected target-independent link policy.
    pub const fn policy(&self) -> LinkPolicy {
        self.policy
    }
}

/// A contract violation that prevents immutable link-plan publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkPlanBuildError {
    /// The plan contains no native link inputs.
    MissingInputs,
    /// One stable link-input identity appears more than once.
    DuplicateInput(LinkInputId),
    /// One search path appears more than once.
    DuplicateSearchPath,
    /// The product has no required primary staging output.
    MissingPrimaryOutput,
    /// The product has more than one primary staging output.
    MultiplePrimaryOutputs,
    /// The primary staging output is optional.
    OptionalPrimaryOutput,
    /// One staging identity appears more than once.
    DuplicateOutput(StagingDestinationId),
    /// Two staging identities resolve to the same filesystem path.
    OutputPathCollision {
        /// First staging identity using the path.
        first: StagingDestinationId,
        /// Conflicting staging identity using the path.
        second: StagingDestinationId,
    },
    /// An executable product has no selected native entry point.
    MissingEntryPoint,
    /// A static-library product contains an inapplicable native entry point.
    UnexpectedEntryPoint,
    /// Companion debug output was requested without a staged debug destination.
    MissingDebugCompanion,
    /// A debug companion was staged without companion debug policy.
    UnexpectedDebugCompanion,
    /// One staged output category is incompatible with the selected product category.
    IncompatibleOutputKind {
        /// Selected native product category.
        product: LinkedProductKind,
        /// Incompatible staged artifact category.
        artifact: LinkedArtifactKind,
    },
}

fn validate_inputs(inputs: &[LinkInput]) -> Result<(), LinkPlanBuildError> {
    if inputs.is_empty() {
        return Err(LinkPlanBuildError::MissingInputs);
    }

    let mut identities = BTreeSet::new();

    for input in inputs {
        if !identities.insert(input.id()) {
            return Err(LinkPlanBuildError::DuplicateInput(input.id()));
        }
    }

    Ok(())
}

fn validate_search_paths(search_paths: &[LinkSearchPath]) -> Result<(), LinkPlanBuildError> {
    let mut unique = BTreeSet::new();

    for search_path in search_paths {
        if !unique.insert(search_path) {
            return Err(LinkPlanBuildError::DuplicateSearchPath);
        }
    }

    Ok(())
}

fn validate_entry_point(
    product_kind: LinkedProductKind,
    entry_point: Option<&LinkSymbolName>,
) -> Result<(), LinkPlanBuildError> {
    match (product_kind, entry_point) {
        (LinkedProductKind::Executable, None) => Err(LinkPlanBuildError::MissingEntryPoint),
        (LinkedProductKind::StaticLibrary, Some(_)) => {
            Err(LinkPlanBuildError::UnexpectedEntryPoint)
        }
        (LinkedProductKind::Executable | LinkedProductKind::SharedLibrary, Some(_))
        | (LinkedProductKind::SharedLibrary | LinkedProductKind::StaticLibrary, None) => Ok(()),
    }
}

fn validate_outputs(
    product_kind: LinkedProductKind,
    policy: LinkPolicy,
    outputs: &[PlannedLinkedArtifact],
) -> Result<(), LinkPlanBuildError> {
    let primary_kind = product_kind.primary_artifact_kind();

    let mut primary = None;
    let mut destinations = BTreeSet::new();
    let mut paths = BTreeMap::new();
    let mut has_debug_companion = false;

    for output in outputs {
        let destination = output.destination().id();

        if !product_kind.accepts_artifact_kind(output.kind()) {
            return Err(LinkPlanBuildError::IncompatibleOutputKind {
                product: product_kind,
                artifact: output.kind(),
            });
        }

        if !destinations.insert(destination) {
            return Err(LinkPlanBuildError::DuplicateOutput(destination));
        }

        if let Some(first) = paths.insert(output.destination().path_key(), destination) {
            return Err(LinkPlanBuildError::OutputPathCollision {
                first,
                second: destination,
            });
        }

        if output.kind() == primary_kind && primary.replace(output).is_some() {
            return Err(LinkPlanBuildError::MultiplePrimaryOutputs);
        }

        has_debug_companion |= output.kind() == LinkedArtifactKind::DebugCompanion;
    }

    let Some(primary) = primary else {
        return Err(LinkPlanBuildError::MissingPrimaryOutput);
    };

    if primary.requirement() != LinkedArtifactRequirement::Required {
        return Err(LinkPlanBuildError::OptionalPrimaryOutput);
    }

    match (policy.debug(), has_debug_companion) {
        (DebugLinkPolicy::Companion, false) => Err(LinkPlanBuildError::MissingDebugCompanion),
        (DebugLinkPolicy::None | DebugLinkPolicy::Embedded, true) => {
            Err(LinkPlanBuildError::UnexpectedDebugCompanion)
        }
        (DebugLinkPolicy::Companion, true)
        | (DebugLinkPolicy::None | DebugLinkPolicy::Embedded, false) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{
        link_input, link_plan_builder, planned_output, planned_output_with_key,
    };
    use crate::{
        DebugLinkPolicy, LinkInputId, LinkPlanBuildError, LinkPolicy, LinkSymbolName,
        LinkedArtifactKind, LinkedArtifactRequirement, StagingDestinationId,
    };

    #[test]
    fn plans_preserve_input_order_and_canonicalize_symbol_sets() {
        let mut builder = link_plan_builder();

        builder.push_input(link_input(2, "second.o"));
        builder.push_input(link_input(1, "first.o"));

        builder.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            "application.stage",
        ));

        builder.set_entry_point(symbol("_start"));

        builder.push_exported_symbol(symbol("zeta"));
        builder.push_exported_symbol(symbol("alpha"));
        builder.push_exported_symbol(symbol("alpha"));

        let Ok(plan) = builder.finish() else {
            panic!("complete test plan must be valid");
        };

        assert_eq!(
            plan.inputs()
                .iter()
                .map(|input| input.id())
                .collect::<Vec<_>>(),
            [LinkInputId::new(2), LinkInputId::new(1)]
        );
        assert_eq!(
            plan.exported_symbols()
                .iter()
                .map(LinkSymbolName::as_str)
                .collect::<Vec<_>>(),
            ["alpha", "zeta"]
        );
    }

    #[test]
    fn plans_reject_duplicate_inputs_and_output_collisions() {
        let mut duplicate_inputs = link_plan_builder();

        duplicate_inputs.push_input(link_input(0, "first.o"));
        duplicate_inputs.push_input(link_input(0, "second.o"));

        duplicate_inputs.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            "application.stage",
        ));

        duplicate_inputs.set_entry_point(symbol("_start"));

        assert_eq!(
            duplicate_inputs.finish(),
            Err(LinkPlanBuildError::DuplicateInput(LinkInputId::new(0)))
        );

        let mut colliding_outputs = link_plan_builder();

        colliding_outputs.push_input(link_input(0, "main.o"));

        colliding_outputs.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            "same.stage",
        ));
        colliding_outputs.push_output(planned_output_with_key(
            1,
            LinkedArtifactKind::DebugCompanion,
            LinkedArtifactRequirement::Optional,
            ".\\same.stage",
            "same.stage",
        ));

        colliding_outputs.set_entry_point(symbol("_start"));
        colliding_outputs.set_policy(companion_debug_policy());

        assert_eq!(
            colliding_outputs.finish(),
            Err(LinkPlanBuildError::OutputPathCollision {
                first: StagingDestinationId::new(0),
                second: StagingDestinationId::new(1),
            })
        );
    }

    #[test]
    fn plans_require_entry_points_and_matching_primary_outputs() {
        let mut missing_entry = link_plan_builder();

        missing_entry.push_input(link_input(0, "main.o"));

        missing_entry.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            "application.stage",
        ));

        assert_eq!(
            missing_entry.finish(),
            Err(LinkPlanBuildError::MissingEntryPoint)
        );

        let mut missing_primary = link_plan_builder();

        missing_primary.push_input(link_input(0, "main.o"));

        missing_primary.push_output(planned_output(
            1,
            LinkedArtifactKind::PlatformCompanion,
            LinkedArtifactRequirement::Optional,
            "application.meta.stage",
        ));

        missing_primary.set_entry_point(symbol("_start"));

        assert_eq!(
            missing_primary.finish(),
            Err(LinkPlanBuildError::MissingPrimaryOutput)
        );
    }

    #[test]
    fn plans_canonicalize_staging_outputs() {
        let mut builder = link_plan_builder();

        builder.push_input(link_input(0, "main.o"));

        builder.push_output(planned_output(
            2,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            "application.stage",
        ));
        builder.push_output(planned_output(
            1,
            LinkedArtifactKind::PlatformCompanion,
            LinkedArtifactRequirement::Optional,
            "application.meta.stage",
        ));

        builder.set_entry_point(symbol("_start"));

        let Ok(plan) = builder.finish() else {
            panic!("complete test plan must be valid");
        };

        assert_eq!(
            plan.outputs()
                .iter()
                .map(|output| output.destination().id())
                .collect::<Vec<_>>(),
            [StagingDestinationId::new(1), StagingDestinationId::new(2)]
        );
    }

    #[test]
    fn plans_require_debug_policy_and_staging_to_agree() {
        let mut missing_companion = link_plan_builder();

        missing_companion.push_input(link_input(0, "main.o"));

        missing_companion.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            "application.stage",
        ));

        missing_companion.set_entry_point(symbol("_start"));
        missing_companion.set_policy(companion_debug_policy());

        assert_eq!(
            missing_companion.finish(),
            Err(LinkPlanBuildError::MissingDebugCompanion)
        );

        let mut unexpected_companion = link_plan_builder();

        unexpected_companion.push_input(link_input(0, "main.o"));

        unexpected_companion.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            "application.stage",
        ));
        unexpected_companion.push_output(planned_output(
            1,
            LinkedArtifactKind::DebugCompanion,
            LinkedArtifactRequirement::Optional,
            "application.debug.stage",
        ));

        unexpected_companion.set_entry_point(symbol("_start"));

        assert_eq!(
            unexpected_companion.finish(),
            Err(LinkPlanBuildError::UnexpectedDebugCompanion)
        );
    }

    #[test]
    fn plans_reject_outputs_for_another_product_kind() {
        let mut builder = link_plan_builder();

        builder.push_input(link_input(0, "main.o"));

        builder.push_output(planned_output(
            0,
            LinkedArtifactKind::Executable,
            LinkedArtifactRequirement::Required,
            "application.stage",
        ));
        builder.push_output(planned_output(
            1,
            LinkedArtifactKind::ImportLibrary,
            LinkedArtifactRequirement::Optional,
            "application.lib.stage",
        ));

        builder.set_entry_point(symbol("_start"));

        assert_eq!(
            builder.finish(),
            Err(LinkPlanBuildError::IncompatibleOutputKind {
                product: crate::LinkedProductKind::Executable,
                artifact: LinkedArtifactKind::ImportLibrary,
            })
        );
    }

    #[test]
    fn plans_are_immutable_and_shareable_after_validation() {
        let plan = crate::test_support::link_plan();

        assert_eq!(plan.inputs().len(), 1);
        assert_eq!(plan.outputs().len(), 1);

        assert_send_sync::<super::LinkPlan>();
    }

    fn symbol(name: &str) -> LinkSymbolName {
        let Some(symbol) = LinkSymbolName::try_new(name) else {
            panic!("test symbol name must be valid");
        };

        symbol
    }

    fn companion_debug_policy() -> LinkPolicy {
        LinkPolicy::new(
            crate::DeadStripPolicy::Preserve,
            crate::SectionGarbageCollectionPolicy::Preserve,
            DebugLinkPolicy::Companion,
            None,
        )
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
