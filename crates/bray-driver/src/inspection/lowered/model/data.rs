// rust-style: allow(module-too-large, reason = "the MIR inspection DTOs and exhaustive projection form one cohesive serialization contract")

use std::fmt::Write;

use bray_bound_tree::{
    ConstructionInputId, ConstructionTarget, ConversionTarget, PatternOperation,
    PatternProjection,
};
use bray_ir::{
    MirAggregateKind, MirAsyncOperation, MirBinaryOperator, MirBlockKind, MirCallArgument,
    MirCallTarget, MirCleanupEdge, MirCleanupPhase, MirConstructionInput, MirEdge,
    MirFieldReference, MirGeneratorKind, MirGeneratorOperation, MirHostOperation,
    MirImmediateValue, MirOperand, MirOperation, MirOperationKind, MirPanicCause, MirPlace,
    MirProjectionKind, MirSourceAnchor, MirSourceOrigin, MirStorageKind, MirStoreKind,
    MirTaskTerminalState, MirTerminatorKind, MirUnaryOperator, MirUnit, MirUnitKey, MirUnitKind,
    MirValueOrigin,
};
use bray_symbols::{AnySymbolId, BorrowKind, SemanticValueStore, SymbolGraph};
use serde::Serialize;

use crate::inspection::{
    InspectionSourceError, InspectionSources, InspectionSymbolIdentity, InspectionSyntaxAnchor,
    InspectionType, TypeInspectionError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MirInspectionModelError {
    MissingOperation,
    MissingSymbol,
    Source,
    Type,
}

impl From<InspectionSourceError> for MirInspectionModelError {
    fn from(_: InspectionSourceError) -> Self {
        Self::Source
    }
}

impl From<TypeInspectionError> for MirInspectionModelError {
    fn from(_: TypeInspectionError) -> Self {
        Self::Type
    }
}

#[derive(Serialize)]
pub(crate) struct InspectionMirUnit {
    pub(crate) unit_kind: &'static str,
    pub(crate) unit_id: u32,
    pub(crate) key: InspectionMirUnitKey,
    pub(crate) source: InspectionMirSource,
    pub(crate) target: String,
    pub(crate) runtime_abi: String,
    pub(crate) entry: u32,
    pub(crate) frame: Option<InspectionMirFrame>,
    pub(crate) storages: Vec<InspectionMirStorage>,
    pub(crate) values: Vec<InspectionMirValue>,
    pub(crate) blocks: Vec<InspectionMirBlock>,
}

impl InspectionMirUnit {
    pub(crate) fn from_mir(
        mir: &MirUnit,
        symbols: &SymbolGraph,
        semantic_values: &SemanticValueStore,
        sources: &InspectionSources<'_>,
    ) -> Result<Self, MirInspectionModelError> {
        let key = inspection_unit_key(mir.key(), symbols, sources)?;
        let source = inspection_source_origin(mir.source(), sources)?;
        let unit_kind = mir_unit_kind(mir.kind());
        let runtime_abi = mir.target().runtime_abi();

        let storages = mir
            .storages()
            .iter()
            .enumerate()
            .map(|(slot, storage)| {
                Ok(InspectionMirStorage {
                    id: compact_id(slot),
                    storage_kind: storage_kind(storage.kind()),
                    r#type: InspectionType::from_type(
                        semantic_values,
                        symbols,
                        storage.ty(),
                    )?,
                    source: inspection_source_anchor(storage.source(), sources)?,
                })
            })
            .collect::<Result<_, MirInspectionModelError>>()?;

        let values = mir
            .values()
            .iter()
            .enumerate()
            .map(|(slot, value)| {
                let origin = match value.origin() {
                    MirValueOrigin::BlockParameter(block) => InspectionMirValueOrigin {
                        kind: "block_parameter",
                        id: block.slot(),
                    },
                    MirValueOrigin::Operation(operation) => InspectionMirValueOrigin {
                        kind: "operation",
                        id: operation.slot(),
                    },
                };

                Ok(InspectionMirValue {
                    id: compact_id(slot),
                    r#type: InspectionType::from_type(semantic_values, symbols, value.ty())?,
                    source: inspection_source_anchor(value.source(), sources)?,
                    origin,
                })
            })
            .collect::<Result<_, MirInspectionModelError>>()?;

        let blocks = mir
            .blocks()
            .iter()
            .enumerate()
            .map(|(slot, block)| {
                let operations = block
                    .operations()
                    .iter()
                    .map(|id| {
                        let operation = mir
                            .operation(*id)
                            .ok_or(MirInspectionModelError::MissingOperation)?;

                        inspection_operation(
                            id.slot(),
                            operation,
                            symbols,
                            semantic_values,
                            sources,
                        )
                    })
                    .collect::<Result<_, MirInspectionModelError>>()?;

                Ok(InspectionMirBlock {
                    id: compact_id(slot),
                    block_kind: block_kind(block.kind()),
                    source: inspection_source_anchor(block.source(), sources)?,
                    parameters: block.parameters().iter().map(|id| id.slot()).collect(),
                    operations,
                    terminator: inspection_terminator(
                        block.terminator().kind(),
                        block.terminator().source(),
                        symbols,
                        semantic_values,
                        sources,
                    )?,
                })
            })
            .collect::<Result<_, MirInspectionModelError>>()?;

        let frame = mir
            .frame_descriptor()
            .map(|frame| {
                let states = frame
                    .states()
                    .iter()
                    .map(|state| InspectionMirFrameState {
                        id: state.state().raw(),
                        entry: state.entry().slot(),
                        initialized_storages: state
                            .initialized_storages()
                            .iter()
                            .map(|storage| storage.slot())
                            .collect(),
                    })
                    .collect();

                Ok::<_, MirInspectionModelError>(InspectionMirFrame {
                    id: digest_text(frame.frame().digest()),
                    abi: format_abi(frame.abi_version().major(), frame.abi_version().minor()),
                    result_type: InspectionType::from_type(
                        semantic_values,
                        symbols,
                        frame.result_type(),
                    )?,
                    states,
                })
            })
            .transpose()?;

        Ok(Self {
            unit_kind,
            unit_id: mir.unit().raw(),
            key,
            source,
            target: mir.target().identity().as_str().to_owned(),
            runtime_abi: format_abi(runtime_abi.major(), runtime_abi.minor()),
            entry: mir.entry().slot(),
            frame,
            storages,
            values,
            blocks,
        })
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum InspectionMirUnitKey {
    Bound {
        unit_kind: &'static str,
        owner: InspectionSymbolIdentity,
        source: InspectionSyntaxAnchor,
    },
    ExecutableHost {
        package: String,
        product: String,
    },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum InspectionMirSource {
    Source {
        syntax: InspectionSyntaxAnchor,
        synthesis: Option<InspectionMirSynthesis>,
    },
    ExecutableHost {
        package: String,
        product: String,
    },
}

#[derive(Serialize)]
pub(crate) struct InspectionMirSynthesis {
    pub(crate) role: &'static str,
    pub(crate) ordinal: u32,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirFrame {
    pub(crate) id: String,
    pub(crate) abi: String,
    pub(crate) result_type: InspectionType,
    pub(crate) states: Vec<InspectionMirFrameState>,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirFrameState {
    pub(crate) id: u32,
    pub(crate) entry: u32,
    pub(crate) initialized_storages: Vec<u32>,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirStorage {
    pub(crate) id: u32,
    pub(crate) storage_kind: &'static str,
    pub(crate) r#type: InspectionType,
    pub(crate) source: InspectionMirSource,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirValue {
    pub(crate) id: u32,
    pub(crate) r#type: InspectionType,
    pub(crate) source: InspectionMirSource,
    pub(crate) origin: InspectionMirValueOrigin,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirValueOrigin {
    pub(crate) kind: &'static str,
    pub(crate) id: u32,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirBlock {
    pub(crate) id: u32,
    pub(crate) block_kind: &'static str,
    pub(crate) source: InspectionMirSource,
    pub(crate) parameters: Vec<u32>,
    pub(crate) operations: Vec<InspectionMirOperation>,
    pub(crate) terminator: InspectionMirTerminator,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirOperation {
    pub(crate) id: u32,
    pub(crate) operation_kind: &'static str,
    pub(crate) result: Option<u32>,
    pub(crate) source: InspectionMirSource,
    pub(crate) attributes: Vec<InspectionMirAttribute>,
    pub(crate) operands: Vec<InspectionMirNamedOperand>,
    pub(crate) places: Vec<InspectionMirNamedPlace>,
    pub(crate) symbols: Vec<InspectionMirNamedSymbol>,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirAttribute {
    pub(crate) name: &'static str,
    pub(crate) value: InspectionMirAttributeValue,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum InspectionMirAttributeValue {
    Text(String),
    Unsigned(u32),
    Boolean(bool),
}

impl InspectionMirAttributeValue {
    pub(crate) fn text(&self) -> String {
        match self {
            // Callers need one owned representation for both stored text and formatted scalars.
            Self::Text(value) => value.clone(),
            Self::Unsigned(value) => value.to_string(),
            Self::Boolean(value) => value.to_string(),
        }
    }
}

impl From<&str> for InspectionMirAttributeValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for InspectionMirAttributeValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<u32> for InspectionMirAttributeValue {
    fn from(value: u32) -> Self {
        Self::Unsigned(value)
    }
}

impl From<bool> for InspectionMirAttributeValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

#[derive(Serialize)]
pub(crate) struct InspectionMirNamedOperand {
    pub(crate) role: String,
    pub(crate) operand: InspectionMirOperand,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirNamedPlace {
    pub(crate) role: String,
    pub(crate) place: InspectionMirPlace,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirNamedSymbol {
    pub(crate) role: String,
    pub(crate) symbol: InspectionSymbolIdentity,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum InspectionMirOperand {
    Value {
        value: u32,
    },
    Constant {
        value: String,
        r#type: InspectionType,
    },
    Immediate {
        value: String,
        r#type: InspectionType,
    },
    Copy {
        place: InspectionMirPlace,
    },
    Move {
        place: InspectionMirPlace,
    },
}

#[derive(Serialize)]
pub(crate) struct InspectionMirPlace {
    pub(crate) storage: u32,
    pub(crate) r#type: InspectionType,
    pub(crate) projections: Vec<InspectionMirProjection>,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirProjection {
    pub(crate) projection_kind: &'static str,
    pub(crate) detail: Option<String>,
    pub(crate) source_type: InspectionType,
    pub(crate) result_type: InspectionType,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirTerminator {
    pub(crate) terminator_kind: &'static str,
    pub(crate) source: InspectionMirSource,
    pub(crate) operands: Vec<InspectionMirNamedOperand>,
    pub(crate) places: Vec<InspectionMirNamedPlace>,
    pub(crate) edges: Vec<InspectionMirEdge>,
    pub(crate) attributes: Vec<InspectionMirAttribute>,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirEdge {
    pub(crate) role: String,
    pub(crate) target: u32,
    pub(crate) arguments: Vec<InspectionMirOperand>,
    pub(crate) cleanup_phase: Option<&'static str>,
}

struct OperationParts {
    attributes: Vec<InspectionMirAttribute>,
    operands: Vec<InspectionMirNamedOperand>,
    places: Vec<InspectionMirNamedPlace>,
    symbols: Vec<InspectionMirNamedSymbol>,
}

impl OperationParts {
    fn new() -> Self {
        Self {
            attributes: Vec::new(),
            operands: Vec::new(),
            places: Vec::new(),
            symbols: Vec::new(),
        }
    }

    fn attribute(
        &mut self,
        name: &'static str,
        value: impl Into<InspectionMirAttributeValue>,
    ) {
        self.attributes.push(InspectionMirAttribute {
            name,
            value: value.into(),
        });
    }

    fn operand(
        &mut self,
        role: impl Into<String>,
        operand: &MirOperand,
        context: &MirInspectionContext<'_>,
    ) -> Result<(), MirInspectionModelError> {
        self.operands.push(InspectionMirNamedOperand {
            role: role.into(),
            operand: inspection_operand(operand, context)?,
        });

        Ok(())
    }

    fn place(
        &mut self,
        role: impl Into<String>,
        place: &MirPlace,
        context: &MirInspectionContext<'_>,
    ) -> Result<(), MirInspectionModelError> {
        self.places.push(InspectionMirNamedPlace {
            role: role.into(),
            place: inspection_place(place, context)?,
        });

        Ok(())
    }

    fn symbol(&mut self, role: impl Into<String>, symbol: AnySymbolId, symbols: &SymbolGraph) {
        self.symbols.push(InspectionMirNamedSymbol {
            role: role.into(),
            symbol: InspectionSymbolIdentity::from_symbol(symbols, symbol),
        });
    }
}

struct MirInspectionContext<'model> {
    symbols: &'model SymbolGraph,
    semantic_values: &'model SemanticValueStore,
}

fn inspection_operation(
    id: u32,
    operation: &MirOperation,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
    sources: &InspectionSources<'_>,
) -> Result<InspectionMirOperation, MirInspectionModelError> {
    let context = MirInspectionContext {
        symbols,
        semantic_values,
    };

    let mut parts = OperationParts::new();
    let operation_kind = operation_parts(operation.kind(), &mut parts, &context)?;

    Ok(InspectionMirOperation {
        id,
        operation_kind,
        result: operation.result().map(|result| result.slot()),
        source: inspection_source_anchor(operation.source(), sources)?,
        attributes: parts.attributes,
        operands: parts.operands,
        places: parts.places,
        symbols: parts.symbols,
    })
}

fn operation_parts(
    operation: &MirOperationKind,
    parts: &mut OperationParts,
    context: &MirInspectionContext<'_>,
) -> Result<&'static str, MirInspectionModelError> {
    let kind = match operation {
        MirOperationKind::AnonymousCallable(key) => {
            parts.attribute("unit_kind", key.kind().as_str());

            "anonymous_callable"
        }
        MirOperationKind::Store {
            kind,
            destination,
            value,
        } => {
            parts.attribute("mode", store_kind(*kind));
            parts.place("destination", destination, context)?;
            parts.operand("value", value, context)?;

            "store"
        }
        MirOperationKind::Borrow { kind, place } => {
            parts.attribute("borrow_kind", borrow_kind(*kind));
            parts.place("place", place, context)?;

            "borrow"
        }
        MirOperationKind::Unary { operator, operand } => {
            parts.attribute("operator", unary_operator(*operator));
            parts.operand("operand", operand, context)?;

            "unary"
        }
        MirOperationKind::Binary {
            operator,
            left,
            right,
        } => {
            parts.attribute("operator", binary_operator(*operator));
            parts.operand("left", left, context)?;
            parts.operand("right", right, context)?;

            "binary"
        }
        MirOperationKind::Aggregate(aggregate) => {
            parts.attribute("aggregate_kind", aggregate_kind(aggregate.kind()));

            for (index, operand) in aggregate.operands().iter().enumerate() {
                parts.operand(format!("element[{index}]"), operand, context)?;
            }

            "aggregate"
        }
        MirOperationKind::Construct(construction) => {
            construction_target(construction.target(), parts, context.symbols);

            for input in construction.inputs() {
                match input {
                    MirConstructionInput::Explicit {
                        input,
                        ordinal,
                        value,
                    } => {
                        parts.attribute("input", format!("{}:{ordinal}", construction_input(*input)));
                        parts.operand(format!("input[{ordinal}]"), value, context)?;
                    }
                    MirConstructionInput::Default { input, ordinal, .. } => {
                        parts.attribute(
                            "default",
                            format!("{}:{ordinal}", construction_input(*input)),
                        );
                    }
                }
            }

            "construct"
        }
        MirOperationKind::Convert {
            operand,
            conversion,
        } => {
            parts.attribute("conversion", conversion_kind(conversion.target()));
            parts.operand("operand", operand, context)?;

            "convert"
        }
        MirOperationKind::PatternProjection {
            subject,
            projection,
            operation,
        } => {
            parts.attribute("projection", pattern_projection(*projection));
            parts.attribute("operation", pattern_operation(*operation));
            parts.operand("subject", subject, context)?;

            "pattern_projection"
        }
        MirOperationKind::Generator(operation) => {
            generator_operation(operation, parts, context)?;

            "generator"
        }
        MirOperationKind::Call(call) => {
            match call.target() {
                MirCallTarget::Direct(reference) => {
                    parts.symbol(
                        "callee",
                        reference.instance().definition().symbol(),
                        context.symbols,
                    );

                    parts.attribute("dispatch", "direct");
                }
                MirCallTarget::Indirect { callee, .. } => {
                    parts.attribute("dispatch", "indirect");
                    parts.operand("callee", callee, context)?;
                }
            }

            for argument in call.arguments() {
                match argument {
                    MirCallArgument::Receiver { value, .. } => {
                        parts.operand("receiver", value, context)?;
                    }
                    MirCallArgument::Explicit { ordinal, value, .. } => {
                        parts.operand(format!("argument[{ordinal}]"), value, context)?;
                    }
                    MirCallArgument::Default { ordinal, .. } => {
                        parts.attribute("default_argument", *ordinal);
                    }
                }
            }

            "call"
        }
        MirOperationKind::PanicReport(cause) => {
            match cause {
                MirPanicCause::Message(message) => parts.operand("message", message, context)?,
                MirPanicCause::Assertion(message) => {
                    parts.attribute("cause", "assertion");

                    if let Some(message) = message {
                        parts.operand("message", message, context)?;
                    }
                }
            }

            "panic_report"
        }
        MirOperationKind::Finalize(place) => {
            parts.place("place", place, context)?;

            "finalize"
        }
        MirOperationKind::Destroy(place) => {
            parts.place("place", place, context)?;

            "destroy"
        }
        MirOperationKind::Cleanup { phase, place } => {
            parts.attribute("phase", cleanup_phase(*phase));
            parts.place("place", place, context)?;

            "cleanup"
        }
        MirOperationKind::Async(operation) => async_operation(operation, parts, context)?,
        MirOperationKind::Host(operation) => host_operation(operation, parts),
    };

    Ok(kind)
}

fn generator_operation(
    operation: &MirGeneratorOperation,
    parts: &mut OperationParts,
    context: &MirInspectionContext<'_>,
) -> Result<(), MirInspectionModelError> {
    match operation {
        MirGeneratorOperation::Begin {
            kind,
            destination,
            exact_count,
        } => {
            parts.attribute("step", "begin");
            parts.attribute("generator_kind", generator_kind(*kind));

            parts.attribute("has_exact_count", exact_count.is_some());

            parts.place("destination", destination, context)?;
        }
        MirGeneratorOperation::Push { destination, value } => {
            parts.attribute("step", "push");
            parts.place("destination", destination, context)?;
            parts.operand("value", value, context)?;
        }
        MirGeneratorOperation::Finish { destination } => {
            parts.attribute("step", "finish");
            parts.place("destination", destination, context)?;
        }
    }

    Ok(())
}

fn async_operation(
    operation: &MirAsyncOperation,
    parts: &mut OperationParts,
    context: &MirInspectionContext<'_>,
) -> Result<&'static str, MirInspectionModelError> {
    let kind = match operation {
        MirAsyncOperation::CreateFrame { .. } => "create_frame",
        MirAsyncOperation::MoveInactiveFrame {
            source,
            destination,
            ..
        } => {
            parts.place("source", source, context)?;
            parts.place("destination", destination, context)?;

            "move_inactive_frame"
        }
        MirAsyncOperation::ResumeFrame {
            state,
            storage,
            runtime,
            ..
        } => {
            parts.attribute("state", state.raw());
            parts.attribute("storage", storage.slot());
            parts.attribute("runtime", runtime.role().as_str());

            "resume_frame"
        }
        MirAsyncOperation::ComposeAwaitedFrame { frame, .. } => {
            parts.operand("frame", frame, context)?;

            "compose_awaited_frame"
        }
        MirAsyncOperation::CommitAwaitedCompletion { .. } => "commit_awaited_completion",
        MirAsyncOperation::StartTask {
            value,
            allocation,
            start,
            ..
        } => {
            parts.attribute("allocation_runtime", allocation.role().as_str());
            parts.attribute("start_runtime", start.role().as_str());
            parts.operand("frame", value, context)?;

            "start_task"
        }
        MirAsyncOperation::RequestTaskCancellation { task, runtime } => {
            parts.attribute("runtime", runtime.role().as_str());
            parts.operand("task", task, context)?;

            "request_task_cancellation"
        }
        MirAsyncOperation::ObserveCurrentRunCancellation { runtime } => {
            parts.attribute("runtime", runtime.role().as_str());

            "observe_current_run_cancellation"
        }
        MirAsyncOperation::ResolveTask { task, runtime } => {
            parts.attribute("runtime", runtime.role().as_str());
            parts.operand("task", task, context)?;

            "resolve_task"
        }
        MirAsyncOperation::PublishTerminalState { state, runtime } => {
            parts.attribute("runtime", runtime.role().as_str());

            match state {
                MirTaskTerminalState::Completed(value) => {
                    parts.attribute("state", "completed");
                    parts.operand("value", value, context)?;
                }
                MirTaskTerminalState::Cancelled => parts.attribute("state", "cancelled"),
                MirTaskTerminalState::Panicked(report) => {
                    parts.attribute("state", "panicked");
                    parts.operand("report", report, context)?;
                }
            }

            "publish_terminal_state"
        }
        MirAsyncOperation::ExecuteCleanupBroadcast { runtime, .. } => {
            parts.attribute("runtime", runtime.role().as_str());

            "execute_cleanup_broadcast"
        }
        MirAsyncOperation::ExecuteLifecycleResolution { runtime, .. } => {
            parts.attribute("runtime", runtime.role().as_str());

            "execute_lifecycle_resolution"
        }
        MirAsyncOperation::TransferCleanupIncident { incident, runtime } => {
            parts.attribute("runtime", runtime.role().as_str());
            parts.operand("incident", incident, context)?;

            "transfer_cleanup_incident"
        }
        MirAsyncOperation::DestroyTerminalTask { task } => {
            parts.operand("task", task, context)?;

            "destroy_terminal_task"
        }
    };

    Ok(kind)
}

fn host_operation(
    operation: &MirHostOperation,
    parts: &mut OperationParts,
) -> &'static str {
    match operation {
        MirHostOperation::ExecuteRoot { root, runtime, .. } => {
            parts.attribute("root_kind", root.kind().as_str());
            parts.attribute("runtime", runtime.role().as_str());

            "execute_root"
        }
        MirHostOperation::RequestRootCancellation { runtime } => {
            parts.attribute("runtime", runtime.role().as_str());

            "request_root_cancellation"
        }
        MirHostOperation::ObserveRootTerminal { runtime } => {
            parts.attribute("runtime", runtime.role().as_str());

            "observe_root_terminal"
        }
        MirHostOperation::ReportCleanupIncidents { runtime } => {
            parts.attribute("runtime", runtime.role().as_str());

            "report_cleanup_incidents"
        }
        MirHostOperation::StructuredShutdown { runtime } => {
            parts.attribute("runtime", runtime.role().as_str());

            "structured_shutdown"
        }
    }
}

fn inspection_terminator(
    terminator: &MirTerminatorKind,
    source: &MirSourceAnchor,
    symbols: &SymbolGraph,
    semantic_values: &SemanticValueStore,
    sources: &InspectionSources<'_>,
) -> Result<InspectionMirTerminator, MirInspectionModelError> {
    let context = MirInspectionContext {
        symbols,
        semantic_values,
    };

    let mut parts = TerminatorParts::new();

    let terminator_kind = match terminator {
        MirTerminatorKind::Goto(edge) => {
            parts.edge("target", edge, None, &context)?;

            "goto"
        }
        MirTerminatorKind::Branch {
            condition,
            then_edge,
            else_edge,
        } => {
            parts.operand("condition", condition, &context)?;
            parts.edge("then", then_edge, None, &context)?;
            parts.edge("else", else_edge, None, &context)?;

            "branch"
        }
        MirTerminatorKind::PatternBranch {
            subject,
            matched,
            unmatched,
            ..
        } => {
            parts.operand("subject", subject, &context)?;
            parts.edge("matched", matched, None, &context)?;
            parts.edge("unmatched", unmatched, None, &context)?;

            "pattern_branch"
        }
        MirTerminatorKind::Iterate {
            cursor,
            item,
            exhausted,
            ..
        } => {
            parts.place("cursor", cursor, &context)?;

            parts.edges.push(InspectionMirEdge {
                role: String::from("item"),
                target: item.slot(),
                arguments: Vec::new(),
                cleanup_phase: None,
            });

            parts.edge("exhausted", exhausted, None, &context)?;

            "iterate"
        }
        MirTerminatorKind::Switch {
            discriminant,
            cases,
            otherwise,
        } => {
            parts.operand("discriminant", discriminant, &context)?;

            for (index, case) in cases.iter().enumerate() {
                parts.edge(format!("case[{index}]"), case.edge(), None, &context)?;
            }

            parts.edge("otherwise", otherwise, None, &context)?;

            "switch"
        }
        MirTerminatorKind::Return(value) => {
            if let Some(value) = value {
                parts.operand("value", value, &context)?;
            }

            "return"
        }
        MirTerminatorKind::Unreachable => "unreachable",
        MirTerminatorKind::Suspend {
            resume_state,
            resume,
            cancellation,
            registration,
            wake,
        } => {
            parts.attribute("resume_state", resume_state.raw());
            parts.attribute("registration_runtime", registration.role().as_str());
            parts.attribute("wake_runtime", wake.role().as_str());
            parts.edge("resume", resume, None, &context)?;
            parts.cleanup_edge("cancellation", cancellation, &context)?;

            "suspend"
        }
        MirTerminatorKind::ForwardRunResult { result, edges } => {
            parts.operand("result", result, &context)?;
            parts.edge("completed", edges.completed(), None, &context)?;
            parts.cleanup_edge("panicked", edges.panicked(), &context)?;
            parts.cleanup_edge("cancelled", edges.cancelled(), &context)?;

            "forward_run_result"
        }
        MirTerminatorKind::BeginCleanup(edge) => {
            parts.cleanup_edge("cleanup", edge, &context)?;

            "begin_cleanup"
        }
        MirTerminatorKind::ContinueCleanup(edge) => {
            parts.cleanup_edge("cleanup", edge, &context)?;

            "continue_cleanup"
        }
        MirTerminatorKind::Panic { report, cleanup } => {
            parts.operand("report", report, &context)?;
            parts.cleanup_edge("cleanup", cleanup, &context)?;

            "panic"
        }
        MirTerminatorKind::CancelCurrentRun { cleanup } => {
            parts.cleanup_edge("cleanup", cleanup, &context)?;

            "cancel_current_run"
        }
    };

    Ok(InspectionMirTerminator {
        terminator_kind,
        source: inspection_source_anchor(source, sources)?,
        operands: parts.operands,
        places: parts.places,
        edges: parts.edges,
        attributes: parts.attributes,
    })
}

struct TerminatorParts {
    operands: Vec<InspectionMirNamedOperand>,
    places: Vec<InspectionMirNamedPlace>,
    edges: Vec<InspectionMirEdge>,
    attributes: Vec<InspectionMirAttribute>,
}

impl TerminatorParts {
    fn new() -> Self {
        Self {
            operands: Vec::new(),
            places: Vec::new(),
            edges: Vec::new(),
            attributes: Vec::new(),
        }
    }

    fn operand(
        &mut self,
        role: impl Into<String>,
        operand: &MirOperand,
        context: &MirInspectionContext<'_>,
    ) -> Result<(), MirInspectionModelError> {
        self.operands.push(InspectionMirNamedOperand {
            role: role.into(),
            operand: inspection_operand(operand, context)?,
        });

        Ok(())
    }

    fn place(
        &mut self,
        role: impl Into<String>,
        place: &MirPlace,
        context: &MirInspectionContext<'_>,
    ) -> Result<(), MirInspectionModelError> {
        self.places.push(InspectionMirNamedPlace {
            role: role.into(),
            place: inspection_place(place, context)?,
        });

        Ok(())
    }

    fn edge(
        &mut self,
        role: impl Into<String>,
        edge: &MirEdge,
        cleanup_phase: Option<&'static str>,
        context: &MirInspectionContext<'_>,
    ) -> Result<(), MirInspectionModelError> {
        self.edges.push(InspectionMirEdge {
            role: role.into(),
            target: edge.target().slot(),
            arguments: edge
                .arguments()
                .iter()
                .map(|argument| inspection_operand(argument, context))
                .collect::<Result<_, _>>()?,
            cleanup_phase,
        });

        Ok(())
    }

    fn cleanup_edge(
        &mut self,
        role: impl Into<String>,
        edge: &MirCleanupEdge,
        context: &MirInspectionContext<'_>,
    ) -> Result<(), MirInspectionModelError> {
        self.edge(
            role,
            edge.edge(),
            Some(cleanup_phase(edge.phase())),
            context,
        )
    }

    fn attribute(
        &mut self,
        name: &'static str,
        value: impl Into<InspectionMirAttributeValue>,
    ) {
        self.attributes.push(InspectionMirAttribute {
            name,
            value: value.into(),
        });
    }
}

fn inspection_operand(
    operand: &MirOperand,
    context: &MirInspectionContext<'_>,
) -> Result<InspectionMirOperand, MirInspectionModelError> {
    match operand {
        MirOperand::Value(value) => Ok(InspectionMirOperand::Value {
            value: value.slot(),
        }),
        MirOperand::Constant { value, ty } => Ok(InspectionMirOperand::Constant {
            value: constant_text(*value, context.semantic_values),
            r#type: InspectionType::from_type(context.semantic_values, context.symbols, *ty)?,
        }),
        MirOperand::Immediate { value, ty } => Ok(InspectionMirOperand::Immediate {
            value: immediate_text(*value).to_owned(),
            r#type: InspectionType::from_type(context.semantic_values, context.symbols, *ty)?,
        }),
        MirOperand::Copy(place) => Ok(InspectionMirOperand::Copy {
            place: inspection_place(place, context)?,
        }),
        MirOperand::Move(place) => Ok(InspectionMirOperand::Move {
            place: inspection_place(place, context)?,
        }),
    }
}

fn inspection_place(
    place: &MirPlace,
    context: &MirInspectionContext<'_>,
) -> Result<InspectionMirPlace, MirInspectionModelError> {
    let projections = place
        .projections()
        .iter()
        .map(|projection| {
            let (projection_kind, detail) = projection_kind(projection.kind(), context.symbols);

            Ok(InspectionMirProjection {
                projection_kind,
                detail,
                source_type: InspectionType::from_type(
                    context.semantic_values,
                    context.symbols,
                    projection.source_type(),
                )?,
                result_type: InspectionType::from_type(
                    context.semantic_values,
                    context.symbols,
                    projection.result_type(),
                )?,
            })
        })
        .collect::<Result<_, MirInspectionModelError>>()?;

    Ok(InspectionMirPlace {
        storage: place.storage().slot(),
        r#type: InspectionType::from_type(
            context.semantic_values,
            context.symbols,
            place.ty(),
        )?,
        projections,
    })
}

fn inspection_unit_key(
    key: &MirUnitKey,
    symbols: &SymbolGraph,
    sources: &InspectionSources<'_>,
) -> Result<InspectionMirUnitKey, MirInspectionModelError> {
    match key {
        MirUnitKey::Bound(key) => {
            let owner = symbols
                .symbol_for_key(key.declared_owner())
                .ok_or(MirInspectionModelError::MissingSymbol)?;

            Ok(InspectionMirUnitKey::Bound {
                unit_kind: key.kind().as_str(),
                owner: InspectionSymbolIdentity::from_symbol(symbols, owner),
                source: InspectionSyntaxAnchor::from_anchor(sources, key.source().syntax())?,
            })
        }
        MirUnitKey::ExecutableHost(product) => Ok(InspectionMirUnitKey::ExecutableHost {
            package: product.package().as_str().to_owned(),
            product: product.name().to_owned(),
        }),
    }
}

fn inspection_source_origin(
    source: &MirSourceOrigin,
    sources: &InspectionSources<'_>,
) -> Result<InspectionMirSource, MirInspectionModelError> {
    match source {
        MirSourceOrigin::Source(source) => Ok(InspectionMirSource::Source {
            syntax: InspectionSyntaxAnchor::from_anchor(sources, source.syntax())?,
            synthesis: None,
        }),
        MirSourceOrigin::ExecutableHost(product) => Ok(InspectionMirSource::ExecutableHost {
            package: product.package().as_str().to_owned(),
            product: product.name().to_owned(),
        }),
    }
}

fn inspection_source_anchor(
    source: &MirSourceAnchor,
    sources: &InspectionSources<'_>,
) -> Result<InspectionMirSource, MirInspectionModelError> {
    match source {
        MirSourceAnchor::Source(origin) => Ok(InspectionMirSource::Source {
            syntax: InspectionSyntaxAnchor::from_anchor(
                sources,
                origin.source_anchor().syntax(),
            )?,
            synthesis: origin
                .synthesized_origin()
                .map(|synthesis| InspectionMirSynthesis {
                    role: synthesis.role().as_str(),
                    ordinal: synthesis.ordinal().raw(),
                }),
        }),
        MirSourceAnchor::ExecutableHost(product) => Ok(InspectionMirSource::ExecutableHost {
            package: product.package().as_str().to_owned(),
            product: product.name().to_owned(),
        }),
    }
}

fn constant_text(
    value: bray_symbols::ConstantValueId,
    semantic_values: &SemanticValueStore,
) -> String {
    use bray_symbols::ConstantValueKind;

    let Ok(value) = semantic_values.constant_value_data(value) else {
        return String::from("<invalid-constant>");
    };

    match value.kind() {
        ConstantValueKind::Error => String::from("<error>"),
        ConstantValueKind::Boolean(value) => value.to_string(),
        ConstantValueKind::Character(value) => format!("'{}'", value.escape_default()),
        ConstantValueKind::Integer(value) => integer_text(value),
        ConstantValueKind::Real(value) => real_bits_text(*value),
        ConstantValueKind::Complex { real, imaginary } => {
            format!("{}+{}i", real_bits_text(*real), real_bits_text(*imaginary))
        }
        ConstantValueKind::String(value) => format!("\"{}\"", value.escape_debug()),
        ConstantValueKind::Unit => String::from("()"),
        ConstantValueKind::NullableAbsent => String::from("none"),
        ConstantValueKind::NullablePresent(_) => String::from("some(<constant>)"),
        ConstantValueKind::Tuple(values) => format!("<tuple:{}>", values.len()),
        ConstantValueKind::Array(values) => format!("<array:{}>", values.len()),
        ConstantValueKind::Product(fields) => format!("<product:{}>", fields.len()),
        ConstantValueKind::Union { fields, .. } => format!("<union:{}>", fields.len()),
    }
}

fn integer_text(value: &bray_symbols::IntegerConstant) -> String {
    use bray_symbols::IntegerSign;

    if let Some(value) = value.to_u64() {
        return value.to_string();
    }

    let mut text = match value.sign() {
        IntegerSign::Negative => String::from("-0x"),
        IntegerSign::NonNegative => String::from("0x"),
    };

    for byte in value.magnitude() {
        let _ = write!(text, "{byte:02x}");
    }

    text
}

fn real_bits_text(value: bray_symbols::RealConstantBits) -> String {
    match value {
        bray_symbols::RealConstantBits::Binary16(bits) => format!("f16:0x{bits:04x}"),
        bray_symbols::RealConstantBits::Binary32(bits) => format!("f32:0x{bits:08x}"),
        bray_symbols::RealConstantBits::Binary64(bits) => format!("f64:0x{bits:016x}"),
        bray_symbols::RealConstantBits::Binary128(bits) => {
            let mut text = String::from("f128:0x");

            for byte in bits {
                let _ = write!(text, "{byte:02x}");
            }

            text
        }
    }
}

fn projection_kind(
    projection: &MirProjectionKind,
    symbols: &SymbolGraph,
) -> (&'static str, Option<String>) {
    match projection {
        MirProjectionKind::Dereference => ("dereference", None),
        MirProjectionKind::Field(field) => {
            let symbol = match field {
                MirFieldReference::Struct(field) => (*field).into(),
                MirFieldReference::UnionPayload(field) => (*field).into(),
            };

            (
                "field",
                Some(
                    InspectionSymbolIdentity::from_symbol(symbols, symbol)
                        .display_name()
                        .into_owned(),
                ),
            )
        }
        MirProjectionKind::TupleField(ordinal) => ("tuple_field", Some(ordinal.to_string())),
        MirProjectionKind::ElementFromStart(ordinal) => {
            ("element_from_start", Some(ordinal.to_string()))
        }
        MirProjectionKind::ElementFromEnd(ordinal) => {
            ("element_from_end", Some(ordinal.to_string()))
        }
        MirProjectionKind::Index(_) => ("index", None),
        MirProjectionKind::Slice { .. } => ("slice", None),
        MirProjectionKind::Variant(variant) => (
            "variant",
            Some(
                InspectionSymbolIdentity::from_symbol(symbols, (*variant).into())
                    .display_name()
                    .into_owned(),
            ),
        ),
        MirProjectionKind::ActiveUnionPayloadField { field, .. } => (
            "active_union_payload_field",
            Some(
                InspectionSymbolIdentity::from_symbol(symbols, (*field).into())
                    .display_name()
                    .into_owned(),
            ),
        ),
        MirProjectionKind::NullableValue => ("nullable_value", None),
    }
}

fn construction_target(
    target: ConstructionTarget,
    parts: &mut OperationParts,
    symbols: &SymbolGraph,
) {
    match target {
        ConstructionTarget::Struct(symbol) => {
            parts.attribute("target_kind", "struct");
            parts.symbol("target", symbol.into(), symbols);
        }
        ConstructionTarget::UnionVariant(symbol) => {
            parts.attribute("target_kind", "union_variant");
            parts.symbol("target", symbol.into(), symbols);
        }
        ConstructionTarget::TypeForm { callable, .. } => {
            parts.attribute("target_kind", "type_form");
            parts.symbol("target", callable.definition().symbol(), symbols);
        }
    }
}

fn construction_input(input: ConstructionInputId) -> &'static str {
    match input {
        ConstructionInputId::StructField(_) => "struct_field",
        ConstructionInputId::UnionPayloadField(_) => "union_payload_field",
        ConstructionInputId::CallableParameter(_) => "callable_parameter",
    }
}

fn mir_unit_kind(kind: &MirUnitKind) -> &'static str {
    match kind {
        MirUnitKind::Synchronous => "synchronous",
        MirUnitKind::ProtectedAsyncFrame(_) => "protected_async_frame",
        MirUnitKind::ExecutableHost(_) => "executable_host",
    }
}

fn storage_kind(kind: MirStorageKind) -> &'static str {
    match kind {
        MirStorageKind::Parameter => "parameter",
        MirStorageKind::Local => "local",
        MirStorageKind::Temporary => "temporary",
        MirStorageKind::Return => "return",
        MirStorageKind::InactiveFrame => "inactive_frame",
        MirStorageKind::CurrentFrame => "current_frame",
        MirStorageKind::CurrentTask => "current_task",
        MirStorageKind::ChildTask => "child_task",
    }
}

fn block_kind(kind: MirBlockKind) -> &'static str {
    match kind {
        MirBlockKind::Ordinary => "ordinary",
        MirBlockKind::CleanupBroadcast => "cleanup_broadcast",
        MirBlockKind::LifecycleResolution => "lifecycle_resolution",
    }
}

fn cleanup_phase(phase: MirCleanupPhase) -> &'static str {
    match phase {
        MirCleanupPhase::TaskCancellation => "task_cancellation",
        MirCleanupPhase::LifecycleResolution => "lifecycle_resolution",
    }
}

fn store_kind(kind: MirStoreKind) -> &'static str {
    match kind {
        MirStoreKind::Initialize => "initialize",
        MirStoreKind::Assign => "assign",
    }
}

fn borrow_kind(kind: BorrowKind) -> &'static str {
    match kind {
        BorrowKind::Shared => "shared",
        BorrowKind::Mutable => "mutable",
    }
}

fn unary_operator(operator: MirUnaryOperator) -> &'static str {
    match operator {
        MirUnaryOperator::Negate => "negate",
        MirUnaryOperator::Not => "not",
        MirUnaryOperator::BitwiseNot => "bitwise_not",
    }
}

fn binary_operator(operator: MirBinaryOperator) -> &'static str {
    match operator {
        MirBinaryOperator::Add => "add",
        MirBinaryOperator::Subtract => "subtract",
        MirBinaryOperator::Multiply => "multiply",
        MirBinaryOperator::Divide => "divide",
        MirBinaryOperator::Remainder => "remainder",
        MirBinaryOperator::Equal => "equal",
        MirBinaryOperator::NotEqual => "not_equal",
        MirBinaryOperator::LessThan => "less_than",
        MirBinaryOperator::LessThanOrEqual => "less_than_or_equal",
        MirBinaryOperator::GreaterThan => "greater_than",
        MirBinaryOperator::GreaterThanOrEqual => "greater_than_or_equal",
        MirBinaryOperator::BitwiseAnd => "bitwise_and",
        MirBinaryOperator::BitwiseOr => "bitwise_or",
        MirBinaryOperator::BitwiseXor => "bitwise_xor",
        MirBinaryOperator::ShiftLeft => "shift_left",
        MirBinaryOperator::ShiftRight => "shift_right",
    }
}

fn aggregate_kind(kind: MirAggregateKind) -> &'static str {
    match kind {
        MirAggregateKind::Tuple => "tuple",
        MirAggregateKind::Array => "array",
        MirAggregateKind::RepeatedArray => "repeated_array",
    }
}

fn generator_kind(kind: MirGeneratorKind) -> &'static str {
    match kind {
        MirGeneratorKind::Array => "array",
        MirGeneratorKind::General => "general",
    }
}

fn conversion_kind(target: &ConversionTarget) -> &'static str {
    match target {
        ConversionTarget::Identity => "identity",
        ConversionTarget::BuiltInScalar => "built_in_scalar",
        ConversionTarget::Composite(_) => "composite",
        ConversionTarget::Trait { .. } => "trait",
    }
}

fn pattern_operation(operation: PatternOperation) -> &'static str {
    match operation {
        PatternOperation::Observe => "observe",
        PatternOperation::SharedBorrow => "shared_borrow",
        PatternOperation::MutableBorrow => "mutable_borrow",
        PatternOperation::Consume => "consume",
        PatternOperation::Copy => "copy",
        PatternOperation::Recovered => "recovered",
    }
}

fn pattern_projection(projection: PatternProjection) -> &'static str {
    match projection {
        PatternProjection::ProductField(_) => "product_field",
        PatternProjection::TupleElement(_) => "tuple_element",
        PatternProjection::ActiveUnionPayloadField { .. } => "active_union_payload_field",
        PatternProjection::ElementFromStart(_) => "element_from_start",
        PatternProjection::ElementFromEnd(_) => "element_from_end",
        PatternProjection::NullableValue => "nullable_value",
        PatternProjection::OwnedTarget => "owned_target",
    }
}

fn immediate_text(value: MirImmediateValue) -> &'static str {
    match value {
        MirImmediateValue::Boolean(true) => "true",
        MirImmediateValue::Boolean(false) => "false",
        MirImmediateValue::Unit => "()",
        MirImmediateValue::NullableAbsent => "none",
    }
}

fn format_abi(major: u16, minor: u16) -> String {
    format!("{major}.{minor}")
}

fn digest_text(digest: [u8; 32]) -> String {
    let mut text = String::with_capacity(64);

    for byte in digest {
        let _ = write!(text, "{byte:02x}");
    }

    text
}

fn compact_id(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}
