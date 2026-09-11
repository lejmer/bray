// rust-style: allow(module-too-large, reason = "the MIR inspection DTOs and exhaustive projection form one cohesive serialization contract")

use bray_symbols::CallableConditions;

use std::fmt::Write;

use bray_bound_tree::{
    BoundCallResult, CheckedMemoryOperationKind, ConstructionInputId, ConstructionTarget,
    ConversionTarget, MemoryLayoutQueryKind, PatternOperation, PatternProjection,
    SelectedConversion,
};
use bray_ir::{
    MirAggregateKind, MirAsyncOperation, MirBinaryOperator, MirBlockKind, MirCallArgument,
    MirCallTarget, MirCallableReference, MirCleanupEdge, MirCleanupPhase, MirEdge,
    MirFieldReference, MirFrameInitializer, MirFrameReference, MirGeneratorKind,
    MirGeneratorOperation, MirHelperReference, MirHostOperation, MirImmediateValue, MirOperand,
    MirOperation, MirOperationKind, MirPanicCause, MirPatternPredicate, MirPlace,
    MirProjectionKind, MirRuntimeReference, MirSourceAnchor, MirSourceOrigin, MirStorageKind,
    MirStoreKind, MirTaskTerminalState, MirTerminatorKind, MirUnaryOperator, MirUnit, MirUnitKey,
    MirUnitKind, MirValueOrigin,
};
use bray_symbols::{AnySymbolId, BorrowKind, CallableAbi, SemanticValueStore, SymbolGraph, TypeId};
use serde::Serialize;

