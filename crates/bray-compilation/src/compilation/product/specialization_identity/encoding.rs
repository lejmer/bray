use std::hash::{Hash, Hasher};

use bray_base::StableDigestHasher;
use bray_codegen::{CodegenGenericArgument, CodegenValueKey};
use bray_symbols::{
    AnySymbolId, CallableInstanceId, CallablePhaseBehavior, ConstantProjection,
    ConstantProjectionKind, ConstantTermData, ConstantTermId, ConstantValueId, ConstantValueKind,
    DependencyContractTemplateId, DependencyGuard, DependencyProjection, DependencyRequirement,
    DependencySubject, DependencySubjectRoot, GenericArgument, GenericSubstitutionId,
    ImplementationInstanceId, SemanticValueStore, SymbolGraph, TraitApplicationId, TypeData,
    TypeId,
};

use crate::fact::FactQueryError;

pub(in crate::compilation::product) fn structural_type_identity(
    values: &SemanticValueStore,
    symbols: &SymbolGraph,
    ty: TypeId,
) -> Result<[u8; 32], FactQueryError> {
    StructuralValueEncoder::type_identity(values, symbols, ty)
}

pub(super) struct StructuralValueEncoder<'a> {
    values: &'a SemanticValueStore,
    symbols: &'a SymbolGraph,
    pub(super) digest: StableDigestHasher,
}

impl<'a> StructuralValueEncoder<'a> {
    fn type_identity(
        values: &'a SemanticValueStore,
        symbols: &'a SymbolGraph,
        ty: TypeId,
    ) -> Result<[u8; 32], FactQueryError> {
        let CodegenGenericArgument::Type(key) =
            Self::argument_key(values, symbols, GenericArgument::Type(ty))?
        else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        Ok(key.digest())
    }

    pub(super) fn argument_key(
        values: &'a SemanticValueStore,
        symbols: &'a SymbolGraph,
        argument: GenericArgument,
    ) -> Result<CodegenGenericArgument, FactQueryError> {
        let (domain, kind) = match argument {
            GenericArgument::Type(_) => (
                b"bray.codegen-concrete-type.v1".as_slice(),
                CodegenArgumentKind::Type,
            ),
            GenericArgument::Constant(_) => (
                b"bray.codegen-concrete-constant.v1".as_slice(),
                CodegenArgumentKind::Constant,
            ),
        };

        let mut encoder = Self {
            values,
            symbols,
            digest: StableDigestHasher::new(),
        };

        encoder.bytes(domain);

        match argument {
            GenericArgument::Type(ty) => encoder.ty(ty)?,
            GenericArgument::Constant(term) => encoder.constant_term(term)?,
        }

        let key = CodegenValueKey::new(encoder.digest.finalize());

        Ok(match kind {
            CodegenArgumentKind::Type => CodegenGenericArgument::Type(key),
            CodegenArgumentKind::Constant => CodegenGenericArgument::Constant(key),
        })
    }

    fn ty(&mut self, id: TypeId) -> Result<(), FactQueryError> {
        let data = self
            .values
            .type_data(id)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        match data.as_ref() {
            TypeData::Error => self.tag(0),
            TypeData::Named {
                definition,
                substitution,
            } => {
                self.tag(1);
                self.symbol(definition.into_any())?;
                self.substitution(*substitution)?;
            }
            TypeData::TypeParameter(parameter) => {
                self.tag(2);
                self.symbol((*parameter).into())?;
            }
            TypeData::ContextualSelf(context) => {
                self.tag(3);
                self.symbol(context.symbol())?;
            }
            TypeData::TypeValuedMemberProjection {
                subject,
                application,
                member,
            } => {
                self.tag(4);
                self.ty(*subject)?;
                self.trait_application(*application)?;
                self.symbol((*member).into())?;
            }
            TypeData::Tuple(elements) => {
                self.tag(5);
                self.length(elements.len());

                for element in elements.iter().copied() {
                    self.ty(element)?;
                }
            }
            TypeData::Array { element, length } => {
                self.tag(6);
                self.ty(*element)?;
                self.constant_term(*length)?;
            }
            TypeData::Slice(element) => {
                self.tag(7);
                self.ty(*element)?;
            }
            TypeData::Generator(element) => {
                self.tag(8);
                self.ty(*element)?;
            }
            TypeData::Nullable(target) => {
                self.tag(9);
                self.ty(*target)?;
            }
            TypeData::Borrow { kind, target } => {
                self.tag(10);

                self.tag(match kind {
                    bray_symbols::BorrowKind::Shared => 0,
                    bray_symbols::BorrowKind::Mutable => 1,
                });

                self.ty(*target)?;
            }
            TypeData::TraitView(application) => {
                self.tag(11);
                self.trait_application(*application)?;
            }
            TypeData::OwnedIndirection { storage, target } => {
                self.tag(12);
                self.ty(*storage)?;
                self.ty(*target)?;
            }
            TypeData::Callable(callable) => {
                self.tag(13);
                self.length(callable.parameters().len());

                for parameter in callable.parameters() {
                    self.bytes(parameter.name().as_str().as_bytes());
                    self.callable_position(parameter.position());
                    self.callable_parameter_mode(parameter.mode());
                    self.ty(parameter.ty())?;
                }

                self.ty(callable.result())?;
                self.callable_constness(callable.constness());
                self.callable_trust(callable.trust());
                self.callable_abi(callable.abi());

                self.callable_phase_behavior(callable.phase_behaviors().invocation())?;

                match callable.phase_behaviors().deferred_execution() {
                    Some(behavior) => {
                        self.tag(1);
                        self.callable_phase_behavior(behavior)?;
                    }
                    None => self.tag(0),
                }
            }
        }

        Ok(())
    }

