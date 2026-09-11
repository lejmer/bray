use std::collections::BTreeSet;
use std::sync::Arc;

use bray_bound_tree::{BoundNodeOrigin, StorageCleanupPart, StorageCleanupProjectionKind};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAggregate, MirAggregateKind, MirBlockId, MirCleanupPhase, MirEdge, MirGeneratorOperation,
    MirImmediateValue, MirOperand, MirOperationCommit, MirOperationKind, MirPlace, MirProjection,
    MirProjectionKind, MirSourceAnchor, MirStorageId, MirStorageKind, MirStoreKind,
    MirTerminatorKind, MirUnitBuildError, MirUnitBuilder,
};
use bray_symbols::{TypeData, TypeId};

use super::LoweringError;
use super::lowerer::Lowerer;
use super::projection::static_cleanup_projection_kind;

pub(super) struct InitializationState<'unit> {
    pub(super) guard: MirPlace,
    pub(super) parts: Vec<InitializedPart<'unit>>,
}

pub(super) struct InitializedPart<'unit> {
    pub(super) plan: &'unit StorageCleanupPart,
    pub(super) guard: MirPlace,
    pub(super) array_types: Arc<[TypeId]>,
}

impl Lowerer<'_> {
    pub(super) fn initialize_cleanup_guards(
        &mut self,
        entry: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<(), LoweringError> {
        let accesses = self
            .input
            .lowering_plans()
            .initialization_guards()
            .collect::<BTreeSet<_>>();

        if accesses.is_empty() {
            return Ok(());
        }

        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

        for access in accesses {
            let storage = self.input.storage_plan();

            let identity = storage
                .root_identity(access)
                .ok_or(LoweringError::MissingStorageIdentity(access))?;

            let record = storage
                .identity(identity)
                .ok_or(LoweringError::MissingStorageIdentityRecord(identity))?;

            let access = storage
                .access(access)
                .ok_or(LoweringError::MissingStorageAccessRecord(access))?;

            let initialized = record.is_initialized_at_entry();

            let ty = storage
                .storage_type(identity)
                .ok_or(LoweringError::MissingStorageIdentityRecord(identity))?;

            let place =
                self.place_for_identity(identity, ty, BoundNodeOrigin::source(access.source()))?;

            let guard = self.new_initialization_guard(entry, source, boolean, &[], initialized)?;
            let mut parts = Vec::new();

            if let Some(checked) = self.input.lowering_plans().cleanup_parts(identity) {
                for part in checked {
                    let mut array_types = Vec::new();
                    let mut element = boolean;

                    for projection in part.projections().iter().rev() {
                        if let StorageCleanupProjectionKind::ArrayElements(length) =
                            projection.projection()
                        {
                            element = self
                                .input
                                .semantic_values()
                                .intern_type(TypeData::Array { element, length })?;

                            array_types.push(element);
                        }
                    }

                    array_types.reverse();

                    let flag = self.new_initialization_guard(
                        entry,
                        source,
                        boolean,
                        &array_types,
                        initialized,
                    )?;

                    parts.push(InitializedPart {
                        plan: part,
                        guard: flag,
                        array_types: array_types.into(),
                    });
                }
            }

            self.initialization_guards
                .insert(place.storage(), InitializationState { guard, parts });
        }

        Ok(())
    }

    pub(super) fn new_initialization_guard(
        &mut self,
        entry: MirBlockId,
        source: &MirSourceAnchor,
        boolean: TypeId,
        array_types: &[TypeId],
        initialized: bool,
    ) -> Result<MirPlace, MirUnitBuildError> {
        let ty = array_types.first().copied().unwrap_or(boolean);

        let flag =
            self.builder
                .push_storage(Self::retained_source(source), MirStorageKind::Local, ty)?;

        let flag = MirPlace::new(flag, [], ty);

        let value = initialization_value(
            &mut self.builder,
            entry,
            source,
            boolean,
            array_types,
            initialized,
        )?;

        self.builder.push_operation(
            entry,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: Self::retained_place(&flag),
                value,
            },
            None,
        )?;

        Ok(flag)
    }

    pub(super) fn push_operation(
        &mut self,
        block: MirBlockId,
        source: MirSourceAnchor,
        kind: MirOperationKind,
        result_type: Option<TypeId>,
    ) -> Result<MirOperationCommit, MirUnitBuildError> {
        if self.initialization_guards.is_empty() && self.receiver_allowance.is_none() {
            return self
                .builder
                .push_operation(block, source, kind, result_type);
        }

        let mut moved = Vec::new();

        kind.for_each_operand(|operand| {
            if let MirOperand::Move(place) = operand
                && !moved.contains(place)
            {
                moved.push(Self::retained_place(place));
            }
        });

        for place in moved {
            self.set_receiver_allowance(block, &source, &place, false)?;
            self.set_storage_initialized(block, &source, &place, false)?;
        }

        match &kind {
            MirOperationKind::Generator(MirGeneratorOperation::Finish { destination: place })
            | MirOperationKind::Destroy(place)
            | MirOperationKind::DestructorRemainder { place, .. }
            | MirOperationKind::Abandon {
                action:
                    bray_ir::MirAbandonmentAction::Destroy | bray_ir::MirAbandonmentAction::Destructor,
                place,
            }
            | MirOperationKind::Cleanup {
                phase: MirCleanupPhase::LifecycleResolution,
                place,
            } => {
                if !matches!(&kind, MirOperationKind::DestructorRemainder { .. }) {
                    self.set_receiver_allowance(block, &source, place, false)?;
                }

                self.set_storage_initialized(block, &source, place, false)?;
            }
            _ => {}
        }

        let initialized = match &kind {
            MirOperationKind::Store { destination, .. }
            | MirOperationKind::Generator(MirGeneratorOperation::Begin { destination, .. }) => {
                Some(Self::retained_place(destination))
            }
            _ => None,
        };

        let commit = self.builder.push_operation(
            block,
            Self::retained_source(&source),
            kind,
            result_type,
        )?;

        if let Some(place) = initialized {
            self.set_receiver_allowance(block, &source, &place, true)?;
            self.set_storage_initialized(block, &source, &place, true)?;
        }

        Ok(commit)
    }

    pub(super) fn set_storage_initialized(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: &MirPlace,
        initialized: bool,
    ) -> Result<(), MirUnitBuildError> {
        let owned_place = self.ownership_place(place);
        let place = &owned_place;

        if place.projections().is_empty() {
            return self.set_root_initialized(block, source, place.storage(), initialized);
        }

        let Some(state) = self.initialization_guards.get(&place.storage()) else {
            return Ok(());
        };

        for part in &state.parts {
            if let Some((guard, dimensions)) = part_guard_for_place(part, state.guard.ty(), place) {
                let value = initialization_value(
                    &mut self.builder,
                    block,
                    source,
                    state.guard.ty(),
                    &part.array_types[dimensions..],
                    initialized,
                )?;

                let operation = MirOperationKind::Store {
                    kind: MirStoreKind::Assign,
                    destination: guard,
                    value,
                };

                self.builder.push_operation(
                    block,
                    Self::retained_source(source),
                    operation,
                    None,
                )?;
            }
        }

        Ok(())
    }

    pub(super) fn set_terminator(
        &mut self,
        block: MirBlockId,
        source: MirSourceAnchor,
        mut kind: MirTerminatorKind,
    ) -> Result<(), MirUnitBuildError> {
        if !self.initialization_guards.is_empty() || self.receiver_allowance.is_some() {
            let mut moved = Vec::new();

            kind.for_each_input(|operand| {
                if let MirOperand::Move(place) = operand
                    && !moved.contains(place)
                {
                    moved.push(Self::retained_place(place));
                }
            });

            for place in moved {
                self.set_receiver_allowance(block, &source, &place, false)?;
                self.set_storage_initialized(block, &source, &place, false)?;
            }

            kind.try_for_each_edge_mut(|edge| self.guard_edge_moves(&source, edge))?;
        }

        self.builder.set_terminator(block, source, kind)
    }

    fn guard_edge_moves(
        &mut self,
        source: &MirSourceAnchor,
        edge: &mut MirEdge,
    ) -> Result<(), MirUnitBuildError> {
        let mut moved = Vec::new();

        for argument in edge.arguments() {
            argument.for_each_operand(|operand| {
                if let MirOperand::Move(place) = operand
                    && (self.receiver_allowance.is_some()
                        || self
                            .initialization_guards
                            .contains_key(&self.ownership_place(place).storage()))
                    && !moved.contains(place)
                {
                    moved.push(Self::retained_place(place));
                }
            });
        }

        if moved.is_empty() {
            return Ok(());
        }

        let bridge = self.builder.push_block(
            Self::retained_source(source),
            self.builder.block_kind(edge.target())?,
        )?;

        let mut arguments = Vec::with_capacity(edge.arguments().len());

        for argument in edge.arguments() {
            let ty = self.builder.operand_type(argument)?;

            let parameter =
                self.builder
                    .push_block_parameter(bridge, Self::retained_source(source), ty)?;

            arguments.push(MirOperand::Value(parameter));
        }

        let mut incoming = edge
            .arguments()
            .iter()
            .map(Self::retained_operand)
            .collect::<Vec<_>>();

        for place in moved {
            let mut projections = Vec::with_capacity(place.projections().len());

            for projection in place.projections() {
                let kind = match projection.kind() {
                    MirProjectionKind::Index(index) => MirProjectionKind::Index(
                        self.forward_cleanup_selector(bridge, source, index, &mut incoming)?,
                    ),
                    MirProjectionKind::Slice { start, end } => MirProjectionKind::Slice {
                        start: start
                            .as_ref()
                            .map(|operand| {
                                self.forward_cleanup_selector(
                                    bridge,
                                    source,
                                    operand,
                                    &mut incoming,
                                )
                            })
                            .transpose()?,
                        end: end
                            .as_ref()
                            .map(|operand| {
                                self.forward_cleanup_selector(
                                    bridge,
                                    source,
                                    operand,
                                    &mut incoming,
                                )
                            })
                            .transpose()?,
                    },
                    // Non-selector projections contain only compact checked identities.
                    kind => kind.clone(),
                };

                projections.push(MirProjection::new(
                    kind,
                    projection.source_type(),
                    projection.result_type(),
                ));
            }

            let selected = MirPlace::new(place.storage(), projections, place.ty());

            self.set_receiver_allowance(bridge, source, &selected, false)?;
            self.set_storage_initialized(bridge, source, &selected, false)?;
        }

        self.builder.set_terminator(
            bridge,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(edge.target(), arguments)),
        )?;

        *edge = MirEdge::new(bridge, incoming);

        Ok(())
    }

    fn forward_cleanup_selector(
        &mut self,
        bridge: MirBlockId,
        source: &MirSourceAnchor,
        selector: &MirOperand,
        incoming: &mut Vec<MirOperand>,
    ) -> Result<MirOperand, MirUnitBuildError> {
        let ty = self.builder.operand_type(selector)?;

        let parameter =
            self.builder
                .push_block_parameter(bridge, Self::retained_source(source), ty)?;

        incoming.push(Self::retained_operand(selector));

        Ok(MirOperand::Value(parameter))
    }

    fn set_root_initialized(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        storage: MirStorageId,
        initialized: bool,
    ) -> Result<(), MirUnitBuildError> {
        let Some(state) = self.initialization_guards.get(&storage) else {
            return Ok(());
        };

        for (flag, array_types) in std::iter::once((&state.guard, &[][..])).chain(
            state
                .parts
                .iter()
                .map(|part| (&part.guard, part.array_types.as_ref())),
        ) {
            let value = initialization_value(
                &mut self.builder,
                block,
                source,
                state.guard.ty(),
                array_types,
                initialized,
            )?;

            let operation = MirOperationKind::Store {
                kind: MirStoreKind::Assign,
                destination: Self::retained_place(flag),
                value,
            };

            self.builder
                .push_operation(block, Self::retained_source(source), operation, None)?;
        }

        Ok(())
    }
}