use crate::inspection::{
    InspectionSourceError, InspectionSources, InspectionSymbolIdentity, InspectionSyntaxAnchor,
    InspectionType, TypeInspectionError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MirInspectionModelError {
    InvalidGeneratedLifecycle,
    MissingOperation,
    MissingSymbol,
    Source(InspectionSourceError),
    Type(TypeInspectionError),
}

impl From<InspectionSourceError> for MirInspectionModelError {
    fn from(error: InspectionSourceError) -> Self {
        Self::Source(error)
    }
}

impl From<TypeInspectionError> for MirInspectionModelError {
    fn from(error: TypeInspectionError) -> Self {
        Self::Type(error)
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
        let source = inspection_source_origin(mir.source(), symbols, sources)?;
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
                    r#type: InspectionType::from_type(semantic_values, symbols, storage.ty())?,
                    source: inspection_source_anchor(storage.source(), symbols, sources)?,
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
                    source: inspection_source_anchor(value.source(), symbols, sources)?,
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
                    source: inspection_source_anchor(block.source(), symbols, sources)?,
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
                        execution: inspection_frame_execution(state.execution()),
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
                    inactive_cleanup: frame.inactive_cleanup().map(bray_ir::MirBlockId::slot),
                    capture_quiescence: frame.capture_abandonment().map(|(entry, _)| entry.slot()),
                    capture_destruction: frame.capture_abandonment().map(|(_, entry)| entry.slot()),
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
    CompilerProvidedCallable {
        callable: InspectionSymbolIdentity,
    },
    Bound {
        unit_kind: &'static str,
        owner: InspectionSymbolIdentity,
        source: InspectionSyntaxAnchor,
    },
    ExecutableHost {
        package: String,
        product: String,
    },
    GeneratedLifecycle {
        role: &'static str,
        type_identity: String,
    },
    ImportedExecutable {
        owner: InspectionSymbolIdentity,
        template: u32,
    },
    ExternalCallable {
        callable: InspectionSymbolIdentity,
    },
    ExternalRuntimeDefault {
        provider: InspectionSymbolIdentity,
    },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum InspectionMirSource {
    CompilerProvidedCallable {
        callable: InspectionSymbolIdentity,
    },
    Source {
        syntax: InspectionSyntaxAnchor,
        synthesis: Option<InspectionMirSynthesis>,
    },
    ExecutableHost {
        package: String,
        product: String,
    },
    GeneratedLifecycle {
        role: &'static str,
    },
    ImportedExecutable {
        owner: InspectionSymbolIdentity,
        template: u32,
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
    pub(crate) inactive_cleanup: Option<u32>,
    pub(crate) capture_quiescence: Option<u32>,
    pub(crate) capture_destruction: Option<u32>,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirFrameState {
    pub(crate) id: u32,
    pub(crate) entry: u32,
    pub(crate) execution: InspectionMirFrameExecution,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirFrameExecution {
    pub(crate) affinity: u32,
    pub(crate) lane_requirements: Vec<&'static str>,
    pub(crate) retained_storages: Vec<u32>,
}

fn inspection_frame_execution(
    execution: &bray_ir::MirFrameExecutionState,
) -> InspectionMirFrameExecution {
    InspectionMirFrameExecution {
        affinity: execution.affinity().code(),
        lane_requirements: execution
            .lane_requirements()
            .iter()
            .map(|lane| match lane {
                bray_runtime_interface::ExecutionLaneRequirement::Blocking => "blocking",
                bray_runtime_interface::ExecutionLaneRequirement::Compute => "compute",
                bray_runtime_interface::ExecutionLaneRequirement::MainThread => "main_thread",
            })
            .collect(),
        retained_storages: execution
            .retained_storages()
            .iter()
            .map(|storage| storage.slot())
            .collect(),
    }
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
    pub(crate) cleanup_execution: Option<InspectionMirFrameExecution>,
    pub(crate) id: u32,
    pub(crate) operation_kind: &'static str,
    pub(crate) result: Option<u32>,
    pub(crate) source: InspectionMirSource,
    pub(crate) attributes: Vec<InspectionMirAttribute>,
    pub(crate) operands: Vec<InspectionMirNamedOperand>,
    pub(crate) places: Vec<InspectionMirNamedPlace>,
    pub(crate) symbols: Vec<InspectionMirNamedSymbol>,
    pub(crate) types: Vec<InspectionMirNamedType>,
    pub(crate) semantic_values: Vec<InspectionMirSemanticValue>,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirAttribute {
    pub(crate) name: String,
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
pub(crate) struct InspectionMirNamedType {
    pub(crate) role: String,
    pub(crate) r#type: InspectionType,
}

#[derive(Serialize)]
pub(crate) struct InspectionMirSemanticValue {
    pub(crate) role: String,
    pub(crate) value_kind: &'static str,
    pub(crate) id: u32,
    pub(crate) text: Option<String>,
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
    pub(crate) symbols: Vec<InspectionMirNamedSymbol>,
    pub(crate) types: Vec<InspectionMirNamedType>,
    pub(crate) semantic_values: Vec<InspectionMirSemanticValue>,
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
    types: Vec<InspectionMirNamedType>,
    semantic_values: Vec<InspectionMirSemanticValue>,
}

impl OperationParts {
    fn new() -> Self {
        Self {
            attributes: Vec::new(),
            operands: Vec::new(),
            places: Vec::new(),
            symbols: Vec::new(),
            types: Vec::new(),
            semantic_values: Vec::new(),
        }
    }

    fn attribute(
        &mut self,
        name: impl Into<String>,
        value: impl Into<InspectionMirAttributeValue>,
    ) {
        self.attributes.push(InspectionMirAttribute {
            name: name.into(),
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

    fn r#type(
        &mut self,
        role: impl Into<String>,
        ty: TypeId,
        context: &MirInspectionContext<'_>,
    ) -> Result<(), MirInspectionModelError> {
        self.types.push(InspectionMirNamedType {
            role: role.into(),
            r#type: InspectionType::from_type(context.semantic_values, context.symbols, ty)?,
        });

        Ok(())
    }

    fn semantic_value(
        &mut self,
        role: impl Into<String>,
        value_kind: &'static str,
        id: u32,
        text: Option<String>,
    ) {
        self.semantic_values.push(InspectionMirSemanticValue {
            role: role.into(),
            value_kind,
            id,
            text,
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
        cleanup_execution: operation
            .cleanup_execution()
            .map(inspection_frame_execution),
        id,
        operation_kind,
        result: operation.result().map(|result| result.slot()),
        source: inspection_source_anchor(operation.source(), symbols, sources)?,
        attributes: parts.attributes,
        operands: parts.operands,
        places: parts.places,
        symbols: parts.symbols,
        types: parts.types,
        semantic_values: parts.semantic_values,
    })
}

fn operation_parts(
    operation: &MirOperationKind,
    parts: &mut OperationParts,
    context: &MirInspectionContext<'_>,
) -> Result<&'static str, MirInspectionModelError> {
    let kind = match operation {
        MirOperationKind::AnonymousCallable(key) => {
            match key {
                bray_ir::MirAnonymousCallableReference::Bound(key) => {
                    parts.attribute("unit_kind", key.kind().as_str());
                }
                bray_ir::MirAnonymousCallableReference::Imported(key) => {
                    parts.symbol("owner", key.owner(), context.symbols);
                    parts.attribute("template", key.template().raw());
                }
            }

            "anonymous_callable"
        }
        MirOperationKind::DeclaredCallable(callable) => {
            callable_reference("callable", *callable, parts, context);

            "declared_callable"
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
            construction_target(construction.target(), parts, context)?;

            for input in construction.inputs() {
                let ordinal = input.ordinal();

                parts.attribute(
                    "input",
                    format!("{}:{ordinal}", construction_input(input.input())),
                );

                parts.symbol(
                    format!("input[{ordinal}]"),
                    construction_input_symbol(input.input()),
                    context.symbols,
                );

                parts.operand(format!("input[{ordinal}]"), input.value(), context)?;
            }

            "construct"
        }
        MirOperationKind::Convert {
            operand,
            conversion,
        } => {
            conversion_parts("conversion", conversion, parts, context)?;
            parts.operand("operand", operand, context)?;

            "convert"
        }
        MirOperationKind::NumericConversion { kind, operand } => {
            parts.attribute(
                "policy",
                match kind {
                    bray_ir::MirNumericConversionKind::Truncate => "truncate",
                },
            );

            parts.operand("operand", operand, context)?;

            "numeric_conversion"
        }
        MirOperationKind::NullableQuery(query) => {
            nullable_query_parts(query, parts, context)?;

            "nullable_query"
        }
        MirOperationKind::PatternProjection {
            subject,
            projection,
            operation,
        } => {
            pattern_projection_parts(*projection, parts, context.symbols);
            parts.attribute("operation", pattern_operation(*operation));
            parts.operand("subject", subject, context)?;

            "pattern_projection"
        }
        MirOperationKind::Generator(operation) => {
            generator_operation(operation, parts, context)?;

            "generator"
        }
        MirOperationKind::Call(call) => {
            call_parts(call, parts, context)?;

            "call"
        }
        MirOperationKind::Memory(memory) => {
            memory_operation_parts(memory, parts, context)?;

            "memory"
        }
        MirOperationKind::Text(text) => {
            text_operation_parts(text, parts, context)?;

            "text"
        }
        MirOperationKind::PanicReport(cause) => {
            match cause {
                MirPanicCause::TaskAdmission => parts.attribute("cause", "task_admission"),
                MirPanicCause::FrameAllocation => parts.attribute("cause", "frame_allocation"),
                MirPanicCause::Message(message) => parts.operand("message", message, context)?,
                MirPanicCause::Assertion(message) => {
                    parts.attribute("cause", "assertion");

                    if let Some(message) = message {
                        parts.operand("message", message, context)?;
                    }
                }
                MirPanicCause::ExplicitTestFailure(message) => {
                    parts.attribute("cause", "explicit_test_failure");
                    parts.operand("message", message, context)?;
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
        MirOperationKind::Abandon { action, place } => {
            parts.place("place", place, context)?;

            bray_ir::MirGeneratedLifecycleRole::Abandon(*action).as_str()
        }
        MirOperationKind::DestructorRemainder { role, place } => {
            parts.attribute("role", role.as_str());
            parts.place("place", place, context)?;

            "destructor_remainder"
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

fn text_operation_parts(
    text: &bray_ir::MirTextOperation,
    parts: &mut OperationParts,
    context: &MirInspectionContext<'_>,
) -> Result<(), MirInspectionModelError> {
    parts.attribute(
        "operation",
        match text.kind() {
            bray_ir::MirTextOperationKind::ScalarCount => "scalar_count",
            bray_ir::MirTextOperationKind::IsEmpty => "is_empty",
            bray_ir::MirTextOperationKind::Equals => "equals",
            bray_ir::MirTextOperationKind::ScalarAt => "scalar_at",
            bray_ir::MirTextOperationKind::ScalarSlice => "scalar_slice",
            bray_ir::MirTextOperationKind::Utf8 => "utf8",
            bray_ir::MirTextOperationKind::FromUtf8 => "from_utf8",
            bray_ir::MirTextOperationKind::CharacterScalarValue => "character_scalar_value",
            bray_ir::MirTextOperationKind::CharacterFromScalarValue => {
                "character_from_scalar_value"
            }
            bray_ir::MirTextOperationKind::CharacterUtf8Length => "character_utf8_length",
            bray_ir::MirTextOperationKind::CharacterUtf8Byte => "character_utf8_byte",
            bray_ir::MirTextOperationKind::CharacterIsAlphabetic => "character_is_alphabetic",
            bray_ir::MirTextOperationKind::CharacterIsNumeric => "character_is_numeric",
            bray_ir::MirTextOperationKind::CharacterIsWhitespace => "character_is_whitespace",
            bray_ir::MirTextOperationKind::Release => "release",
        },
    );

    for (index, operand) in text.operands().iter().enumerate() {
        parts.operand(format!("argument[{index}]"), operand, context)?;
    }

    for (index, ty) in text.operand_types().iter().copied().enumerate() {
        parts.r#type(format!("argument[{index}]"), ty, context)?;
    }

    if let Some(result) = text.result_type() {
        parts.r#type("result", result, context)?;
    }

    Ok(())
}

fn nullable_query_parts(
    query: &bray_ir::MirNullableQuery,
    parts: &mut OperationParts,
    context: &MirInspectionContext<'_>,
) -> Result<(), MirInspectionModelError> {
    parts.attribute(
        "query",
        match query.kind() {
            bray_ir::MirNullableQueryKind::IsPresent => "is_present",
            bray_ir::MirNullableQueryKind::IsAbsent => "is_absent",
        },
    );

    parts.operand("operand", query.operand(), context)?;
    parts.r#type("operand", query.operand_type(), context)?;
    parts.r#type("nullable", query.nullable_type(), context)?;

    parts.r#type("result", query.result_type(), context)
}

fn memory_operation_parts(
    memory: &bray_ir::MirMemoryOperation,
    parts: &mut OperationParts,
    context: &MirInspectionContext<'_>,
) -> Result<(), MirInspectionModelError> {
    match memory.kind() {
        CheckedMemoryOperationKind::AtomicLoad { order, .. }
        | CheckedMemoryOperationKind::AtomicStore { order, .. }
        | CheckedMemoryOperationKind::AtomicExchange { order, .. }
        | CheckedMemoryOperationKind::AtomicFetch { order, .. }
        | CheckedMemoryOperationKind::AtomicWait { order, .. } => {
            parts.attribute("memory_order", order.as_str());
        }
        CheckedMemoryOperationKind::AtomicCompareExchange {
            success, failure, ..
        } => {
            parts.attribute("success_order", success.as_str());
            parts.attribute("failure_order", failure.as_str());
        }
        _ => {}
    }

    let (name, types) = match memory.kind() {
        CheckedMemoryOperationKind::UninitNew { element } => {
            ("uninit_new", vec![("element", element)])
        }
        CheckedMemoryOperationKind::UninitPointer { element, .. } => {
            ("uninit_pointer", vec![("element", element)])
        }
        CheckedMemoryOperationKind::UninitWrite { element } => {
            ("uninit_write", vec![("element", element)])
        }
        CheckedMemoryOperationKind::UninitAssumeInitialized { element } => {
            ("uninit_assume_initialized", vec![("element", element)])
        }
        CheckedMemoryOperationKind::UninitMove { element } => {
            ("uninit_move", vec![("element", element)])
        }
        CheckedMemoryOperationKind::BorrowFrom { pointee, .. } => {
            ("borrow_from", vec![("pointee", pointee)])
        }
        CheckedMemoryOperationKind::Address { pointee, .. } => {
            ("address", vec![("pointee", pointee)])
        }
        CheckedMemoryOperationKind::Null { pointee } => ("null", vec![("pointee", pointee)]),
        CheckedMemoryOperationKind::IsNull { pointee } => ("is_null", vec![("pointee", pointee)]),
        CheckedMemoryOperationKind::Offset { pointee, .. } => {
            ("offset", vec![("pointee", pointee)])
        }
        CheckedMemoryOperationKind::Reinterpret { source, target } => {
            ("reinterpret", vec![("source", source), ("target", target)])
        }
        CheckedMemoryOperationKind::CallableFromPointer { callable } => {
            ("callable_from_pointer", vec![("callable", callable)])
        }
        CheckedMemoryOperationKind::PointerFromCallable { callable } => {
            ("pointer_from_callable", vec![("callable", callable)])
        }
        CheckedMemoryOperationKind::Read { pointee, .. } => ("read", vec![("pointee", pointee)]),
        CheckedMemoryOperationKind::Write { pointee } => ("write", vec![("pointee", pointee)]),
        CheckedMemoryOperationKind::Copy { pointee, .. } => ("copy", vec![("pointee", pointee)]),
        CheckedMemoryOperationKind::LayoutQuery { ty, kind } => {
            let name = match kind {
                MemoryLayoutQueryKind::Size => "size_of",
                MemoryLayoutQueryKind::Alignment => "align_of",
                MemoryLayoutQueryKind::Stride => "stride_of",
                MemoryLayoutQueryKind::Layout => "layout_of",
                MemoryLayoutQueryKind::Trailing => "trailing_layout_of",
            };

            (name, vec![("type", ty)])
        }
        CheckedMemoryOperationKind::RawAllocate | CheckedMemoryOperationKind::Allocate => {
            ("allocate", Vec::new())
        }
        CheckedMemoryOperationKind::RawDeallocate | CheckedMemoryOperationKind::Deallocate => {
            ("deallocate", Vec::new())
        }
        CheckedMemoryOperationKind::RawBufferCapacity => ("raw_buffer_capacity", Vec::new()),
        CheckedMemoryOperationKind::RawBufferInitializedCount => {
            ("raw_buffer_initialized_count", Vec::new())
        }
        CheckedMemoryOperationKind::RawBufferPointer => ("raw_buffer_pointer", Vec::new()),
        CheckedMemoryOperationKind::RawBufferInitializedSlice => {
            ("raw_buffer_initialized_slice", Vec::new())
        }
        CheckedMemoryOperationKind::RawBufferInitializedSliceMut => {
            ("raw_buffer_initialized_slice_mut", Vec::new())
        }
        CheckedMemoryOperationKind::RawBufferSparePointer { element } => {
            ("raw_buffer_spare_pointer", vec![("element", element)])
        }
        CheckedMemoryOperationKind::RawBufferSetInitializedCount => {
            ("raw_buffer_set_initialized_count", Vec::new())
        }
        CheckedMemoryOperationKind::RawBufferRelease { element } => {
            ("raw_buffer_release", vec![("element", element)])
        }
        CheckedMemoryOperationKind::RawBufferReplace { element } => {
            ("raw_buffer_replace", vec![("element", element)])
        }
        CheckedMemoryOperationKind::RawBufferRelocate { element } => {
            ("raw_buffer_relocate", vec![("element", element)])
        }
        CheckedMemoryOperationKind::ByteBufferFill => ("byte_buffer_fill", Vec::new()),
        CheckedMemoryOperationKind::ByteBufferCopy => ("byte_buffer_copy", Vec::new()),
        CheckedMemoryOperationKind::ByteBufferRead => ("byte_buffer_read", Vec::new()),
        CheckedMemoryOperationKind::SequenceLength => ("sequence_length", Vec::new()),
        CheckedMemoryOperationKind::CallbackState { state } => {
            ("callback_state", vec![("state", state)])
        }
        CheckedMemoryOperationKind::VolatileRead { pointee, .. } => {
            ("volatile_read", vec![("pointee", pointee)])
        }
        CheckedMemoryOperationKind::VolatileWrite { pointee, .. } => {
            ("volatile_write", vec![("pointee", pointee)])
        }
        CheckedMemoryOperationKind::ExposeAddress { pointee } => {
            ("expose_address", vec![("pointee", pointee)])
        }
        CheckedMemoryOperationKind::FromExposedAddress { pointee } => {
            ("from_exposed_address", vec![("pointee", pointee)])
        }
        CheckedMemoryOperationKind::CompareAddress { pointee, .. } => {
            ("compare_address", vec![("pointee", pointee)])
        }
        CheckedMemoryOperationKind::Fence { compiler_only, .. } => (
            if compiler_only {
                "compiler_fence"
            } else {
                "hardware_fence"
            },
            Vec::new(),
        ),
        CheckedMemoryOperationKind::CatastrophicAbort => ("catastrophic_abort", Vec::new()),
        CheckedMemoryOperationKind::DebuggerTrap => ("debugger_trap", Vec::new()),
        CheckedMemoryOperationKind::UnreachableTermination => {
            ("unreachable_termination", Vec::new())
        }
        CheckedMemoryOperationKind::SpinLoopHint => ("spin_loop_hint", Vec::new()),
        CheckedMemoryOperationKind::TargetFeatureEnabled { .. } => {
            ("target_feature_enabled", Vec::new())
        }
        CheckedMemoryOperationKind::InlineAssembly {
            inputs,
            output,
            labels,
            ..
        } => {
            let mut types = vec![("inputs", inputs)];

            if let Some(output) = output {
                types.push(("output", output));
            }

            if let Some(labels) = labels {
                types.push(("labels", labels));
            }

            ("inline_assembly", types)
        }
        CheckedMemoryOperationKind::AtomicInitialize { value } => {
            ("atomic_initialize", vec![("value", value)])
        }
        CheckedMemoryOperationKind::AtomicLoad { value, .. } => {
            ("atomic_load", vec![("value", value)])
        }
        CheckedMemoryOperationKind::AtomicStore { value, .. } => {
            ("atomic_store", vec![("value", value)])
        }
        CheckedMemoryOperationKind::AtomicExchange { value, .. } => {
            ("atomic_exchange", vec![("value", value)])
        }
        CheckedMemoryOperationKind::AtomicCompareExchange {
            value, weak: false, ..
        } => ("atomic_compare_exchange", vec![("value", value)]),
        CheckedMemoryOperationKind::AtomicCompareExchange {
            value, weak: true, ..
        } => ("atomic_compare_exchange_weak", vec![("value", value)]),
        CheckedMemoryOperationKind::AtomicFetch { value, kind, .. } => {
            let name = match kind {
                bray_bound_tree::AtomicFetchKind::Add => "atomic_fetch_add",
                bray_bound_tree::AtomicFetchKind::Subtract => "atomic_fetch_sub",
                bray_bound_tree::AtomicFetchKind::And => "atomic_fetch_and",
                bray_bound_tree::AtomicFetchKind::Or => "atomic_fetch_or",
                bray_bound_tree::AtomicFetchKind::Xor => "atomic_fetch_xor",
            };

            (name, vec![("value", value)])
        }
        CheckedMemoryOperationKind::AtomicWait { value, .. } => {
            ("atomic_wait", vec![("value", value)])
        }
        CheckedMemoryOperationKind::AtomicNotify { value, all: false } => {
            ("atomic_notify_one", vec![("value", value)])
        }
        CheckedMemoryOperationKind::AtomicNotify { value, all: true } => {
            ("atomic_notify_all", vec![("value", value)])
        }
    };

    parts.attribute("memory_operation", name);

    for (role, ty) in types {
        parts.r#type(role, ty, context)?;
    }

    for (index, operand) in memory.operands().iter().enumerate() {
        parts.operand(format!("operand[{index}]"), operand, context)?;
    }

    Ok(())
}

fn call_parts(
    call: &bray_ir::MirCall,
    parts: &mut OperationParts,
    context: &MirInspectionContext<'_>,
) -> Result<(), MirInspectionModelError> {
    match call.target() {
        MirCallTarget::Direct(reference) => {
            parts.attribute("dispatch", "direct");
            callable_reference("callee", *reference, parts, context);
        }
        MirCallTarget::Runtime(reference) => {
            parts.attribute("dispatch", "runtime");
            runtime_reference("callee", *reference, parts);
        }
        MirCallTarget::ConstructionDefault {
            target,
            owner_type,
            provider,
        } => {
            parts.attribute("dispatch", "construction_default");
            construction_target(*target, parts, context)?;
            parts.r#type("owner", *owner_type, context)?;
            parts.symbol("provider", provider.symbol(), context.symbols);
        }
        MirCallTarget::ParameterDefault { callable, provider } => {
            parts.attribute("dispatch", "parameter_default");
            callable_reference("callee", *callable, parts, context);
            parts.symbol("provider", (*provider).into(), context.symbols);
        }
        MirCallTarget::Indirect { callee, abi } => {
            parts.attribute("dispatch", "indirect");
            parts.attribute("callable_abi", callable_abi(*abi));
            parts.operand("callee", callee, context)?;
        }
    }

    match call.result() {
        BoundCallResult::Immediate(ty) => {
            parts.attribute("result_mode", "immediate");
            parts.r#type("result", ty, context)?;
        }
        BoundCallResult::LazyFuture(result) => {
            parts.attribute("result_mode", "lazy_future");
            parts.r#type("completion", result.completion_type(), context)?;
            parts.r#type("result", result.future_type(), context)?;
        }
    }

    for argument in call.arguments() {
        match argument {
            MirCallArgument::Receiver { parameter, value } => {
                parts.symbol("receiver_parameter", (*parameter).into(), context.symbols);
                parts.operand("receiver", value, context)?;
            }
            MirCallArgument::Explicit {
                parameter,
                ordinal,
                value,
            } => {
                if let Some(parameter) = parameter {
                    parts.symbol(
                        format!("parameter[{ordinal}]"),
                        (*parameter).into(),
                        context.symbols,
                    );
                }

                parts.operand(format!("argument[{ordinal}]"), value, context)?;
            }
        }
    }

    if let Some(behaviors) = call.phase_behaviors() {
        parts.attribute(
            "execution",
            match behaviors.execution() {
                bray_symbols::CallableExecution::Synchronous => "synchronous",
                bray_symbols::CallableExecution::Asynchronous => "asynchronous",
            },
        );

        phase_behavior("invocation", behaviors.invocation(), parts, context.symbols);

        if let Some(deferred) = behaviors.deferred_execution() {
            phase_behavior("deferred", deferred, parts, context.symbols);
        }
    }

    if let Some(contract) = call.contract() {
        match contract {
            bray_symbols::CallableContractTemplate::Source(contract) => {
                parts.attribute("contract_kind", "source");

                parts.symbol(
                    "contract_owner",
                    contract.owner().into_any(),
                    context.symbols,
                );

                parts.attribute(
                    "contract_expression_count",
                    compact_id(contract.expressions().len()),
                );

                parts.attribute(
                    "contract_capability_count",
                    compact_id(contract.capabilities().len()),
                );
            }
            bray_symbols::CallableContractTemplate::Resolved(contract) => {
                parts.attribute("contract_kind", "resolved");

                parts.attribute(
                    "contract_precondition_count",
                    compact_id(contract.conditions().invocation_preconditions().len()),
                );

                parts.attribute(
                    "contract_postcondition_count",
                    compact_id(
                        contract
                            .conditions()
                            .normal_completion_postconditions()
                            .len(),
                    ),
                );
            }
        }
    }

    for (index, witness) in call.dispatch_witnesses().iter().enumerate() {
        parts.semantic_value(
            format!("dispatch_implementation[{index}]"),
            "implementation_instance",
            witness.slot(),
            None,
        );
    }

    for (index, witness) in call.witnesses().iter().enumerate() {
        parts.semantic_value(
            format!("implementation[{index}]"),
            "implementation_instance",
            witness.witness().slot(),
            None,
        );

        parts.r#type(
            format!("implementation_subject[{index}]"),
            witness.requirement().subject(),
            context,
        )?;

        parts.semantic_value(
            format!("implementation_trait[{index}]"),
            "trait_application",
            witness.requirement().trait_application().slot(),
            None,
        );
    }

    Ok(())
}

fn callable_reference(
    role: &str,
    reference: MirCallableReference,
    parts: &mut OperationParts,
    context: &MirInspectionContext<'_>,
) {
    let instance = reference.instance();

    parts.symbol(role, instance.definition().symbol(), context.symbols);

    parts.attribute(format!("{role}_abi"), callable_abi(reference.abi()));

    parts.semantic_value(
        format!("{role}_substitution"),
        "generic_substitution",
        instance.substitution().slot(),
        None,
    );
}

fn phase_behavior(
    role: &str,
    behavior: &bray_symbols::CallablePhaseBehavior,
    parts: &mut OperationParts,
    symbols: &SymbolGraph,
) {
    parts.semantic_value(
        format!("{role}_dependency_contract"),
        "dependency_contract_template",
        behavior.dependency_contract().slot(),
        None,
    );

    for (index, effect) in behavior.effects().iter().enumerate() {
        parts.symbol(
            format!("{role}_effect[{index}]"),
            effect.declaration(),
            symbols,
        );
    }

    for (index, capability) in behavior.capabilities().iter().enumerate() {
        parts.symbol(
            format!("{role}_capability[{index}]"),
            capability.declaration(),
            symbols,
        );
    }

    for (index, requirement) in behavior.execution_requirements().iter().enumerate() {
        parts.symbol(
            format!("{role}_execution_requirement[{index}]"),
            requirement.declaration(),
            symbols,
        );
    }

    parts.attribute(
        format!("{role}_may_cancel"),
        matches!(
            behavior.current_run_cancellation(),
            bray_symbols::CurrentRunCancellation::MayEnter
        ),
    );
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
            element,
            exact_count,
        } => {
            parts.attribute("step", "begin");
            parts.attribute("generator_kind", generator_kind(*kind));

            parts.attribute("has_exact_count", exact_count.is_some());
            parts.r#type("element_type", *element, context)?;

            if let Some(exact_count) = exact_count {
                parts.semantic_value("exact_count", "constant_term", exact_count.slot(), None);
            }

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
        MirAsyncOperation::CreateFrame {
            storage,
            frame,
            initializer,
            destination,
        } => {
            parts.attribute("storage", storage.as_str());
            frame_reference("frame", *frame, parts);
            parts.place("destination", destination, context)?;

            match initializer {
                MirFrameInitializer::Callable(call) => {
                    parts.attribute("initializer", "callable");
                    call_parts(call, parts, context)?;
                }
                MirFrameInitializer::Lifecycle {
                    role,
                    ty,
                    receiver,
                    result,
                } => {
                    parts.attribute("initializer", "lifecycle");
                    parts.attribute("role", format!("{role:?}"));
                    parts.r#type("owner", *ty, context)?;
                    parts.operand("receiver", receiver, context)?;
                    parts.r#type("completion", result.completion_type(), context)?;
                    parts.r#type("result", result.future_type(), context)?;
                }
            }

            "create_frame"
        }
        MirAsyncOperation::ResumeFrame {
            state,
            storage,
            runtime,
            frame,
        } => {
            parts.attribute("frame", digest_text(frame.digest()));
            parts.attribute("state", state.raw());
            parts.attribute("storage", storage.slot());
            runtime_reference("runtime", *runtime, parts);

            "resume_frame"
        }
        MirAsyncOperation::ComposeAwaitedFrame {
            parent,
            child,
            frame,
            entry,
        } => {
            parts.attribute("parent_frame", digest_text(parent.digest()));
            frame_reference("child_frame", *child, parts);
            parts.operand("frame", frame, context)?;
            parts.attribute("entry", entry.as_str());

            "compose_awaited_frame"
        }
        MirAsyncOperation::StartTask {
            frame,
            value,
            destination,
            allocation,
            start,
        } => {
            frame_reference("frame", *frame, parts);
            runtime_reference("allocation_runtime", *allocation, parts);
            runtime_reference("start_runtime", *start, parts);
            parts.operand("frame", value, context)?;
            parts.place("destination", destination, context)?;

            "start_task"
        }
        MirAsyncOperation::RequestTaskCancellation { task, runtime } => {
            runtime_reference("runtime", *runtime, parts);
            parts.operand("task", task, context)?;

            "request_task_cancellation"
        }
        MirAsyncOperation::ObserveCurrentRunCancellation { runtime } => {
            runtime_reference("runtime", *runtime, parts);

            "observe_current_run_cancellation"
        }
        MirAsyncOperation::DestroyInactiveCaptures { frame, runtime } => {
            runtime_reference("runtime", *runtime, parts);
            parts.operand("frame", frame, context)?;

            "destroy_inactive_captures"
        }
        MirAsyncOperation::ResolveTask { task, runtime, .. }
        | MirAsyncOperation::BorrowTaskCompletion { task, runtime }
        | MirAsyncOperation::ReleaseTaskCompletionBorrow { task, runtime } => {
            runtime_reference("runtime", *runtime, parts);
            parts.operand("task", task, context)?;

            match operation {
                MirAsyncOperation::BorrowTaskCompletion { .. } => "borrow_task_completion",
                MirAsyncOperation::ReleaseTaskCompletionBorrow { .. } => {
                    "release_task_completion_borrow"
                }
                _ => "resolve_task",
            }
        }
        MirAsyncOperation::ResolveAwaitedFrame { runtime, .. } => {
            runtime_reference("runtime", *runtime, parts);

            "resolve_awaited_frame"
        }
        MirAsyncOperation::PublishTerminalState { state, runtime } => {
            runtime_reference("runtime", *runtime, parts);

            match state {
                MirTaskTerminalState::Completed(value) => {
                    parts.attribute("state", "completed");
                    parts.operand("value", value, context)?;
                }
                MirTaskTerminalState::Cancelled => parts.attribute("state", "cancelled"),
                MirTaskTerminalState::CapturesCompleted => {
                    parts.attribute("state", "captures_completed")
                }
                MirTaskTerminalState::Panicked(report) => {
                    parts.attribute("state", "panicked");
                    parts.operand("report", report, context)?;
                }
            }

            "publish_terminal_state"
        }
        MirAsyncOperation::ExecuteCleanupBroadcast { frame, runtime } => {
            parts.attribute("frame", digest_text(frame.digest()));
            runtime_reference("runtime", *runtime, parts);

            "execute_cleanup_broadcast"
        }
        MirAsyncOperation::ExecuteLifecycleResolution { frame, runtime } => {
            parts.attribute("frame", digest_text(frame.digest()));
            runtime_reference("runtime", *runtime, parts);

            "execute_lifecycle_resolution"
        }
        MirAsyncOperation::TransferCleanupIncident { incident, runtime } => {
            runtime_reference("runtime", *runtime, parts);
            parts.operand("incident", incident, context)?;

            "transfer_cleanup_incident"
        }
        MirAsyncOperation::DestroyTerminalTask { task, completion } => {
            parts.operand("task", task, context)?;

            if let Some(ty) = completion {
                parts.r#type("retained_completion", *ty, context)?;
            }

            "destroy_terminal_task"
        }
    };

    Ok(kind)
}

fn frame_reference(role: &str, frame: MirFrameReference, parts: &mut OperationParts) {
    match frame {
        MirFrameReference::Known(frame) => {
            parts.attribute(format!("{role}_kind"), "known");
            parts.attribute(role, digest_text(frame.digest()));
        }
        MirFrameReference::Erased => parts.attribute(format!("{role}_kind"), "erased"),
    }
}

fn runtime_reference(role: &str, runtime: MirRuntimeReference, parts: &mut OperationParts) {
    parts.attribute(role, runtime.role().as_str());

    let version = runtime.abi_version();

    parts.attribute(
        format!("{role}_abi"),
        format_abi(version.major(), version.minor()),
    );
}

fn host_operation(operation: &MirHostOperation, parts: &mut OperationParts) -> &'static str {
    match operation {
        MirHostOperation::MaterializeStatic { place } => {
            parts.attribute("storage", place.storage().slot());

            "materialize_static"
        }
        MirHostOperation::SelectTestEntry { entry, runtime } => {
            parts.attribute("entry", entry.slot());
            runtime_reference("runtime", *runtime, parts);

            "select_test_entry"
        }
        MirHostOperation::ExecuteRoot {
            entry,
            root,
            execution,
            runtime,
        } => {
            parts.attribute("entry", entry.slot());
            parts.attribute("root_kind", root.kind().as_str());

            match execution {
                bray_runtime_interface::RootExecution::Synchronous => {
                    parts.attribute("execution", "synchronous");
                }
                bray_runtime_interface::RootExecution::Asynchronous { frame } => {
                    parts.attribute("execution", "asynchronous");
                    parts.attribute("frame", digest_text(frame.digest()));
                }
            }

            runtime_reference("runtime", *runtime, parts);

            "execute_root"
        }
        MirHostOperation::ObserveRootTerminal { entry, runtime } => {
            parts.attribute("entry", entry.slot());
            runtime_reference("runtime", *runtime, parts);

            "observe_root_terminal"
        }
        MirHostOperation::ResolveRootTerminal {
            entry,
            error: _,
            completion,
            panic,
            entry_failure,
        } => {
            parts.attribute("entry", entry.slot());
            runtime_reference("completion", *completion, parts);
            runtime_reference("panic", *panic, parts);
            runtime_reference("entry_failure", *entry_failure, parts);

            "resolve_root_terminal"
        }
        MirHostOperation::ReportCleanupIncidents { runtime } => {
            runtime_reference("runtime", *runtime, parts);

            "report_cleanup_incidents"
        }
        MirHostOperation::BeginStaticCleanup => "begin_static_cleanup",
        MirHostOperation::StructuredShutdown { runtime } => {
            runtime_reference("runtime", *runtime, parts);

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
            predicate,
            matched,
            unmatched,
        } => {
            parts.operand("subject", subject, &context)?;
            pattern_predicate(*predicate, &mut parts, &context)?;
            parts.edge("matched", matched, None, &context)?;
            parts.edge("unmatched", unmatched, None, &context)?;

            "pattern_branch"
        }
        MirTerminatorKind::Iterate {
            cursor,
            next,
            element_type,
            item,
            exhausted,
            ..
        } => {
            parts.place("cursor", cursor, &context)?;
            parts.callable_reference("next", *next, &context);
            parts.r#type("element", *element_type, &context)?;

            parts.edges.push(InspectionMirEdge {
                role: String::from("item"),
                target: item.slot(),
                arguments: Vec::new(),
                cleanup_phase: None,
            });

            parts.edge("exhausted", exhausted, None, &context)?;

            "iterate"
        }
        MirTerminatorKind::RangeIterate {
            cursor,
            element_type,
            item,
            exhausted,
        } => {
            parts.place("cursor", cursor, &context)?;
            parts.r#type("element", *element_type, &context)?;

            parts.edges.push(InspectionMirEdge {
                role: String::from("item"),
                target: item.slot(),
                arguments: Vec::new(),
                cleanup_phase: None,
            });

            parts.edge("exhausted", exhausted, None, &context)?;

            "range_iterate"
        }
        MirTerminatorKind::Switch {
            discriminant,
            cases,
            otherwise,
        } => {
            parts.operand("discriminant", discriminant, &context)?;

            for (index, case) in cases.iter().enumerate() {
                parts.semantic_value(
                    format!("case[{index}]"),
                    "constant_value",
                    case.value().slot(),
                    Some(constant_text(case.value(), semantic_values)),
                );

                parts.edge(format!("case[{index}]"), case.edge(), None, &context)?;
            }

            parts.edge("otherwise", otherwise, None, &context)?;

            "switch"
        }
        MirTerminatorKind::InlineAssembly(assembly) => {
            parts.operand("inputs", assembly.inputs(), &context)?;
            parts.r#type("inputs", assembly.inputs_type(), &context)?;
            parts.r#type("output", assembly.output_type(), &context)?;

            parts.edges.push(InspectionMirEdge {
                role: String::from("normal"),
                target: assembly.normal().slot(),
                arguments: Vec::new(),
                cleanup_phase: None,
            });

            for (index, alternate) in assembly.alternates().iter().enumerate() {
                parts.edges.push(InspectionMirEdge {
                    role: format!("alternate[{index}]"),
                    target: alternate.slot(),
                    arguments: Vec::new(),
                    cleanup_phase: None,
                });
            }

            "inline_assembly"
        }
        MirTerminatorKind::Return(value) => {
            if let Some(value) = value {
                parts.operand("value", value, &context)?;
            }

            "return"
        }
        MirTerminatorKind::Unreachable => "unreachable",
        MirTerminatorKind::Suspend {
            kind,
            payload,
            resume_state,
            resume,
            cancellation,
            registration,
            wake,
        } => {
            parts.attribute("kind", kind.as_str());

            if let Some(payload) = payload {
                parts.operand("payload", payload, &context)?;
            }

            parts.attribute("resume_state", resume_state.raw());
            parts.attribute("registration_runtime", registration.role().as_str());

            parts.attribute("registration_runtime_abi", runtime_abi_text(*registration));

            parts.attribute("wake_runtime", wake.role().as_str());
            parts.attribute("wake_runtime_abi", runtime_abi_text(*wake));
            parts.edge("resume", resume, None, &context)?;

            if let Some(cancellation) = cancellation {
                parts.cleanup_edge("cancellation", cancellation, &context)?;
            } else {
                parts.attribute("cancellation", "shielded");
            }

            "suspend"
        }
        MirTerminatorKind::ForwardRunResult { result, edges } => {
            parts.operand("result", result, &context)?;
            parts.edge("completed", edges.completed(), None, &context)?;
            parts.cleanup_edge("panicked", edges.panicked(), &context)?;
            parts.cleanup_edge("cancelled", edges.cancelled(), &context)?;

            "forward_run_result"
        }
        MirTerminatorKind::CheckCallOutcome {
            completed,
            panicked,
            cancelled,
        } => {
            parts.edge("completed", completed, None, &context)?;
            parts.edge("cancelled", cancelled, None, &context)?;

            parts.edges.push(InspectionMirEdge {
                role: String::from("panicked"),
                target: panicked.target().slot(),
                arguments: Vec::new(),
                cleanup_phase: None,
            });

            parts.r#type("report", panicked.report_type(), &context)?;

            "check_call_outcome"
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
        MirTerminatorKind::PropagatePanic { report, .. } => {
            parts.operand("report", report, &context)?;

            "propagate_panic"
        }
        MirTerminatorKind::PropagateCancellation { .. } => "propagate_cancellation",
        MirTerminatorKind::CancelCurrentRun { cleanup } => {
            parts.cleanup_edge("cleanup", cleanup, &context)?;

            "cancel_current_run"
        }
    };

    Ok(InspectionMirTerminator {
        terminator_kind,
        source: inspection_source_anchor(source, symbols, sources)?,
        operands: parts.operands,
        places: parts.places,
        edges: parts.edges,
        attributes: parts.attributes,
        symbols: parts.symbols,
        types: parts.types,
        semantic_values: parts.semantic_values,
    })
}

struct TerminatorParts {
    operands: Vec<InspectionMirNamedOperand>,
    places: Vec<InspectionMirNamedPlace>,
    edges: Vec<InspectionMirEdge>,
    attributes: Vec<InspectionMirAttribute>,
    symbols: Vec<InspectionMirNamedSymbol>,
    types: Vec<InspectionMirNamedType>,
    semantic_values: Vec<InspectionMirSemanticValue>,
}

impl TerminatorParts {
    fn new() -> Self {
        Self {
            operands: Vec::new(),
            places: Vec::new(),
            edges: Vec::new(),
            attributes: Vec::new(),
            symbols: Vec::new(),
            types: Vec::new(),
            semantic_values: Vec::new(),
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
        name: impl Into<String>,
        value: impl Into<InspectionMirAttributeValue>,
    ) {
        self.attributes.push(InspectionMirAttribute {
            name: name.into(),
            value: value.into(),
        });
    }

    fn symbol(&mut self, role: impl Into<String>, symbol: AnySymbolId, symbols: &SymbolGraph) {
        self.symbols.push(InspectionMirNamedSymbol {
            role: role.into(),
            symbol: InspectionSymbolIdentity::from_symbol(symbols, symbol),
        });
    }

    fn r#type(
        &mut self,
        role: impl Into<String>,
        ty: TypeId,
        context: &MirInspectionContext<'_>,
    ) -> Result<(), MirInspectionModelError> {
        self.types.push(InspectionMirNamedType {
            role: role.into(),
            r#type: InspectionType::from_type(context.semantic_values, context.symbols, ty)?,
        });

        Ok(())
    }

    fn semantic_value(
        &mut self,
        role: impl Into<String>,
        value_kind: &'static str,
        id: u32,
        text: Option<String>,
    ) {
        self.semantic_values.push(InspectionMirSemanticValue {
            role: role.into(),
            value_kind,
            id,
            text,
        });
    }

    fn callable_reference(
        &mut self,
        role: &str,
        reference: MirCallableReference,
        context: &MirInspectionContext<'_>,
    ) {
        let instance = reference.instance();

        self.symbol(role, instance.definition().symbol(), context.symbols);
        self.attribute(format!("{role}_abi"), callable_abi(reference.abi()));

        self.semantic_value(
            format!("{role}_substitution"),
            "generic_substitution",
            instance.substitution().slot(),
            None,
        );
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
        r#type: InspectionType::from_type(context.semantic_values, context.symbols, place.ty())?,
        projections,
    })
}

fn inspection_unit_key(
    key: &MirUnitKey,
    symbols: &SymbolGraph,
    sources: &InspectionSources<'_>,
) -> Result<InspectionMirUnitKey, MirInspectionModelError> {
    match key {
        MirUnitKey::CompilerProvidedCallable(definition) => {
            Ok(InspectionMirUnitKey::CompilerProvidedCallable {
                callable: InspectionSymbolIdentity::from_symbol(symbols, definition.symbol()),
            })
        }
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
        MirUnitKey::GeneratedLifecycle(key) => Ok(InspectionMirUnitKey::GeneratedLifecycle {
            role: key.role().as_str(),
            type_identity: digest_text(key.type_identity()),
        }),
        MirUnitKey::ImportedExecutable(key) => Ok(InspectionMirUnitKey::ImportedExecutable {
            owner: InspectionSymbolIdentity::from_symbol(symbols, key.owner()),
            template: key.template().raw(),
        }),
        MirUnitKey::ExternalCallable(definition) => Ok(InspectionMirUnitKey::ExternalCallable {
            callable: InspectionSymbolIdentity::from_symbol(symbols, definition.symbol()),
        }),
        MirUnitKey::ExternalRuntimeDefault(provider) => {
            Ok(InspectionMirUnitKey::ExternalRuntimeDefault {
                provider: InspectionSymbolIdentity::from_symbol(symbols, *provider),
            })
        }
    }
}

fn inspection_source_origin(
    source: &MirSourceOrigin,
    symbols: &SymbolGraph,
    sources: &InspectionSources<'_>,
) -> Result<InspectionMirSource, MirInspectionModelError> {
    match source {
        MirSourceOrigin::CompilerProvidedCallable(definition) => {
            Ok(InspectionMirSource::CompilerProvidedCallable {
                callable: InspectionSymbolIdentity::from_symbol(symbols, definition.symbol()),
            })
        }
        MirSourceOrigin::Source(source) => Ok(InspectionMirSource::Source {
            syntax: InspectionSyntaxAnchor::from_anchor(sources, source.syntax())?,
            synthesis: None,
        }),
        MirSourceOrigin::ExecutableHost(product) => Ok(InspectionMirSource::ExecutableHost {
            package: product.package().as_str().to_owned(),
            product: product.name().to_owned(),
        }),
        MirSourceOrigin::GeneratedLifecycle(reference) => {
            Ok(InspectionMirSource::GeneratedLifecycle {
                role: lifecycle_helper_role(reference)
                    .ok_or(MirInspectionModelError::InvalidGeneratedLifecycle)?,
            })
        }
        MirSourceOrigin::ImportedExecutable(key) => Ok(InspectionMirSource::ImportedExecutable {
            owner: InspectionSymbolIdentity::from_symbol(symbols, key.owner()),
            template: key.template().raw(),
        }),
    }
}

fn inspection_source_anchor(
    source: &MirSourceAnchor,
    symbols: &SymbolGraph,
    sources: &InspectionSources<'_>,
) -> Result<InspectionMirSource, MirInspectionModelError> {
    match source {
        MirSourceAnchor::CompilerProvidedCallable(definition) => {
            Ok(InspectionMirSource::CompilerProvidedCallable {
                callable: InspectionSymbolIdentity::from_symbol(symbols, definition.symbol()),
            })
        }
        MirSourceAnchor::Source(origin) => Ok(InspectionMirSource::Source {
            syntax: InspectionSyntaxAnchor::from_anchor(sources, origin.source_anchor().syntax())?,
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
        MirSourceAnchor::GeneratedLifecycle(reference) => {
            Ok(InspectionMirSource::GeneratedLifecycle {
                role: lifecycle_helper_role(reference)
                    .ok_or(MirInspectionModelError::InvalidGeneratedLifecycle)?,
            })
        }
        MirSourceAnchor::ImportedExecutable(key) => Ok(InspectionMirSource::ImportedExecutable {
            owner: InspectionSymbolIdentity::from_symbol(symbols, key.owner()),
            template: key.template().raw(),
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
        ConstantValueKind::StaticAddress(_) => String::from("<static-address>"),
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

    push_hex_bytes(&mut text, value.magnitude().iter().copied());

    text
}

fn real_bits_text(value: bray_symbols::RealConstantBits) -> String {
    match value {
        bray_symbols::RealConstantBits::Binary16(bits) => format!("f16:0x{bits:04x}"),
        bray_symbols::RealConstantBits::Binary32(bits) => format!("f32:0x{bits:08x}"),
        bray_symbols::RealConstantBits::Binary64(bits) => format!("f64:0x{bits:016x}"),
        bray_symbols::RealConstantBits::Binary128(bits) => {
            let mut text = String::from("f128:0x");

            push_hex_bytes(&mut text, bits);

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
        MirProjectionKind::ActiveUnionPayloadElement { ordinal, .. } => (
            "active_union_payload_element",
            Some(ordinal.raw().to_string()),
        ),
        MirProjectionKind::NullableValue => ("nullable_value", None),
        MirProjectionKind::OwnedStorage => ("owned_storage", None),
    }
}

fn pattern_predicate(
    predicate: MirPatternPredicate,
    parts: &mut TerminatorParts,
    context: &MirInspectionContext<'_>,
) -> Result<(), MirInspectionModelError> {
    match predicate {
        MirPatternPredicate::Literal(literal) => {
            parts.attribute("predicate", "literal");
            parts.semantic_value("predicate", "constant_value", literal.slot(), None);
        }
        MirPatternPredicate::Constant(constant) => {
            parts.attribute("predicate", "constant");
            parts.semantic_value("predicate", "constant_term", constant.slot(), None);
        }
        MirPatternPredicate::NullableAbsent => parts.attribute("predicate", "nullable_absent"),
        MirPatternPredicate::NullablePresent => parts.attribute("predicate", "nullable_present"),
        MirPatternPredicate::ActiveUnionVariant(variant) => {
            parts.attribute("predicate", "active_union_variant");
            parts.symbol("variant", variant.into(), context.symbols);
        }
        MirPatternPredicate::ProductShape(product) => {
            parts.attribute("predicate", "product_shape");
            parts.symbol("product", product.into(), context.symbols);
        }
        MirPatternPredicate::TupleShape(arity) => {
            parts.attribute("predicate", "tuple_shape");
            parts.attribute("arity", arity);
        }
        MirPatternPredicate::ArrayShape(length) => {
            parts.attribute("predicate", "array_shape");
            parts.attribute("length", length);
        }
        MirPatternPredicate::OwnedTarget => parts.attribute("predicate", "owned_target"),
    }

    Ok(())
}

fn construction_target(
    target: ConstructionTarget,
    parts: &mut OperationParts,
    context: &MirInspectionContext<'_>,
) -> Result<(), MirInspectionModelError> {
    match target {
        ConstructionTarget::Struct(symbol) => {
            parts.attribute("target_kind", "struct");
            parts.symbol("target", symbol.into(), context.symbols);
        }
        ConstructionTarget::UnionVariant(symbol) => {
            parts.attribute("target_kind", "union_variant");
            parts.symbol("target", symbol.into(), context.symbols);
        }
        ConstructionTarget::TypeForm {
            callable,
            requirement,
            witness,
        } => {
            parts.attribute("target_kind", "type_form");
            callable_instance("target", callable, parts, context.symbols);
            implementation_requirement("target", requirement, parts, context)?;

            parts.semantic_value(
                "target_witness",
                "implementation_instance",
                witness.slot(),
                None,
            );
        }
    }

    Ok(())
}

fn conversion_parts(
    role: &str,
    conversion: &SelectedConversion,
    parts: &mut OperationParts,
    context: &MirInspectionContext<'_>,
) -> Result<(), MirInspectionModelError> {
    parts.attribute(format!("{role}_kind"), conversion_kind(conversion.target()));
    parts.r#type(format!("{role}_source"), conversion.source_type(), context)?;
    parts.r#type(format!("{role}_target"), conversion.target_type(), context)?;

    match conversion.target() {
        ConversionTarget::Identity
        | ConversionTarget::CallableContract
        | ConversionTarget::NullablePresent
        | ConversionTarget::BuiltInScalar
        | ConversionTarget::CVariadicPromotion => {}
        ConversionTarget::Composite(elements) => {
            for (index, element) in elements.iter().enumerate() {
                conversion_parts(&format!("{role}[{index}]"), element, parts, context)?;
            }
        }
        ConversionTarget::Trait {
            member,
            fulfillment,
            requirement,
            witness,
        } => {
            callable_instance(&format!("{role}_member"), *member, parts, context.symbols);

            callable_instance(
                &format!("{role}_fulfillment"),
                *fulfillment,
                parts,
                context.symbols,
            );

            implementation_requirement(role, *requirement, parts, context)?;

            parts.semantic_value(
                format!("{role}_witness"),
                "implementation_instance",
                witness.slot(),
                None,
            );
        }
        ConversionTarget::TraitConstraint {
            member,
            requirement,
            dispatch,
        } => {
            let Some((owner, ordinal)) = dispatch.constraint() else {
                return Err(MirInspectionModelError::MissingSymbol);
            };

            callable_instance(&format!("{role}_member"), *member, parts, context.symbols);
            implementation_requirement(role, *requirement, parts, context)?;

            parts.attribute(
                format!("{role}_constraint_owner"),
                owner.symbol().kind().as_str(),
            );

            parts.attribute(
                format!("{role}_constraint_ordinal"),
                ordinal.raw().to_string(),
            );
        }
    }

    Ok(())
}

fn callable_instance(
    role: &str,
    callable: bray_symbols::CallableInstanceData,
    parts: &mut OperationParts,
    symbols: &SymbolGraph,
) {
    parts.symbol(role, callable.definition().symbol(), symbols);

    parts.semantic_value(
        format!("{role}_substitution"),
        "generic_substitution",
        callable.substitution().slot(),
        None,
    );
}

fn implementation_requirement(
    role: &str,
    requirement: bray_symbols::ImplementationRequirementKey,
    parts: &mut OperationParts,
    context: &MirInspectionContext<'_>,
) -> Result<(), MirInspectionModelError> {
    parts.r#type(
        format!("{role}_requirement_subject"),
        requirement.subject(),
        context,
    )?;

    parts.semantic_value(
        format!("{role}_requirement_trait"),
        "trait_application",
        requirement.trait_application().slot(),
        None,
    );

    Ok(())
}

const fn construction_input_symbol(input: ConstructionInputId) -> AnySymbolId {
    match input {
        ConstructionInputId::StructField(field) => AnySymbolId::StructField(field),
        ConstructionInputId::UnionPayloadField(field) => AnySymbolId::UnionPayloadField(field),
        ConstructionInputId::CallableParameter(parameter) => {
            AnySymbolId::CallableParameter(parameter)
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

const fn callable_abi(abi: CallableAbi) -> &'static str {
    match abi {
        CallableAbi::Bray => "bray",
        CallableAbi::C => "c",
        CallableAbi::System => "system",
    }
}

fn mir_unit_kind(kind: &MirUnitKind) -> &'static str {
    match kind {
        MirUnitKind::Synchronous => "synchronous",
        MirUnitKind::ProtectedAsyncFrame(_) => "protected_async_frame",
        MirUnitKind::ExecutableHost(_) => "executable_host",
    }
}

fn lifecycle_helper_role(reference: &MirHelperReference) -> Option<&'static str> {
    bray_ir::MirGeneratedLifecycleRole::from_reference(reference)
        .map(bray_ir::MirGeneratedLifecycleRole::as_str)
}

fn storage_kind(kind: &MirStorageKind) -> &'static str {
    match kind {
        MirStorageKind::Parameter(_) => "parameter",
        MirStorageKind::Local => "local",
        MirStorageKind::Temporary => "temporary",
        MirStorageKind::Return => "return",
        MirStorageKind::CurrentFrame => "current_frame",
        MirStorageKind::CurrentTask => "current_task",
        MirStorageKind::ChildTask => "child_task",
        MirStorageKind::Static(_) => "static",
        MirStorageKind::NativeStatic(_) => "native_static",
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
        MirAggregateKind::NullablePresent => "nullable_present",
        MirAggregateKind::Range => "range",
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
        ConversionTarget::CallableContract => "callable_contract",
        ConversionTarget::NullablePresent => "nullable_present",
        ConversionTarget::BuiltInScalar => "built_in_scalar",
        ConversionTarget::CVariadicPromotion => "c_variadic_promotion",
        ConversionTarget::Composite(_) => "composite",
        ConversionTarget::Trait { .. } => "trait",
        ConversionTarget::TraitConstraint { .. } => "trait_constraint",
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

fn pattern_projection_parts(
    projection: PatternProjection,
    parts: &mut OperationParts,
    symbols: &SymbolGraph,
) {
    match projection {
        PatternProjection::ProductField(field) => {
            parts.attribute("projection", "product_field");
            parts.symbol("projected_field", field.into(), symbols);
        }
        PatternProjection::TupleElement(ordinal) => {
            parts.attribute("projection", "tuple_element");
            parts.attribute("projection_ordinal", ordinal.raw());
        }
        PatternProjection::ActiveUnionPayloadField { variant, field } => {
            parts.attribute("projection", "active_union_payload_field");
            parts.symbol("active_variant", variant.into(), symbols);
            parts.symbol("projected_field", field.into(), symbols);
        }
        PatternProjection::ElementFromStart(ordinal) => {
            parts.attribute("projection", "element_from_start");
            parts.attribute("projection_ordinal", ordinal.raw());
        }
        PatternProjection::ElementFromEnd(ordinal) => {
            parts.attribute("projection", "element_from_end");
            parts.attribute("projection_ordinal", ordinal.raw());
        }
        PatternProjection::NullableValue => parts.attribute("projection", "nullable_value"),
        PatternProjection::OwnedTarget => parts.attribute("projection", "owned_target"),
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

fn runtime_abi_text(reference: MirRuntimeReference) -> String {
    let version = reference.abi_version();

    format_abi(version.major(), version.minor())
}

fn digest_text(digest: [u8; 32]) -> String {
    let mut text = String::with_capacity(64);

    push_hex_bytes(&mut text, digest);

    text
}

fn push_hex_bytes(text: &mut String, bytes: impl IntoIterator<Item = u8>) {
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
}

fn compact_id(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}
