use super::core::{PredicateLowerer, PredicateLoweringContext};
use bray_bound_tree::{CheckedMemoryOperationKind, MemoryLayoutQueryKind};
use bray_compiler_known::{ImplementationHook, RepresentationRole};
use bray_ir::{
    MirBinaryOperator, MirBlockId, MirCall, MirCallTarget, MirMemoryOperation, MirNullableQuery,
    MirOperand, MirOperationKind, MirTextOperation,
};
use bray_symbols::TypeId;

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(super) fn builtin_call(
        &mut self,
        block: MirBlockId,
        call: &MirCall,
        result: TypeId,
    ) -> Result<Option<MirOperand>, C::Error> {
        let MirCallTarget::Direct(callable) = call.target() else {
            return Ok(None);
        };

        let Some(hook) = self.context.predicate_hook(callable.instance())? else {
            return Ok(None);
        };

        // The operation retains cheap operand descriptors independently of the selected call.
        let inputs = call
            .arguments()
            .iter()
            .map(|argument| argument.value().clone())
            .collect::<Vec<_>>();

        let types = inputs
            .iter()
            .map(|input| self.builder.operand_type(input))
            .collect::<Vec<_>>();

        if let Some(kind) = crate::lowering::expression::nullable::nullable_query_kind(hook) {
            let [operand] = inputs.as_slice() else {
                panic!("checked nullable query retains one receiver");
            };

            let nullable = self.context.semantic_values().unborrowed_type(types[0]);

            return self
                .value(
                    block,
                    MirOperationKind::NullableQuery(MirNullableQuery::new(
                        kind,
                        operand.clone(),
                        types[0],
                        nullable,
                        result,
                    )),
                    result,
                )
                .map(Some);
        }

        if let Some(kind) = crate::lowering::expression::sequence::sequence_operation_kind(hook) {
            let usize_type = self
                .context
                .representation_type(RepresentationRole::ScalarUsize)?;

            let length = self.value(
                block,
                MirOperationKind::Memory(Box::new(MirMemoryOperation::new(
                    CheckedMemoryOperationKind::SequenceLength,
                    inputs,
                    types,
                    Some(usize_type),
                ))),
                usize_type,
            )?;

            let value = match kind {
                crate::lowering::expression::sequence::SequenceOperationKind::Length => length,
                crate::lowering::expression::sequence::SequenceOperationKind::IsEmpty => self
                    .machine(
                        block,
                        MirBinaryOperator::Equal,
                        length,
                        self.integer(usize_type, 0)?,
                        self.boolean,
                    )?,
            };

            return Ok(Some(value));
        }

        if let Some(kind) = crate::lowering::expression::text::text_operation_kind(hook) {
            return self
                .value(
                    block,
                    MirOperationKind::Text(MirTextOperation::new(
                        kind,
                        inputs,
                        types,
                        Some(result),
                    )),
                    result,
                )
                .map(Some);
        }

        if hook == ImplementationHook::NumericTruncate {
            let [value] = inputs.as_slice() else {
                panic!("checked numeric policy retains one argument");
            };

            return self
                .value(
                    block,
                    MirOperationKind::NumericConversion {
                        kind: bray_ir::MirNumericConversionKind::Truncate,
                        operand: value.clone(),
                    },
                    result,
                )
                .map(Some);
        }

        let memory = self.memory_kind(callable.instance(), hook, &types, result)?;

        memory
            .map(|kind| {
                self.value(
                    block,
                    MirOperationKind::Memory(Box::new(MirMemoryOperation::new(
                        kind,
                        inputs,
                        types,
                        Some(result),
                    ))),
                    result,
                )
            })
            .transpose()
    }

    fn memory_kind(
        &self,
        callable: bray_symbols::CallableInstanceData,
        hook: ImplementationHook,
        types: &[TypeId],
        result: TypeId,
    ) -> Result<Option<CheckedMemoryOperationKind>, C::Error> {
        if let Some(kind) = MemoryLayoutQueryKind::for_implementation_hook(hook) {
            let substitution = self
                .context
                .semantic_values()
                .generic_substitution_data(callable.substitution());

            let [binding] = substitution.bindings() else {
                panic!("checked layout query retains exactly one generic argument");
            };

            let bray_symbols::GenericArgument::Type(ty) = binding.argument() else {
                panic!("checked layout query retains its subject type");
            };

            let ty = self.context.predicate_type(ty)?;

            return Ok(Some(CheckedMemoryOperationKind::LayoutQuery { ty, kind }));
        }

        Ok(match hook {
            ImplementationHook::UninitNew | ImplementationHook::UninitFrom => {
                let element = self
                    .context
                    .compiler_known_symbols()
                    .unary_representation_argument(
                        self.context.semantic_values(),
                        RepresentationRole::Uninit,
                        result,
                    )
                    .expect("checked protected initialization retains its element type");

                Some(if hook == ImplementationHook::UninitNew {
                    CheckedMemoryOperationKind::UninitNew { element }
                } else {
                    CheckedMemoryOperationKind::UninitFrom { element }
                })
            }
            ImplementationHook::AtomicInitialize => {
                let value = self
                    .context
                    .compiler_known_symbols()
                    .unary_representation_argument(
                        self.context.semantic_values(),
                        RepresentationRole::Atomic,
                        result,
                    )
                    .expect("checked atomic initialization retains its value type");

                Some(CheckedMemoryOperationKind::AtomicInitialize { value })
            }
            ImplementationHook::RawPointerByteAddress => {
                let pointee = self
                    .context
                    .compiler_known_symbols()
                    .unary_representation_argument(
                        self.context.semantic_values(),
                        RepresentationRole::RawPointer,
                        types[0],
                    )
                    .expect("checked byte-address projection retains its data pointee");

                Some(CheckedMemoryOperationKind::ByteAddress { pointee })
            }
            ImplementationHook::RawPointerNull | ImplementationHook::RawPointerIsNull => {
                let pointer = if hook == ImplementationHook::RawPointerNull {
                    result
                } else {
                    types[0]
                };

                let pointee = self
                    .context
                    .compiler_known_symbols()
                    .unary_representation_argument(
                        self.context.semantic_values(),
                        RepresentationRole::RawPointer,
                        pointer,
                    )
                    .expect("checked pointer observation retains its pointee");

                Some(if hook == ImplementationHook::RawPointerNull {
                    CheckedMemoryOperationKind::Null { pointee }
                } else {
                    CheckedMemoryOperationKind::IsNull { pointee }
                })
            }
            _ => None,
        })
    }
}
