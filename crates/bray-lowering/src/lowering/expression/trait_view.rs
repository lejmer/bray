use bray_bound_tree::{
    BoundCallableTarget, BoundExpressionId, PatternOperation, PatternProjection, SelectedCall,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirCallTarget, MirCallableReference, MirCallableSelection, MirOperand,
    MirOperationKind, MirSourceAnchor,
};
use bray_symbols::{
    BorrowKind, CallableParameterData, CallableParameterMode, CallableParameterName,
    CallablePosition, CallableTypeData, SymbolOrdinal, TypeData,
};

use crate::lowering::{LoweringError, block::LoweredExpression, lowerer::Lowerer};

impl Lowerer<'_> {
    #[expect(
        clippy::too_many_arguments,
        reason = "view lowering and selection keep the checked receiver, witness slots, staged inputs, and source context explicit"
    )]
    pub(in crate::lowering::expression) fn lower_trait_view(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        kind: BorrowKind,
        source: MirSourceAnchor,
        requirement: bray_symbols::ImplementationRequirementKey,
        methods: &[Option<bray_bound_tree::MemberTarget>],
        defaults: &[Option<(
            bray_bound_tree::MemberTarget,
            bray_symbols::CallableParameterDefaultProviderSymbolId,
        )>],
    ) -> Result<LoweredExpression, LoweringError> {
        let values = self.input.semantic_values();

        let borrow_type = values.intern_type(TypeData::Borrow {
            kind,
            target: requirement.subject(),
        })?;

        let borrowed = self.lower_storage_borrow(
            id,
            id,
            current,
            kind,
            requirement.subject(),
            borrow_type,
            Self::retained_source(&source),
        )?;

        let Some(current) = borrowed.block else {
            return Ok(borrowed);
        };

        let operand = borrowed
            .value
            .expect("view formation retains its concrete storage borrow");

        let selection = |method: &bray_bound_tree::MemberTarget| {
            let signature = method
                .callable_signature()
                .expect("view entries retain checked callable signatures");

            let data = values.type_data(signature.callable_type());

            let TypeData::Callable(callable) = data.as_ref() else {
                panic!("view entry retains a callable type")
            };

            MirCallableSelection::new(
                MirCallableReference::new(
                    method
                        .callable_instance()
                        .expect("view entry retains its selected implementation"),
                    callable.abi(),
                ),
                method.trait_dispatch(),
                None,
                method.witnesses().iter().map(|witness| witness.witness()),
            )
        };

        let entries = methods
            .iter()
            .map(|method| {
                method
                    .as_ref()
                    .map(|method| bray_ir::MirTraitViewEntry::Callable(selection(method)))
            })
            .chain(defaults.iter().map(|entry| {
                entry.as_ref().map(
                    |(method, provider)| bray_ir::MirTraitViewEntry::DefaultValue {
                        callable: selection(method),
                        provider: *provider,
                    },
                )
            }))
            .collect::<Vec<_>>();

        let result_type = self.expression_type(id);

        let result = self.push_typed_value_operation(
            id,
            current,
            Self::retained_source(&source),
            MirOperationKind::TraitView {
                operand,
                entries: entries.into(),
            },
            result_type,
        )?;

        Ok(LoweredExpression::continuing(current, Some(result), source))
    }

    pub(in crate::lowering) fn is_trait_view_call(&self, selection: &SelectedCall) -> bool {
        matches!(selection.target(), BoundCallableTarget::TraitView { .. })
    }

    pub(in crate::lowering::expression) fn lower_trait_view_call_target(
        &mut self,
        id: BoundExpressionId,
        callee: BoundExpressionId,
        current: MirBlockId,
        selection: &SelectedCall,
        arguments: &mut [bray_ir::MirCallArgument],
        source: &MirSourceAnchor,
    ) -> Result<MirCallTarget, LoweringError> {
        let Some(bray_ir::MirCallArgument::Receiver { value, .. }) = arguments.first_mut() else {
            panic!("view call retains its receiver-first input");
        };

        let view = value.clone();
        let values = self.input.semantic_values();

        let BoundCallableTarget::TraitView { slot, .. } = selection.target() else {
            panic!("view call retains its checked table slot")
        };

        // This internal pointer carries the address only. Source authority remains on the checked receiver.
        let byte = self.representation_type(RepresentationRole::ScalarU8)?;

        let pointer = values.intern_type(TypeData::Borrow {
            kind: BorrowKind::Shared,
            target: byte,
        })?;

        let callable = values.type_data(self.expression_type(callee));

        let TypeData::Callable(callable) = callable.as_ref() else {
            panic!("view method retains its checked callable type")
        };

        let receiver_parameter = CallableParameterData::new(
            CallableParameterName::try_new("self").expect("receiver name is nonempty"),
            CallablePosition::PositionalOrNamed,
            CallableParameterMode::Immutable,
            pointer,
        );

        // The physical signature shares all checked phase contracts with the view surface.
        let callable = CallableTypeData::new(
            std::iter::once(receiver_parameter).chain(callable.parameters().iter().cloned()),
            callable.result(),
            callable.constness(),
            callable.trust(),
            callable.abi(),
            callable.dependency_contracts(),
        )
        .with_variadic(callable.is_variadic())
        .with_phase_behaviors(callable.phase_behaviors().clone());

        let callable = values.intern_type(TypeData::Callable(callable))?;

        let (function, receiver) =
            self.lower_trait_view_lookup(id, current, source, view, slot, callable, pointer)?;

        let target = MirCallTarget::Indirect {
            callee: function,
            abi: selection.abi(),
        };

        *value = receiver;

        Ok(target)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "view lowering and selection keep the checked receiver, witness slots, staged inputs, and source context explicit"
    )]
    fn lower_trait_view_lookup(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        source: &MirSourceAnchor,
        view: MirOperand,
        slot: SymbolOrdinal,
        callable: bray_symbols::TypeId,
        pointer: bray_symbols::TypeId,
    ) -> Result<(MirOperand, MirOperand), LoweringError> {
        let values = self.input.semantic_values();
        let pair = values.intern_type(TypeData::tuple([callable, pointer]))?;

        let pair = self.push_typed_value_operation(
            id,
            current,
            Self::retained_source(source),
            MirOperationKind::TraitViewMethod {
                operand: view,
                slot,
            },
            pair,
        )?;

        let function = self.push_typed_value_operation(
            id,
            current,
            Self::retained_source(source),
            MirOperationKind::PatternProjection {
                subject: pair.clone(),
                projection: PatternProjection::TupleElement(SymbolOrdinal::new(0)),
                operation: PatternOperation::Copy,
            },
            callable,
        )?;

        let receiver = self.push_typed_value_operation(
            id,
            current,
            Self::retained_source(source),
            MirOperationKind::PatternProjection {
                subject: pair,
                projection: PatternProjection::TupleElement(SymbolOrdinal::new(1)),
                operation: PatternOperation::Copy,
            },
            pointer,
        )?;

        Ok((function, receiver))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "view lowering and selection keep the checked receiver, witness slots, staged inputs, and source context explicit"
    )]
    pub(in crate::lowering) fn lower_trait_view_default_target(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        source: &MirSourceAnchor,
        selection: &SelectedCall,
        ordinal: u32,
        result: bray_symbols::TypeId,
        view: MirOperand,
        arguments: &mut [MirOperand],
    ) -> Result<MirCallTarget, LoweringError> {
        let BoundCallableTarget::TraitView { defaults, .. } = selection.target() else {
            panic!("view default retains its table slot")
        };

        let slot = SymbolOrdinal::new(
            defaults
                .raw()
                .checked_add(ordinal)
                .expect("view default slot fits its checked table"),
        );

        let values = self.input.semantic_values();
        let byte = self.representation_type(RepresentationRole::ScalarU8)?;

        let pointer = values.intern_type(TypeData::Borrow {
            kind: BorrowKind::Shared,
            target: byte,
        })?;

        let observed_pointer = values.intern_type(TypeData::Borrow {
            kind: BorrowKind::Shared,
            target: pointer,
        })?;

        let parameters = arguments.iter().enumerate().map(|(index, argument)| {
            CallableParameterData::new(
                CallableParameterName::try_new(format!("input{index}"))
                    .expect("provider parameter name is nonempty"),
                CallablePosition::PositionalOrNamed,
                CallableParameterMode::Immutable,
                if index == 0 {
                    observed_pointer
                } else {
                    self.builder.operand_type(argument)
                },
            )
        });

        let dependencies = values.empty_dependency_contract_template()?;

        let callable = values.intern_type(TypeData::Callable(CallableTypeData::new(
            parameters,
            result,
            bray_symbols::CallableConstness::Runtime,
            bray_symbols::CallableTrust::Safe,
            bray_symbols::CallableAbi::Bray,
            bray_symbols::CallableDependencyContracts::synchronous(dependencies),
        )))?;

        let (function, receiver) =
            self.lower_trait_view_lookup(id, current, source, view, slot, callable, pointer)?;

        let receiver = self.materialize_synthetic_place(
            current,
            Self::retained_source(source),
            receiver,
            pointer,
        )?;

        let receiver = self.push_typed_value_operation(
            id,
            current,
            Self::retained_source(source),
            MirOperationKind::Borrow {
                kind: BorrowKind::Shared,
                place: receiver,
            },
            observed_pointer,
        )?;

        *arguments
            .first_mut()
            .expect("view provider retains its receiver-first observed input") = receiver;

        Ok(MirCallTarget::Indirect {
            callee: function,
            abi: bray_symbols::CallableAbi::Bray,
        })
    }
}
