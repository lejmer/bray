use bray_codegen::{CodegenTarget, CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping};
use bray_ir::{MirPredicateReference, MirUnitKey, MirUnitId, MirUnit};
use bray_symbols::{BorrowKind, CallableAbi, CallableExecution, TypeData};
use super::super::super::specialization::ConcreteCodegenInstance;
use super::super::synthetic::CompilationSyntheticLoweringContext;
use super::super::support::invocation_signature;
use crate::compilation::{Compilation, CodegenPreparationError};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation::product) fn concrete_codegen_predicate(&self, owner: &ConcreteCodegenInstance, reference: &MirPredicateReference, target: &CodegenTarget, cancellation: &CancellationToken) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        let (context, reference) = match reference {
            MirPredicateReference::Clause { callable, ordinal } => {
                let callee = self.concrete_codegen_callable_data(owner, &callable.instance(), target, cancellation)?;
                let instance = callee.callable_instance().expect("selected requirement retains its callable instance");
                (callee, MirPredicateReference::Clause { callable: bray_ir::MirCallableReference::new(instance, callable.abi()), ordinal: *ordinal })
            }
            MirPredicateReference::Condition { condition, input_types } => {
                let values = self.semantic_value_store()?;
                let condition = match owner.substitution() { Some(substitution) => values.substitute_constant_term(*condition, substitution).map_err(FactQueryError::SemanticValueStore)?, None => *condition };
                let types = input_types.iter().map(|ty| self.concrete_codegen_type(*ty, owner.substitution(), Some(owner), cancellation)).collect::<Result<Vec<_>, _>>()?;
                (owner.clone(), MirPredicateReference::condition(condition, types))
            }
            MirPredicateReference::Predicate(predicate) => {
                let values = self.semantic_value_store()?;
                let substitution = match owner.substitution() { Some(parent) => values.substitute_generic_substitution(predicate.substitution(), parent).map_err(FactQueryError::SemanticValueStore)?, None => predicate.substitution() };
                let substitution = self.realize_codegen_substitution(substitution)?;
                let declaration = values.generic_substitution_data(substitution).owner();
                let requirements = self.concrete_codegen_constraint_requirements(declaration, substitution, cancellation)?;
                let witnesses = requirements.into_iter().map(|requirement| self.concrete_codegen_requirement_witness(owner.implementation_witnesses().iter().copied(), requirement, cancellation)).collect::<Result<Vec<_>, _>>()?;
                let witnesses = self.concrete_codegen_witnesses(witnesses, cancellation)?;
                let reference = MirPredicateReference::Predicate(bray_symbols::PredicateInstanceData::new(predicate.definition(), substitution));
                return Ok(ConcreteCodegenInstance::specialized_helper(MirUnitKey::Predicate(reference), substitution, self.codegen_specialization(substitution)?, &witnesses, owner.key().target().clone()));
            }
        };
        Ok(ConcreteCodegenInstance::inherited_helper(&context, MirUnitKey::Predicate(reference)).expect("selected predicate inherits its established specialization"))
    }

    pub(in crate::compilation::product) fn codegen_predicate_mir(&self, instance: &ConcreteCodegenInstance, unit: MirUnitId, cancellation: &CancellationToken) -> Result<MirUnit, CodegenPreparationError> {
        let MirUnitKey::Predicate(reference) = instance.key().template() else { panic!("predicate body retains its predicate identity"); };
        let context = CompilationSyntheticLoweringContext::new(self, Some(instance), cancellation)?;
        bray_lowering::lower_predicate(&context, reference, unit, instance.key().target())
    }

    pub(in crate::compilation::product::realization) fn codegen_predicate_signature(&self, instance: &ConcreteCodegenInstance, reference: &MirPredicateReference, cancellation: &CancellationToken) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        let types = self.predicate_input_types(reference, cancellation)?.into_iter()
            .map(|ty| self.concrete_codegen_type(ty, instance.substitution(), Some(instance), cancellation))
            .collect::<Result<Vec<_>, _>>()?;
        let values = self.semantic_value_store()?;
        let parameters = types.into_iter().map(|ty| {
            let borrowed = values.intern_type(TypeData::Borrow { kind: BorrowKind::Shared, target: ty }).map_err(FactQueryError::SemanticValueStore)?;
            Ok(CodegenParameterMapping::direct(borrowed, None, []))
        }).collect::<Result<Vec<_>, CodegenPreparationError>>()?;
        Ok(invocation_signature(CodegenCallableSignature::new(parameters, CodegenResultMapping::direct(self.codegen_representation_type(bray_compiler_known::RepresentationRole::ScalarBool)?, None, []), CallableAbi::Bray, false), bray_symbols::CallableInvocation::Bray, CallableExecution::Synchronous))
    }
}