    fn constant_term(&mut self, id: ConstantTermId) -> Result<(), FactQueryError> {
        let data = self
            .values
            .constant_term_data(id)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        match data.as_ref() {
            ConstantTermData::Value(value) => {
                self.tag(0);
                self.constant_value(*value)?;
            }
            ConstantTermData::IntegerLiteral { ty, value } => {
                self.tag(1);
                self.target_sized_integer_type(*ty);
                self.integer(value);
            }
            ConstantTermData::Parameter(parameter) => {
                self.tag(2);
                self.symbol((*parameter).into())?;
            }
            ConstantTermData::TargetFact(definition) => {
                self.tag(3);
                self.symbol((*definition).into())?;
            }
            ConstantTermData::Unary { operation, operand } => {
                self.tag(4);
                self.constant_unary_operation(*operation);
                self.constant_term(*operand)?;
            }
            ConstantTermData::Binary {
                operation,
                left,
                right,
            } => {
                self.tag(5);
                self.constant_binary_operation(*operation);
                self.constant_term(*left)?;
                self.constant_term(*right)?;
            }
            ConstantTermData::Conversion { operand, target } => {
                self.tag(6);
                self.constant_term(*operand)?;
                self.ty(*target)?;
            }
            ConstantTermData::NullablePresent(value) => {
                self.tag(7);
                self.constant_term(*value)?;
            }
            ConstantTermData::Tuple(values) => {
                self.tag(8);
                self.constant_terms(values)?;
            }
            ConstantTermData::Array(values) => {
                self.tag(9);
                self.constant_terms(values)?;
            }
            ConstantTermData::Product(fields) => {
                self.tag(10);
                self.length(fields.len());

                for field in fields.iter() {
                    self.symbol((*field.field()).into())?;
                    self.constant_term(*field.value())?;
                }
            }
            ConstantTermData::Union { variant, fields } => {
                self.tag(11);
                self.symbol((*variant).into())?;
                self.length(fields.len());

                for field in fields.iter() {
                    self.symbol((*field.field()).into())?;
                    self.constant_term(*field.value())?;
                }
            }
            ConstantTermData::DefinitionApplication {
                definition,
                substitution,
                selected_implementation,
            } => {
                self.tag(12);
                self.symbol(definition.into_any())?;
                self.substitution(*substitution)?;
                self.optional_implementation(*selected_implementation)?;
            }
            ConstantTermData::Call {
                callable,
                selected_implementation,
                arguments,
            } => {
                self.tag(13);
                self.callable_instance(*callable)?;
                self.optional_implementation(*selected_implementation)?;
                self.constant_terms(arguments)?;
            }
            ConstantTermData::Projection(projection) => {
                self.tag(14);
                self.constant_projection(*projection)?;
            }
        }

        Ok(())
    }