fn initialization_value(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    boolean: TypeId,
    array_types: &[TypeId],
    initialized: bool,
) -> Result<MirOperand, MirUnitBuildError> {
    let mut value = MirOperand::Immediate {
        value: MirImmediateValue::Boolean(initialized),
        ty: boolean,
    };

    for ty in array_types.iter().rev() {
        let commit = builder.push_operation(
            block,
            Lowerer::retained_source(source),
            MirOperationKind::Aggregate(MirAggregate::new(
                MirAggregateKind::RepeatedArray,
                [value],
            )),
            Some(*ty),
        )?;

        value = MirOperand::Value(commit.result().ok_or(
            MirUnitBuildError::MissingOperationResult(commit.operation()),
        )?);
    }

    Ok(value)
}

pub(super) fn part_guard_for_place(
    part: &InitializedPart<'_>,
    boolean: TypeId,
    place: &MirPlace,
) -> Option<(MirPlace, usize)> {
    if place.projections().len() > part.plan.projections().len() {
        return None;
    }

    let mut dimensions = 0;
    let mut guard_projections = Vec::new();

    for (planned, actual) in part.plan.projections().iter().zip(place.projections()) {
        match planned.projection() {
            StorageCleanupProjectionKind::OwnedTarget(_) => {
                if actual.kind() != &MirProjectionKind::Dereference {
                    return None;
                }
            }
            StorageCleanupProjectionKind::Component(_)
            | StorageCleanupProjectionKind::UnionPayloadElement { .. } => {
                if static_cleanup_projection_kind(planned.projection()).as_ref()
                    != Some(actual.kind())
                {
                    return None;
                }
            }
            StorageCleanupProjectionKind::ArrayElements(_) => {
                let kind = match actual.kind() {
                    MirProjectionKind::Index(index) => {
                        MirProjectionKind::Index(Lowerer::retained_operand(index))
                    }
                    MirProjectionKind::ElementFromStart(index) => {
                        MirProjectionKind::ElementFromStart(*index)
                    }
                    MirProjectionKind::ElementFromEnd(index) => {
                        MirProjectionKind::ElementFromEnd(*index)
                    }
                    _ => return None,
                };

                let source_type = *part.array_types.get(dimensions)?;
                dimensions += 1;
                let result_type = part.array_types.get(dimensions).copied().unwrap_or(boolean);

                guard_projections.push(MirProjection::new(kind, source_type, result_type));
            }
        }
    }

    let ty = part.array_types.get(dimensions).copied().unwrap_or(boolean);

    Some((
        MirPlace::new(part.guard.storage(), guard_projections, ty),
        dimensions,
    ))
}
