use super::super::synthetic::CompilationSyntheticLoweringContext;
use crate::compilation::CodegenPreparationError;
use crate::compilation::binder::binding_query_error;
use crate::fact::FactQueryError;
use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{CheckedTemplate, CheckedTemplateIndexCall};
use bray_ir::{MirCall, MirCallTarget, MirCallableReference, MirOperand, MirPredicateReference};
use bray_lowering::PredicateLoweringContext;
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData, CallableSignatureQuery,
    ConstantTermData, ConstantTermId, ConstantValueId, GenericSubstitutionId,
    ImplementationInstanceId, PredicateDefinitionSymbolId, SymbolKey, SymbolKeyData,
    SymbolQueryRequest, TypeId,
};
use std::sync::Arc;

impl PredicateLoweringContext for CompilationSyntheticLoweringContext<'_> {
    fn predicate_hook(
        &self,
        callable: CallableInstanceData,
    ) -> Result<Option<bray_compiler_known::ImplementationHook>, Self::Error> {
        use bray_checker::CheckerRequestContext;

        let context = self.compilation.checker_context(self.cancellation)?;

        let hook = context
            .implementation_hook(callable.definition().symbol())
            .map_err(FactQueryError::from)?;

        Ok(hook
            .filter(|hook| hook.is_available())
            .map(|hook| hook.hook()))
    }
    fn predicate_input_ordinal(
        &self,
        field: bray_bound_tree::ConstructionInputId,
    ) -> Result<u32, Self::Error> {
        let binding = self.compilation.binding_context(self.cancellation)?;

        Ok(match field {
            bray_bound_tree::ConstructionInputId::StructField(field) => binding
                .struct_field(field)
                .map_err(binding_query_error)?
                .expect("checked constructor retains its struct field")
                .ordinal(),
            bray_bound_tree::ConstructionInputId::UnionPayloadField(field) => binding
                .union_payload_field(field)
                .map_err(binding_query_error)?
                .expect("checked constructor retains its union field")
                .ordinal(),
            bray_bound_tree::ConstructionInputId::CallableParameter(parameter) => binding
                .callable_parameter(parameter)
                .map_err(binding_query_error)?
                .expect("checked constructor retains its parameter")
                .ordinal(),
        })
    }
    fn predicate_template(
        &self,
        reference: &MirPredicateReference,
    ) -> Result<Arc<CheckedTemplate>, Self::Error> {
        self.compilation
            .predicate_checked_template(reference, self.cancellation)
    }
    fn predicate_type(&self, ty: TypeId) -> Result<TypeId, Self::Error> {
        let instance = self
            .realization
            .expect("predicate lowering retains its selected specialization");

        self.compilation.concrete_codegen_type(
            ty,
            instance.substitution(),
            Some(instance),
            self.cancellation,
        )
    }
    fn predicate_symbol(&self, key: &SymbolKey) -> Result<AnySymbolId, Self::Error> {
        let symbol = match key.data() {
            SymbolKeyData::External(key) => self
                .compilation
                .imported_symbol_skeleton_result_with_cancellation(self.cancellation)?
                .value()
                .as_ref()
                .and_then(|symbols| symbols.symbol_by_external_key(key)),
            _ => self.compilation.symbol_graph()?.symbol_for_key(key),
        };

        Ok(symbol.expect("checked template retains a resolvable declaration identity"))
    }
    fn predicate_constant(&self, term: ConstantTermId) -> Result<ConstantValueId, Self::Error> {
        let term = self.compilation.substitute_codegen_constant_term(
            term,
            self.realization
                .and_then(|instance| instance.substitution()),
        )?;

        let data = self.values.constant_term_data(term);

        match data.as_ref() {
            ConstantTermData::Value(value) => Ok(*value),
            ConstantTermData::Typed { term, .. } => self.predicate_constant(*term),
            _ => panic!(
                "checked native OpenConstantTerm contract failed: {:?}",
                term
            ),
        }
    }
    fn predicate_declaration(
        &self,
        key: &SymbolKey,
        substitution: Option<GenericSubstitutionId>,
        ty: TypeId,
    ) -> Result<MirOperand, Self::Error> {
        let symbol = self.predicate_symbol(key)?;

        if let AnySymbolId::GenericConstParameter(parameter) = symbol {
            let term = self
                .values
                .intern_constant_term(ConstantTermData::Parameter(parameter))
                .map_err(FactQueryError::SemanticValueStore)?;

            return Ok(MirOperand::Constant {
                value: self.predicate_constant(term)?,
                ty,
            });
        }

        if let AnySymbolId::UnionVariant(variant) = symbol {
            let value = self
                .values
                .intern_constant_value(bray_symbols::ConstantValueData::new(
                    ty,
                    bray_symbols::ConstantValueKind::union(variant, []),
                ))
                .map_err(FactQueryError::SemanticValueStore)?;

            return Ok(MirOperand::Constant { value, ty });
        }

        let definition = crate::compilation::constant::constant_definition_id(symbol)
            .expect("checked predicate declaration is a constant");

        let substitution = crate::compilation::substitution::substitution_for_owner(
            self.values,
            definition.into_any(),
            substitution.or_else(|| {
                self.realization
                    .and_then(|instance| instance.substitution())
            }),
        )?;

        let checked = self.compilation.constant_instance_with_cancellation(
            bray_symbols::ConstantInstanceKey::new(definition, substitution, None),
            self.cancellation,
        )?;

        if checked.diagnostics().has_errors() {
            return Err(CodegenPreparationError::Diagnostics(
                checked.diagnostics().clone(),
            ));
        }

        Ok(MirOperand::Constant {
            value: checked.value().value(),
            ty,
        })
    }
    fn predicate_call(
        &self,
        key: &SymbolKey,
        substitution: GenericSubstitutionId,
        dispatch: Option<&bray_bound_tree::CheckedTemplateDispatch>,
        inputs: Vec<MirOperand>,
    ) -> Result<(MirCall, Vec<TypeId>), Self::Error> {
        let symbol = self.predicate_symbol(key)?;

        if let Some(provider) = bray_symbols::DefaultValueProvider::try_from_symbol(symbol) {
            assert!(
                dispatch.is_none(),
                "default helper has no selected trait dispatch"
            );

            return self.predicate_default_call(provider, substitution, inputs);
        }

        if let Some(predicate) = PredicateDefinitionSymbolId::try_from_any(symbol) {
            return self.predicate_application(
                bray_symbols::PredicateInstanceData::new(predicate, substitution),
                inputs,
            );
        }

        let definition = CallableDefinitionId::try_new(symbol)
            .expect("checked predicate call retains a callable declaration");

        let implementation = dispatch
            .and_then(|dispatch| match dispatch {
                bray_bound_tree::CheckedTemplateDispatch::Implementation(key, substitution) => {
                    Some((key, substitution))
                }
                _ => None,
            })
            .map(|(key, substitution)| {
                let definition =
                    bray_symbols::ImplementationSymbolId::try_from_any(self.predicate_symbol(key)?)
                        .expect("checked dispatch retains its implementation");

                self.values
                    .intern_implementation_instance(bray_symbols::ImplementationInstanceData::new(
                        definition,
                        *substitution,
                    ))
                    .map_err(|cause| {
                        CodegenPreparationError::from(FactQueryError::SemanticValueStore(cause))
                    })
            })
            .transpose()?;

        let (mut call, parameters) = self.predicate_instance_call(
            CallableInstanceData::new(definition, substitution),
            implementation,
            inputs,
        )?;

        let dispatch = dispatch
            .map(|dispatch| dispatch.trait_dispatch(self.values, |key| self.predicate_symbol(key)))
            .transpose()?
            .flatten();

        if let Some(dispatch) = dispatch {
            call = call.with_trait_dispatch(dispatch);
        }

        Ok((call, parameters))
    }
    fn predicate_application(
        &self,
        predicate: bray_symbols::PredicateInstanceData,
        inputs: Vec<MirOperand>,
    ) -> Result<(MirCall, Vec<TypeId>), Self::Error> {
        let reference = MirPredicateReference::Predicate(predicate);

        let parameters = self
            .compilation
            .predicate_input_types(&reference, self.cancellation)?
            .into_iter()
            .map(|ty| {
                let ty = self
                    .values
                    .substitute_type(ty, predicate.substitution())
                    .map_err(FactQueryError::SemanticValueStore)?;

                let ty = self.predicate_type(ty)?;

                self.values
                    .intern_type(bray_symbols::TypeData::Borrow {
                        kind: bray_symbols::BorrowKind::Shared,
                        target: ty,
                    })
                    .map_err(|cause| {
                        CodegenPreparationError::from(FactQueryError::SemanticValueStore(cause))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok((
            MirCall::protocol(
                MirCallTarget::Predicate(reference),
                bray_bound_tree::BoundCallResult::Immediate(
                    self.compilation.codegen_representation_type(
                        bray_compiler_known::RepresentationRole::ScalarBool,
                    )?,
                ),
                inputs,
                [],
            ),
            parameters,
        ))
    }
    fn predicate_instance_call(
        &self,
        callable: CallableInstanceData,
        implementation: Option<ImplementationInstanceId>,
        inputs: Vec<MirOperand>,
    ) -> Result<(MirCall, Vec<TypeId>), Self::Error> {
        let (signature, parameters) = self.predicate_callable_signature(callable)?;

        let binding = self.compilation.binding_context(self.cancellation)?;

        let contracts = binding
            .resolve_symbol_query(
                SymbolQueryRequest::<bray_symbols::CallableContractsQuery>::new(
                    callable.definition().callable_symbol(),
                ),
            )
            .map_err(binding_query_error)?;

        if contracts.diagnostics().has_errors() {
            return Err(CodegenPreparationError::Diagnostics(
                contracts.diagnostics().clone(),
            ));
        }

        let data = self.values.type_data(signature.callable_type());

        let bray_symbols::TypeData::Callable(ty) = data.as_ref() else {
            panic!("checked callable signature retains its callable type");
        };

        let result = self.predicate_type(signature.result())?;
        let target = MirCallTarget::Direct(MirCallableReference::new(callable, ty.abi()));

        let arguments = inputs.into_iter().enumerate().map(|(ordinal, value)| {
            bray_ir::MirCallArgument::positional(
                u32::try_from(ordinal).expect("checked call input ordinal fits its signature"),
                value,
            )
        });

        // The emitted call retains the declaration's checked immutable phase behavior.
        Ok((
            MirCall::selected(
                target,
                bray_bound_tree::BoundCallResult::Immediate(result),
                arguments,
                contracts.value().phase_behaviors().clone(),
                None,
                implementation,
                None,
                [],
            )
            .with_invocation(signature.invocation()),
            parameters,
        ))
    }
    fn predicate_index_bound_type(
        &self,
        selected: &CheckedTemplateIndexCall,
    ) -> Result<TypeId, Self::Error> {
        let definition = CallableDefinitionId::try_new(self.predicate_symbol(&selected.callable)?)
            .expect("checked index retains a callable declaration");

        let binding = self.compilation.binding_context(self.cancellation)?;

        let signature = binding
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                definition.callable_symbol(),
            ))
            .map_err(binding_query_error)?;

        if signature.diagnostics().has_errors() {
            return Err(CodegenPreparationError::Diagnostics(
                signature.diagnostics().clone(),
            ));
        }

        let parameters = signature
            .value()
            .parameter_type_templates(self.values)
            .expect("checked slice signature exposes its parameter templates");

        let bound = self.compilation.resolve_codegen_type(
            parameters
                .first()
                .expect("checked custom slice has bound parameters"),
            selected.substitution,
            self.cancellation,
        )?;

        self.predicate_type(bound)
    }
    fn predicate_field_type(
        &self,
        subject: TypeId,
        field: bray_ir::MirFieldReference,
    ) -> Result<TypeId, Self::Error> {
        let symbol = match field {
            bray_ir::MirFieldReference::Struct(field) => field.into(),
            bray_ir::MirFieldReference::UnionPayload(field) => field.into(),
        };

        let template = self
            .compilation
            .symbol_type_template(symbol)?
            .expect("checked projection field retains its type template");

        if template.diagnostics().has_errors() {
            return Err(CodegenPreparationError::Diagnostics(
                template.diagnostics().clone(),
            ));
        }

        let data = self.values.type_data(subject);

        let bray_symbols::TypeData::Named { substitution, .. } = data.as_ref() else {
            panic!("checked field projection retains its named subject");
        };

        let ty = self.compilation.resolve_codegen_type(
            template.value(),
            *substitution,
            self.cancellation,
        )?;

        self.predicate_type(ty)
    }
}