    fn constant_value(&mut self, id: ConstantValueId) -> Result<(), FactQueryError> {
        let data = self
            .values
            .constant_value_data(id)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        self.ty(data.ty())?;

        match data.kind() {
            ConstantValueKind::Error => self.tag(0),
            ConstantValueKind::Boolean(value) => {
                self.tag(1);
                self.boolean(*value);
            }
            ConstantValueKind::Character(value) => {
                self.tag(2);
                self.digest.write_u32(u32::from(*value));
            }
            ConstantValueKind::Integer(value) => {
                self.tag(3);
                self.integer(value);
            }
            ConstantValueKind::Real(value) => {
                self.tag(4);
                self.real(*value);
            }
            ConstantValueKind::Complex { real, imaginary } => {
                self.tag(5);
                self.real(*real);
                self.real(*imaginary);
            }
            ConstantValueKind::String(value) => {
                self.tag(6);
                self.bytes(value.as_bytes());
            }
            ConstantValueKind::Unit => self.tag(7),
            ConstantValueKind::NullableAbsent => self.tag(8),
            ConstantValueKind::NullablePresent(value) => {
                self.tag(9);
                self.constant_value(*value)?;
            }
            ConstantValueKind::Tuple(values) => {
                self.tag(10);
                self.constant_values(values)?;
            }
            ConstantValueKind::Array(values) => {
                self.tag(11);
                self.constant_values(values)?;
            }
            ConstantValueKind::Product(fields) => {
                self.tag(12);
                self.length(fields.len());

                for field in fields.iter() {
                    self.symbol((*field.field()).into())?;
                    self.constant_value(*field.value())?;
                }
            }
            ConstantValueKind::Union { variant, fields } => {
                self.tag(13);
                self.symbol((*variant).into())?;
                self.length(fields.len());

                for field in fields.iter() {
                    self.symbol((*field.field()).into())?;
                    self.constant_value(*field.value())?;
                }
            }
        }

        Ok(())
    }

    fn substitution(&mut self, id: GenericSubstitutionId) -> Result<(), FactQueryError> {
        let data = self
            .values
            .generic_substitution_data(id)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        self.symbol(data.owner().symbol())?;
        self.length(data.bindings().len());

        for binding in data.bindings() {
            self.symbol(binding.parameter().into_any())?;

            match binding.argument() {
                GenericArgument::Type(ty) => {
                    self.tag(0);
                    self.ty(ty)?;
                }
                GenericArgument::Constant(term) => {
                    self.tag(1);
                    self.constant_term(term)?;
                }
            }
        }

        Ok(())
    }

    fn trait_application(&mut self, id: TraitApplicationId) -> Result<(), FactQueryError> {
        let data = self
            .values
            .trait_application_data(id)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        self.symbol(data.definition().into())?;

        self.substitution(data.substitution())
    }

    fn callable_instance(&mut self, id: CallableInstanceId) -> Result<(), FactQueryError> {
        let data = self
            .values
            .callable_instance_data(id)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        self.symbol(data.definition().symbol())?;

        self.substitution(data.substitution())
    }

    fn optional_implementation(
        &mut self,
        id: Option<ImplementationInstanceId>,
    ) -> Result<(), FactQueryError> {
        match id {
            Some(id) => {
                self.tag(1);

                self.implementation_instance(id)
            }
            None => {
                self.tag(0);

                Ok(())
            }
        }
    }

    fn implementation_instance(
        &mut self,
        id: ImplementationInstanceId,
    ) -> Result<(), FactQueryError> {
        let data = self
            .values
            .implementation_instance_data(id)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        self.symbol(data.definition().into_any())?;

        self.substitution(data.substitution())
    }

    fn callable_phase_behavior(
        &mut self,
        behavior: &CallablePhaseBehavior,
    ) -> Result<(), FactQueryError> {
        self.length(behavior.effects().len());

        for requirement in behavior.effects() {
            self.symbol(requirement.declaration())?;
        }

        self.length(behavior.capabilities().len());

        for requirement in behavior.capabilities() {
            self.symbol(requirement.declaration())?;
        }

        self.length(behavior.trusted_capabilities().len());

        for requirement in behavior.trusted_capabilities() {
            self.ordinal(requirement.ordinal());
            self.symbol(requirement.capability().into())?;
        }

        self.length(behavior.execution_requirements().len());

        for requirement in behavior.execution_requirements() {
            self.symbol(requirement.declaration())?;
        }

        self.length(behavior.lifecycle_obligations().len());

        for obligation in behavior.lifecycle_obligations() {
            self.lifecycle_obligation(*obligation);
        }

        self.dependency_contract(behavior.dependency_contract())?;
        self.current_run_cancellation(behavior.current_run_cancellation());

        Ok(())
    }

