use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_ir::{MirCall, MirCallTarget, MirCallableReference, MirDefaultOwner, MirOperand};
use bray_lowering::PredicateLoweringContext;
use bray_symbols::{CallableDefinitionId, CallableInstanceData, CallableSignature, CallableSignatureQuery, GenericSubstitutionId, SymbolQueryRequest, TypeData, TypeId};

use super::super::synthetic::CompilationSyntheticLoweringContext;
use crate::compilation::{CodegenPreparationError, binder::binding_query_error};
use crate::fact::FactQueryError;

impl CompilationSyntheticLoweringContext<'_> {
    pub(super) fn predicate_callable_signature(&self, callable: CallableInstanceData) -> Result<(CallableSignature, Vec<TypeId>), CodegenPreparationError> {
        let binding = self.compilation.binding_context(self.cancellation)?;
        let template = binding.resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(callable.definition().callable_symbol()))
            .map_err(binding_query_error)?;
        let constants = self.compilation.checked_constant_terms_for_templates_with_cancellation(
            [template.value().callable_type(), template.value().result()], self.cancellation)?;
        for diagnostics in [template.diagnostics(), constants.diagnostics()] {
            if diagnostics.has_errors() {
                return Err(CodegenPreparationError::Diagnostics(diagnostics.clone()));
            }
        }
        let signature = bray_checker::resolve_callable_signature_template(self.values, template.value(), callable.substitution(), constants.value())
            .expect("checked callable signature has its demanded constant inputs");
        let parameters = signature.input_types(self.values).map_err(crate::fact::FactQueryError::SemanticValueStore)?.into_iter()
            .map(|ty| self.predicate_type(ty)).collect::<Result<_, _>>()?;
        Ok((signature, parameters))
    }

    pub(super) fn predicate_default_call(&self, provider: bray_bound_tree::DefaultValueProvider,
        substitution: GenericSubstitutionId, inputs: Vec<MirOperand>) -> Result<(MirCall, Vec<TypeId>), CodegenPreparationError> {
        let bray_bound_tree::DefaultValueProvider::CallableParameter(_) = provider else {
            panic!("checked template invocation has a callable parameter provider");
        };
        let binding = self.compilation.binding_context(self.cancellation)?;
        let subject = binding.runtime_default_subject(provider.symbol()).map_err(binding_query_error)?
            .expect("checked default invocation retains its subject");
        let bray_symbols::AnySymbolId::CallableParameter(parameter) = subject else {
            panic!("callable default retains its parameter");
        };
        let parameter = binding.callable_parameter(parameter).map_err(binding_query_error)?
            .expect("checked default invocation retains its parameter declaration");
        let definition = CallableDefinitionId::try_new(parameter.owner().into_any())
            .expect("parameter owner is a callable definition");
        let callable = CallableInstanceData::new(definition, substitution);
        let (signature, parameters) = self.predicate_callable_signature(callable)?;
        let expected = usize::try_from(parameter.ordinal()).expect("parameter ordinal fits host") + usize::from(signature.receiver().is_some());
        assert_eq!(inputs.len(), expected, "default invocation supplies exactly its earlier inputs");
        let data = self.values.type_data(signature.callable_type());
        let TypeData::Callable(ty) = data.as_ref() else { panic!("checked default owner has a callable signature"); };
        let result = signature.parameters()[usize::try_from(parameter.ordinal()).expect("parameter ordinal fits host")].ty();
        let result = self.predicate_type(result)?;
        let parameters = parameters.into_iter().take(expected).map(|target| {
            self.values.intern_type(TypeData::Borrow { kind: bray_symbols::BorrowKind::Mutable, target })
                .map_err(FactQueryError::SemanticValueStore).map_err(CodegenPreparationError::from)
        }).collect::<Result<Vec<_>, _>>()?;
        let call = MirCall::protocol(MirCallTarget::DefaultValue {
            owner: MirDefaultOwner::Callable(MirCallableReference::new(callable, ty.abi())), provider,
        }, bray_bound_tree::BoundCallResult::Immediate(result), inputs, []);
        Ok((call, parameters))
    }
}
