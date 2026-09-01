use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_codegen::{CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticBag;
use bray_ir::MirUnitKey;
use bray_symbols::{
    AnySymbolId, CallableAbi, CallableDefinitionId, CallableExecution,
    CallableParameterDefaultQuery, CallableParameterDefaultValue, CallableSignatureQuery,
    RuntimeDefaultProviderInput, StructFieldDefaultQuery, StructFieldDefaultValue,
    SymbolQueryRequest, TypeData, TypeId, UnionPayloadDefaultValue, UnionPayloadFieldDefaultQuery,
};

use super::super::super::CodegenPreparationError;
use super::super::super::Compilation;
use super::super::super::checker::CompilationCheckerContext;
use super::super::specialization::ConcreteCodegenInstance;
use super::contextual_self::{codegen_instance_contextual_self, substitute_contextual_self};
use super::support::{
    callable_type_signature, codegen_checker_error, is_void_result, receiver_codegen_type,
    synchronous_bray_signature, void_signature,
};
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation::product) fn codegen_instance_signature(
        &self,
        instance: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        if let MirUnitKey::GeneratedLifecycle(_) = instance.key().template() {
            let reference = instance.generated_lifecycle_reference().ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Instance(instance.key().clone()),
                    ProductDataKind::GeneratedLifecycleInstance,
                )
            })?;

            return self.generated_lifecycle_signature(reference, cancellation);
        }

        if let Some(callable_type) = instance.anonymous_callable_type() {
            let callable_type = self.concrete_codegen_type(
                callable_type,
                instance.substitution(),
                Some(instance),
                cancellation,
            )?;

            let callable = self
                .semantic_value_store()?
                .type_data(callable_type)
                .map_err(FactQueryError::SemanticValueStore)?;

            let TypeData::Callable(callable) = callable.as_ref() else {
                return Err(ProductQueryFailure::UnexpectedSemanticType {
                    ty: callable_type,
                    expected: crate::compilation::ProductValueKind::CallableType,
                    actual: callable.as_ref().clone(),
                }
                .into());
            };

            return callable_type_signature(self, callable);
        }

        if let Some(provider) =
            self.codegen_runtime_default_provider(instance.key(), cancellation)?
        {
            return self.codegen_runtime_default_signature(instance, provider, cancellation);
        }

        if let Some(declaration) = instance.static_declaration() {
            return self.codegen_static_initializer_signature(instance, declaration, cancellation);
        }

        let definition = match instance.key().template() {
            MirUnitKey::ExecutableHost(_) => return Ok(void_signature(CallableAbi::Bray)),
            MirUnitKey::Bound(_)
            | MirUnitKey::ImportedExecutable(_)
            | MirUnitKey::ExternalCallable(_) => {
                self.codegen_callable_definition(instance.key())?
            }
            MirUnitKey::GeneratedLifecycle(_) | MirUnitKey::ExternalRuntimeDefault(_) => {
                return Err(ProductQueryFailure::missing(
                    ProductQueryContext::Instance(instance.key().clone()),
                    ProductDataKind::CallableSignature,
                )
                .into());
            }
        };

        let values = self.semantic_value_store()?;

        let callable = instance.callable_instance().ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::Instance(instance.key().clone()),
                ProductDataKind::CallableInstance,
            )
        })?;

        if callable.definition() != definition {
            return Err(ProductQueryFailure::CallableDefinitionMismatch {
                instance: instance.key().clone(),
                expected: definition,
                actual: callable.definition(),
            }
            .into());
        }

        let substitution = callable.substitution();
        let binding_context = self.binding_context(cancellation)?;

        let contextual_self = codegen_instance_contextual_self(&binding_context, instance)?;

        let template = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                definition.callable_symbol(),
            ))
            .map_err(super::super::super::binder::binding_query_error)?;

        let constants = self.checked_constant_terms_for_templates_with_cancellation(
            [template.value().callable_type(), template.value().result()],
            cancellation,
        )?;

        let signature = bray_checker::resolve_callable_signature_template(
            values,
            template.value(),
            substitution,
            constants.value(),
        )
        .map_err(FactQueryError::from)?;

        let Some(signature) = signature else {
            if constants.diagnostics().has_errors() {
                return Err(CodegenPreparationError::Diagnostics(
                    constants.diagnostics().clone(),
                ));
            }

            return Err(ProductQueryFailure::missing(
                ProductQueryContext::CallableDefinition(definition),
                ProductDataKind::CallableSignature,
            )
            .into());
        };

        let signature = signature
            .try_map_types(|ty| substitute_contextual_self(values, ty, contextual_self))?;

        let checker = CompilationCheckerContext::new(binding_context)
            .with_implementation_witnesses(instance.implementation_witnesses().iter().copied());

        let mut diagnostics = DiagnosticBag::new();

        let signature = bray_checker::normalize_callable_signature_type_valued_members(
            &checker,
            signature,
            &mut diagnostics,
        )
        .map_err(codegen_checker_error)?;

        if diagnostics.has_errors() {
            return Err(CodegenPreparationError::Diagnostics(diagnostics));
        }

        let callable = values
            .type_data(signature.callable_type())
            .map_err(FactQueryError::SemanticValueStore)?;

        let TypeData::Callable(callable) = callable.as_ref() else {
            return Err(ProductQueryFailure::UnexpectedSemanticType {
                ty: signature.callable_type(),
                expected: crate::compilation::ProductValueKind::CallableType,
                actual: callable.as_ref().clone(),
            }
            .into());
        };

        let receiver = signature
            .receiver()
            .map(|receiver| {
                let receiver_ty = self.concrete_codegen_type(
                    receiver.ty(),
                    Some(substitution),
                    Some(instance),
                    cancellation,
                )?;

                receiver_codegen_type(values, receiver_ty, receiver.mode())
                    .map_err(CodegenPreparationError::from)
            })
            .transpose()?;

        let parameter_types = signature
            .parameters()
            .iter()
            .map(|parameter| {
                self.concrete_codegen_type(
                    parameter.ty(),
                    Some(substitution),
                    Some(instance),
                    cancellation,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let parameters = receiver
            .into_iter()
            .chain(parameter_types)
            .map(|ty| CodegenParameterMapping::direct(ty, None, []));

        let result_type = self.concrete_codegen_type(
            signature.result(),
            Some(substitution),
            Some(instance),
            cancellation,
        )?;

        let result = if callable.execution() == CallableExecution::Asynchronous {
            let future = self
                .available_compiler_known_symbols()
                .unary_representation_type(values, RepresentationRole::Future, result_type)
                .map_err(FactQueryError::SemanticValueStore)?
                .ok_or_else(|| {
                    ProductQueryFailure::missing(
                        ProductQueryContext::UnaryRepresentation {
                            role: RepresentationRole::Future,
                            argument: result_type,
                        },
                        ProductDataKind::CompilerKnownRepresentation,
                    )
                })?;

            CodegenResultMapping::direct(future, None, [])
        } else if is_void_result(self, result_type)? {
            CodegenResultMapping::Void
        } else {
            CodegenResultMapping::direct(result_type, None, [])
        };

        let signature = CodegenCallableSignature::new(
            parameters,
            result,
            callable.abi(),
            callable.is_variadic(),
        );

        Ok(synchronous_bray_signature(
            signature,
            callable.abi(),
            callable.execution(),
        ))
    }

    pub(super) fn codegen_static_initializer_signature(
        &self,
        instance: &ConcreteCodegenInstance,
        declaration: bray_symbols::StaticSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        let substitution = instance.substitution().ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::Instance(instance.key().clone()),
                ProductDataKind::StaticSubstitution,
            )
        })?;

        let template = self.static_instance_template(declaration)?;

        let ty = self.resolve_codegen_type(
            template.value().declared_type(),
            substitution,
            cancellation,
        )?;

        Ok(CodegenCallableSignature::new(
            [],
            CodegenResultMapping::direct(ty, None, []),
            CallableAbi::Bray,
            false,
        ))
    }

    pub(super) fn codegen_runtime_default_signature(
        &self,
        instance: &ConcreteCodegenInstance,
        provider: AnySymbolId,
        cancellation: &CancellationToken,
    ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        let binding_context = self.binding_context(cancellation)?;

        let owner = binding_context
            .runtime_default_subject(provider)
            .map_err(super::super::super::binder::binding_query_error)?
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::Symbol(provider),
                    ProductDataKind::RuntimeDefaultSubject,
                )
            })?;

        let (inputs, result) = match owner {
            AnySymbolId::CallableParameter(owner) => {
                let checked = binding_context
                    .resolve_symbol_query(SymbolQueryRequest::<CallableParameterDefaultQuery>::new(
                        owner,
                    ))
                    .map_err(super::super::super::binder::binding_query_error)?;

                let CallableParameterDefaultValue::Valid(surface) = checked.value().value() else {
                    return Err(CodegenPreparationError::Diagnostics(
                        checked.diagnostics().clone(),
                    ));
                };

                (surface.inputs().to_vec(), surface.result())
            }
            AnySymbolId::StructField(owner) => {
                let checked = binding_context
                    .resolve_symbol_query(SymbolQueryRequest::<StructFieldDefaultQuery>::new(owner))
                    .map_err(super::super::super::binder::binding_query_error)?;

                let StructFieldDefaultValue::Valid(surface) = checked.value().value() else {
                    return Err(CodegenPreparationError::Diagnostics(
                        checked.diagnostics().clone(),
                    ));
                };

                (surface.inputs().to_vec(), surface.result())
            }
            AnySymbolId::UnionPayloadField(owner) => {
                let checked = binding_context
                    .resolve_symbol_query(SymbolQueryRequest::<UnionPayloadFieldDefaultQuery>::new(
                        owner,
                    ))
                    .map_err(super::super::super::binder::binding_query_error)?;

                let UnionPayloadDefaultValue::Valid(surface) = checked.value().value() else {
                    return Err(CodegenPreparationError::Diagnostics(
                        checked.diagnostics().clone(),
                    ));
                };

                (surface.inputs().to_vec(), surface.result())
            }
            _ => {
                return Err(ProductQueryFailure::UnsupportedRuntimeDefaultSubject {
                    provider,
                    subject: owner,
                }
                .into());
            }
        };

        let substitution = instance.substitution();

        let parameters = inputs
            .iter()
            .copied()
            .map(|input| {
                self.codegen_runtime_default_input_type(input, instance, cancellation)
                    .map(|ty| CodegenParameterMapping::direct(ty, None, []))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let result =
            self.concrete_codegen_type(result, substitution, Some(instance), cancellation)?;

        let is_void = is_void_result(self, result)?;

        let result = if is_void {
            CodegenResultMapping::Void
        } else {
            CodegenResultMapping::direct(result, None, [])
        };

        Ok(
            CodegenCallableSignature::new(parameters, result, CallableAbi::Bray, false)
                .with_panic_report_context(),
        )
    }

    pub(super) fn codegen_runtime_default_input_type(
        &self,
        input: RuntimeDefaultProviderInput,
        instance: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<TypeId, CodegenPreparationError> {
        let binding_context = self.binding_context(cancellation)?;

        let (owner, parameter) = match input {
            RuntimeDefaultProviderInput::Receiver(parameter) => {
                let owner = binding_context
                    .receiver_parameter(parameter)
                    .map_err(super::super::super::binder::binding_query_error)?
                    .map(bray_symbols::ReceiverParameterSymbol::owner)
                    .ok_or_else(|| {
                        ProductQueryFailure::missing(
                            ProductQueryContext::Symbol(parameter.into()),
                            ProductDataKind::Symbol,
                        )
                    })?;

                (owner, AnySymbolId::ReceiverParameter(parameter))
            }
            RuntimeDefaultProviderInput::EarlierParameter(parameter) => {
                let owner = binding_context
                    .callable_parameter(parameter)
                    .map_err(super::super::super::binder::binding_query_error)?
                    .map(bray_symbols::CallableParameterSymbol::owner)
                    .ok_or_else(|| {
                        ProductQueryFailure::missing(
                            ProductQueryContext::Symbol(parameter.into()),
                            ProductDataKind::Symbol,
                        )
                    })?;

                (owner, AnySymbolId::CallableParameter(parameter))
            }
        };

        let template = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(owner))
            .map_err(super::super::super::binder::binding_query_error)?;

        let substitution = instance.substitution().ok_or_else(|| {
            ProductQueryFailure::missing(
                ProductQueryContext::Instance(instance.key().clone()),
                ProductDataKind::GenericSubstitution,
            )
        })?;

        let constants = self.checked_constant_terms_for_templates_with_cancellation(
            [template.value().callable_type(), template.value().result()],
            cancellation,
        )?;

        let signature = bray_checker::resolve_callable_signature_template(
            self.semantic_value_store()?,
            template.value(),
            substitution,
            constants.value(),
        )
        .map_err(FactQueryError::from)?;

        let Some(signature) = signature else {
            if constants.diagnostics().has_errors() {
                return Err(CodegenPreparationError::Diagnostics(
                    constants.diagnostics().clone(),
                ));
            }

            return Err(ProductQueryFailure::missing(
                ProductQueryContext::Instance(instance.key().clone()),
                ProductDataKind::CallableSignature,
            )
            .into());
        };

        let values = self.semantic_value_store()?;

        let receiver = signature
            .receiver()
            .filter(|receiver| AnySymbolId::ReceiverParameter(receiver.parameter()) == parameter)
            .map(|receiver| {
                let ty = self.concrete_codegen_type(
                    receiver.ty(),
                    Some(substitution),
                    Some(instance),
                    cancellation,
                )?;

                receiver_codegen_type(values, ty, receiver.mode())
                    .map_err(CodegenPreparationError::from)
            })
            .transpose()?;

        let ty = match receiver {
            Some(receiver) => receiver,
            None => signature
                .parameters()
                .iter()
                .find_map(|candidate| {
                    (AnySymbolId::CallableParameter(candidate.parameter()) == parameter)
                        .then_some(candidate.ty())
                })
                .ok_or_else(|| {
                    ProductQueryFailure::missing(
                        ProductQueryContext::Symbol(parameter),
                        ProductDataKind::CallableParameters,
                    )
                })?,
        };

        self.concrete_codegen_type(ty, Some(substitution), Some(instance), cancellation)
    }

    pub(super) fn codegen_callable_definition(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
    ) -> Result<CallableDefinitionId, FactQueryError> {
        match instance.template() {
            MirUnitKey::Bound(key) => {
                let symbol = self
                    .symbol_graph()?
                    .symbol_for_key(key.declared_owner())
                    .ok_or_else(|| {
                        ProductQueryFailure::missing(
                            ProductQueryContext::SymbolKey(key.declared_owner().clone()),
                            ProductDataKind::Symbol,
                        )
                    })?;

                CallableDefinitionId::try_new(symbol).ok_or_else(|| {
                    ProductQueryFailure::InvalidCallableDefinitionSymbol {
                        symbol,
                        actual: symbol.kind(),
                    }
                    .into()
                })
            }
            MirUnitKey::ImportedExecutable(definition) => {
                let symbol = definition.owner();

                CallableDefinitionId::try_new(symbol).ok_or_else(|| {
                    ProductQueryFailure::InvalidCallableDefinitionSymbol {
                        symbol,
                        actual: symbol.kind(),
                    }
                    .into()
                })
            }
            MirUnitKey::ExternalCallable(definition) => Ok(*definition),
            MirUnitKey::ExecutableHost(_)
            | MirUnitKey::GeneratedLifecycle(_)
            | MirUnitKey::ExternalRuntimeDefault(_) => Err(ProductQueryFailure::missing(
                ProductQueryContext::Instance(instance.clone()),
                ProductDataKind::CallableSignature,
            )
            .into()),
        }
    }
}