    fn dependency_contract(
        &mut self,
        id: DependencyContractTemplateId,
    ) -> Result<(), FactQueryError> {
        let data = self
            .values
            .dependency_contract_template_data(id)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        self.length(data.requirements().len());

        for requirement in data.requirements() {
            self.dependency_requirement(requirement)?;
        }

        Ok(())
    }

    fn dependency_requirement(
        &mut self,
        requirement: &DependencyRequirement,
    ) -> Result<(), FactQueryError> {
        match requirement {
            DependencyRequirement::Direct { subject, kind } => {
                self.tag(0);
                self.dependency_subject(subject)?;
                self.dependency_requirement_kind(*kind);
            }
            DependencyRequirement::Guarded(guarded) => {
                self.tag(1);
                self.dependency_guard(guarded.guard())?;
                self.length(guarded.requirements().len());

                for requirement in guarded.requirements() {
                    self.dependency_requirement(requirement)?;
                }
            }
        }

        Ok(())
    }

    fn dependency_guard(&mut self, guard: &DependencyGuard) -> Result<(), FactQueryError> {
        match guard {
            DependencyGuard::NullablePresent(subject) => {
                self.tag(0);
                self.dependency_subject(subject)?;
            }
            DependencyGuard::ActiveUnionVariant { subject, variant } => {
                self.tag(1);
                self.dependency_subject(subject)?;
                self.symbol((*variant).into())?;
            }
        }

        Ok(())
    }

    fn dependency_subject(&mut self, subject: &DependencySubject) -> Result<(), FactQueryError> {
        match subject.subject_root() {
            DependencySubjectRoot::Receiver => self.tag(0),
            DependencySubjectRoot::Parameter(ordinal) => {
                self.tag(1);
                self.ordinal(ordinal);
            }
            DependencySubjectRoot::Result => self.tag(2),
            DependencySubjectRoot::ScopedCapability(ordinal) => {
                self.tag(3);
                self.ordinal(ordinal);
            }
            DependencySubjectRoot::ImplementationWitness(instance) => {
                self.tag(4);
                self.implementation_instance(instance)?;
            }
        }

        self.length(subject.projections().len());

        for projection in subject.projections() {
            match projection {
                DependencyProjection::ProductField(field) => {
                    self.tag(0);
                    self.symbol((*field).into())?;
                }
                DependencyProjection::TupleElement(ordinal) => {
                    self.tag(1);
                    self.ordinal(*ordinal);
                }
                DependencyProjection::Element(index) => {
                    self.tag(2);
                    self.constant_term(*index)?;
                }
                DependencyProjection::NullableValue => self.tag(3),
                DependencyProjection::UnionPayloadField(field) => {
                    self.tag(4);
                    self.symbol((*field).into())?;
                }
                DependencyProjection::OwnedTarget => self.tag(5),
            }
        }

        Ok(())
    }

    fn constant_projection(
        &mut self,
        projection: ConstantProjection,
    ) -> Result<(), FactQueryError> {
        self.constant_term(projection.subject())?;

        match projection.kind() {
            ConstantProjectionKind::TupleElement(ordinal) => {
                self.tag(0);
                self.ordinal(ordinal);
            }
            ConstantProjectionKind::ArrayElement(index) => {
                self.tag(1);
                self.constant_term(index)?;
            }
            ConstantProjectionKind::ProductField(field) => {
                self.tag(2);
                self.symbol(field.into())?;
            }
            ConstantProjectionKind::UnionPayloadField(field) => {
                self.tag(3);
                self.symbol(field.into())?;
            }
            ConstantProjectionKind::NullableValue => self.tag(4),
        }

        Ok(())
    }

    fn constant_terms(&mut self, terms: &[ConstantTermId]) -> Result<(), FactQueryError> {
        self.length(terms.len());

        for term in terms.iter().copied() {
            self.constant_term(term)?;
        }

        Ok(())
    }

    fn constant_values(&mut self, values: &[ConstantValueId]) -> Result<(), FactQueryError> {
        self.length(values.len());

        for value in values.iter().copied() {
            self.constant_value(value)?;
        }

        Ok(())
    }

    fn symbol(&mut self, id: AnySymbolId) -> Result<(), FactQueryError> {
        let key = self
            .symbols
            .symbol_key(id)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        key.hash(&mut self.digest);

        Ok(())
    }
}

enum CodegenArgumentKind {
    Type,
    Constant,
}
