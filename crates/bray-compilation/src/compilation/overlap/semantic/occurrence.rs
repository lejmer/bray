use std::collections::BTreeSet;

use bray_symbols::{
    ConstantTermData, ConstantTermId, GenericArgument, GenericConstParameterSymbolId,
    GenericSubstitutionId, GenericTypeParameterSymbolId, SemanticValueStoreError, TypeData, TypeId,
};

use super::SemanticUnifier;

impl SemanticUnifier<'_> {
    pub(super) fn type_contains_parameter(
        &self,
        ty: TypeId,
        parameter: GenericTypeParameterSymbolId,
        visited: &mut BTreeSet<TypeId>,
    ) -> Result<bool, SemanticValueStoreError> {
        if !visited.insert(ty) {
            return Ok(false);
        }

        let ty = self.values.type_data(ty)?;

        match ty.as_ref() {
            TypeData::TypeParameter(candidate) => Ok(*candidate == parameter),
            TypeData::Named { substitution, .. } => {
                self.substitution_contains_type_parameter(*substitution, parameter, visited)
            }
            TypeData::TypeValuedMemberProjection {
                subject,
                application,
                ..
            } => {
                if self.type_contains_parameter(*subject, parameter, visited)? {
                    return Ok(true);
                }

                let application = self.values.trait_application_data(*application)?;

                self.substitution_contains_type_parameter(
                    application.substitution(),
                    parameter,
                    visited,
                )
            }
            TypeData::Tuple(elements) => {
                for element in &**elements {
                    if self.type_contains_parameter(*element, parameter, visited)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            TypeData::Array { element, .. }
            | TypeData::FlexibleArray(element)
            | TypeData::Slice(element)
            | TypeData::Generator(element)
            | TypeData::Nullable(element) => {
                self.type_contains_parameter(*element, parameter, visited)
            }
            TypeData::Borrow { target, .. } => {
                self.type_contains_parameter(*target, parameter, visited)
            }
            TypeData::TraitView(application) => {
                let application = self.values.trait_application_data(*application)?;

                self.substitution_contains_type_parameter(
                    application.substitution(),
                    parameter,
                    visited,
                )
            }
            TypeData::OwnedIndirection { storage, target } => {
                if self.type_contains_parameter(*storage, parameter, visited)? {
                    return Ok(true);
                }

                self.type_contains_parameter(*target, parameter, visited)
            }
            TypeData::Callable(callable) => {
                for callable_parameter in callable.parameters() {
                    if self.type_contains_parameter(callable_parameter.ty(), parameter, visited)? {
                        return Ok(true);
                    }
                }

                self.type_contains_parameter(callable.result(), parameter, visited)
            }
            TypeData::Error | TypeData::ContextualSelf(_) => Ok(false),
        }
    }

    fn substitution_contains_type_parameter(
        &self,
        substitution: GenericSubstitutionId,
        parameter: GenericTypeParameterSymbolId,
        visited: &mut BTreeSet<TypeId>,
    ) -> Result<bool, SemanticValueStoreError> {
        let substitution = self.values.generic_substitution_data(substitution)?;

        for binding in substitution.bindings() {
            if let GenericArgument::Type(ty) = binding.argument()
                && self.type_contains_parameter(ty, parameter, visited)?
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    pub(super) fn constant_contains_parameter(
        &self,
        term: ConstantTermId,
        parameter: GenericConstParameterSymbolId,
        visited: &mut BTreeSet<ConstantTermId>,
    ) -> Result<bool, SemanticValueStoreError> {
        if !visited.insert(term) {
            return Ok(false);
        }

        let term = self.values.constant_term_data(term)?;

        match term.as_ref() {
            ConstantTermData::Typed { term, .. } => {
                self.constant_contains_parameter(*term, parameter, visited)
            }
            ConstantTermData::Parameter(candidate) => Ok(*candidate == parameter),
            ConstantTermData::Unary { operand, .. }
            | ConstantTermData::Conversion { operand, .. }
            | ConstantTermData::NullablePresent(operand) => {
                self.constant_contains_parameter(*operand, parameter, visited)
            }
            ConstantTermData::Binary { left, right, .. } => {
                if self.constant_contains_parameter(*left, parameter, visited)? {
                    return Ok(true);
                }

                self.constant_contains_parameter(*right, parameter, visited)
            }
            ConstantTermData::Tuple(elements) | ConstantTermData::Array(elements) => {
                for element in &**elements {
                    if self.constant_contains_parameter(*element, parameter, visited)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            ConstantTermData::Product(fields) => {
                for field in &**fields {
                    if self.constant_contains_parameter(*field.value(), parameter, visited)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            ConstantTermData::Union { fields, .. } => {
                for field in &**fields {
                    if self.constant_contains_parameter(*field.value(), parameter, visited)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            ConstantTermData::DefinitionApplication { substitution, .. } => {
                self.substitution_contains_const_parameter(*substitution, parameter, visited)
            }
            ConstantTermData::Call {
                callable,
                arguments,
                ..
            } => {
                let callable = self.values.callable_instance_data(*callable)?;

                if self.substitution_contains_const_parameter(
                    callable.substitution(),
                    parameter,
                    visited,
                )? {
                    return Ok(true);
                }

                for argument in &**arguments {
                    if self.constant_contains_parameter(*argument, parameter, visited)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            ConstantTermData::PredicateCall {
                predicate,
                arguments,
            } => {
                if self.substitution_contains_const_parameter(
                    predicate.substitution(),
                    parameter,
                    visited,
                )? {
                    return Ok(true);
                }

                for argument in &**arguments {
                    if self.constant_contains_parameter(*argument, parameter, visited)? {
                        return Ok(true);
                    }
                }

                Ok(false)
            }
            ConstantTermData::Test { subject, .. } => {
                self.constant_contains_parameter(*subject, parameter, visited)
            }
            ConstantTermData::Projection(projection) => {
                self.constant_contains_parameter(projection.subject(), parameter, visited)
            }
            ConstantTermData::Value(_)
            | ConstantTermData::IntegerLiteral { .. }
            | ConstantTermData::CallableArgument(_)
            | ConstantTermData::TargetProperty(_) => Ok(false),
        }
    }

    fn substitution_contains_const_parameter(
        &self,
        substitution: GenericSubstitutionId,
        parameter: GenericConstParameterSymbolId,
        visited: &mut BTreeSet<ConstantTermId>,
    ) -> Result<bool, SemanticValueStoreError> {
        let substitution = self.values.generic_substitution_data(substitution)?;

        for binding in substitution.bindings() {
            if let GenericArgument::Constant(term) = binding.argument()
                && self.constant_contains_parameter(term, parameter, visited)?
            {
                return Ok(true);
            }
        }

        Ok(false)
    }
}
