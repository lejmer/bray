use super::super::super::specialization::ConcreteCodegenInstance;
use super::super::support::invocation_signature;
use super::super::synthetic::CompilationSyntheticLoweringContext;
use crate::compilation::{CodegenPreparationError, Compilation};
use crate::fact::{CancellationToken, FactQueryError};
use bray_base::StableDigestHasher;
use bray_codegen::{
    CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping, CodegenTarget,
};
use bray_ir::{MirPredicateReference, MirUnit, MirUnitId, MirUnitKey};
use bray_symbols::{BorrowKind, CallableAbi, CallableExecution, TypeData};
use std::hash::{Hash, Hasher};

impl Compilation {
    pub(in crate::compilation::product) fn concrete_codegen_predicate(
        &self,
        owner: &ConcreteCodegenInstance,
        reference: &MirPredicateReference,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenInstance, CodegenPreparationError> {
        let (context, reference) = match reference {
            MirPredicateReference::Clause { callable, ordinal } => {
                let callee = self.concrete_codegen_callable_data(
                    owner,
                    &callable.instance(),
                    target,
                    cancellation,
                )?;

                let instance = callee
                    .callable_instance()
                    .expect("selected requirement retains its callable instance");

                (
                    callee,
                    MirPredicateReference::Clause {
                        callable: bray_ir::MirCallableReference::new(instance, callable.abi()),
                        ordinal: *ordinal,
                    },
                )
            }
            MirPredicateReference::Condition {
                condition,
                input_types,
            } => {
                let values = self.semantic_value_store()?;

                let condition = match owner.substitution() {
                    Some(substitution) => values
                        .substitute_constant_term(*condition, substitution)
                        .map_err(FactQueryError::SemanticValueStore)?,
                    None => *condition,
                };

                let types = input_types
                    .iter()
                    .map(|ty| {
                        self.concrete_codegen_type(
                            *ty,
                            owner.substitution(),
                            Some(owner),
                            cancellation,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                (
                    owner.clone(),
                    MirPredicateReference::condition(condition, types),
                )
            }
            MirPredicateReference::Predicate(predicate) => {
                let values = self.semantic_value_store()?;

                let substitution = match owner.substitution() {
                    Some(parent) => values
                        .substitute_generic_substitution(predicate.substitution(), parent)
                        .map_err(FactQueryError::SemanticValueStore)?,
                    None => predicate.substitution(),
                };

                let substitution = self.realize_codegen_substitution(substitution)?;
                let declaration = values.generic_substitution_data(substitution).owner();

                let requirements = self.concrete_codegen_constraint_requirements(
                    declaration,
                    substitution,
                    cancellation,
                )?;

                let witnesses = requirements
                    .into_iter()
                    .map(|requirement| {
                        self.concrete_codegen_requirement_witness(
                            owner.implementation_witnesses().iter().copied(),
                            requirement,
                            cancellation,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let witnesses = self.concrete_codegen_witnesses(witnesses, cancellation)?;

                let reference = MirPredicateReference::Predicate(
                    bray_symbols::PredicateInstanceData::new(predicate.definition(), substitution),
                );

                let specialization = self.codegen_specialization(substitution)?;
                let key = self.predicate_key(reference, &specialization, cancellation)?;

                return Ok(ConcreteCodegenInstance::specialized_helper(
                    MirUnitKey::Predicate(key),
                    substitution,
                    specialization,
                    &witnesses,
                    owner.key().target().clone(),
                ));
            }
        };

        let key = self.predicate_key(reference, context.key().specialization(), cancellation)?;

        Ok(
            ConcreteCodegenInstance::inherited_helper(&context, MirUnitKey::Predicate(key))
                .expect("selected predicate inherits its established specialization"),
        )
    }

    pub(in crate::compilation::product) fn codegen_predicate_mir(
        &self,
        instance: &ConcreteCodegenInstance,
        unit: MirUnitId,
        cancellation: &CancellationToken,
    ) -> Result<MirUnit, CodegenPreparationError> {
        let MirUnitKey::Predicate(key) = instance.key().template() else {
            panic!("predicate body retains its predicate identity");
        };

        let context = CompilationSyntheticLoweringContext::new(self, Some(instance), cancellation)?;

        bray_lowering::lower_predicate(&context, key, unit, instance.key().target())
    }

    pub(in crate::compilation::product::realization) fn codegen_predicate_signature(
        &self,
        instance: &ConcreteCodegenInstance,
        reference: &MirPredicateReference,
        cancellation: &CancellationToken,
    ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        let types = self
            .predicate_input_types(reference, cancellation)?
            .into_iter()
            .map(|ty| {
                self.concrete_codegen_type(
                    ty,
                    instance.substitution(),
                    Some(instance),
                    cancellation,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let values = self.semantic_value_store()?;

        let parameters = types
            .into_iter()
            .map(|ty| {
                let borrowed = values
                    .intern_type(TypeData::Borrow {
                        kind: BorrowKind::Shared,
                        target: ty,
                    })
                    .map_err(FactQueryError::SemanticValueStore)?;

                Ok(CodegenParameterMapping::direct(borrowed, None, []))
            })
            .collect::<Result<Vec<_>, CodegenPreparationError>>()?;

        Ok(invocation_signature(
            CodegenCallableSignature::new(
                parameters,
                CodegenResultMapping::direct(
                    self.codegen_representation_type(
                        bray_compiler_known::RepresentationRole::ScalarBool,
                    )?,
                    None,
                    [],
                ),
                CallableAbi::Bray,
                false,
            ),
            bray_symbols::CallableInvocation::Bray,
            CallableExecution::Synchronous,
        ))
    }
}

impl Compilation {
    fn predicate_key(
        &self,
        reference: bray_ir::MirPredicateReference,
        specialization: &bray_codegen::CodegenSpecialization,
        cancellation: &CancellationToken,
    ) -> Result<bray_ir::MirPredicateKey, CodegenPreparationError> {
        use crate::compilation::product::specialization_identity::content::{
            StructuralSemanticValue, structural_semantic_identity,
        };

        let context = self.binding_context(cancellation)?;
        let values = self.semantic_value_store()?;
        let mut hasher = StableDigestHasher::new();

        hasher.write(b"bray.codegen-predicate-key.v1");

        match &reference {
            bray_ir::MirPredicateReference::Clause { callable, ordinal } => {
                hasher.write_u8(0);

                self.portable_codegen_symbol_key(
                    &context,
                    callable.instance().definition().symbol(),
                )?
                .hash(&mut hasher);

                ordinal.hash(&mut hasher);
                callable.abi().hash(&mut hasher);
            }
            bray_ir::MirPredicateReference::Predicate(predicate) => {
                hasher.write_u8(1);

                self.portable_codegen_symbol_key(&context, predicate.definition().into_any())?
                    .hash(&mut hasher);
            }
            bray_ir::MirPredicateReference::Condition {
                condition,
                input_types,
            } => {
                hasher.write_u8(2);

                structural_semantic_identity(
                    values,
                    &context,
                    StructuralSemanticValue::ConstantTerm(*condition),
                )?
                .hash(&mut hasher);

                input_types.len().hash(&mut hasher);

                for ty in input_types.iter() {
                    structural_semantic_identity(
                        values,
                        &context,
                        StructuralSemanticValue::Type(*ty),
                    )?
                    .hash(&mut hasher);
                }
            }
        }

        specialization.hash(&mut hasher);

        Ok(bray_ir::MirPredicateKey::new(hasher.finalize(), reference))
    }
}
