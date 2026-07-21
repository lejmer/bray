use bray_symbols::{
    CallableDependencyContracts, DependencyContractTemplateId, DependencyGuard,
    DependencyProjection, DependencyRequirement, DependencySubject, DependencySubjectRoot,
    SemanticValueStoreError,
};

use super::header::HeaderMatcher;

impl HeaderMatcher<'_> {
    pub(super) fn match_dependency_contracts(
        &mut self,
        pattern: CallableDependencyContracts,
        actual: CallableDependencyContracts,
    ) -> Result<bool, SemanticValueStoreError> {
        if !self.match_dependency_template(pattern.invocation(), actual.invocation())? {
            return Ok(false);
        }

        match (pattern.deferred_execution(), actual.deferred_execution()) {
            (Some(pattern), Some(actual)) => self.match_dependency_template(pattern, actual),
            (None, None) => Ok(true),
            (Some(_), None) | (None, Some(_)) => Ok(false),
        }
    }

    fn match_dependency_template(
        &mut self,
        pattern: DependencyContractTemplateId,
        actual: DependencyContractTemplateId,
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern == actual {
            return Ok(true);
        }

        let pattern = self.values.dependency_contract_template_data(pattern)?;
        let actual = self.values.dependency_contract_template_data(actual)?;

        self.match_dependency_requirements(pattern.requirements(), actual.requirements())
    }

    fn match_dependency_requirements(
        &mut self,
        pattern: &[DependencyRequirement],
        actual: &[DependencyRequirement],
    ) -> Result<bool, SemanticValueStoreError> {
        if pattern.len() != actual.len() {
            return Ok(false);
        }

        let mut used = vec![false; actual.len()];

        self.match_dependency_requirement_from(pattern, actual, &mut used, 0)
    }

    fn match_dependency_requirement_from(
        &mut self,
        pattern: &[DependencyRequirement],
        actual: &[DependencyRequirement],
        used: &mut [bool],
        index: usize,
    ) -> Result<bool, SemanticValueStoreError> {
        let Some(pattern_requirement) = pattern.get(index) else {
            return Ok(true);
        };

        for (actual_index, actual_requirement) in actual.iter().enumerate() {
            if used[actual_index] {
                continue;
            }

            // Each speculative pairing owns its inferred arguments so failed paths can roll back.
            let previous_arguments = self.arguments.clone();

            if self.match_dependency_requirement(pattern_requirement, actual_requirement)? {
                used[actual_index] = true;

                if self.match_dependency_requirement_from(pattern, actual, used, index + 1)? {
                    return Ok(true);
                }

                used[actual_index] = false;
            }

            self.arguments = previous_arguments;
        }

        Ok(false)
    }

    fn match_dependency_requirement(
        &mut self,
        pattern: &DependencyRequirement,
        actual: &DependencyRequirement,
    ) -> Result<bool, SemanticValueStoreError> {
        match (pattern, actual) {
            (
                DependencyRequirement::Direct {
                    subject: pattern_subject,
                    kind: pattern_kind,
                },
                DependencyRequirement::Direct {
                    subject: actual_subject,
                    kind: actual_kind,
                },
            ) if pattern_kind == actual_kind => {
                self.match_dependency_subject(pattern_subject, actual_subject)
            }
            (DependencyRequirement::Guarded(pattern), DependencyRequirement::Guarded(actual)) => {
                if !self.match_dependency_guard(pattern.guard(), actual.guard())? {
                    return Ok(false);
                }

                self.match_dependency_requirements(pattern.requirements(), actual.requirements())
            }
            _ => Ok(false),
        }
    }

    fn match_dependency_guard(
        &mut self,
        pattern: &DependencyGuard,
        actual: &DependencyGuard,
    ) -> Result<bool, SemanticValueStoreError> {
        match (pattern, actual) {
            (
                DependencyGuard::NullablePresent(pattern),
                DependencyGuard::NullablePresent(actual),
            ) => self.match_dependency_subject(pattern, actual),
            (
                DependencyGuard::ActiveUnionVariant {
                    subject: pattern_subject,
                    variant: pattern_variant,
                },
                DependencyGuard::ActiveUnionVariant {
                    subject: actual_subject,
                    variant: actual_variant,
                },
            ) if pattern_variant == actual_variant => {
                self.match_dependency_subject(pattern_subject, actual_subject)
            }
            _ => Ok(false),
        }
    }

    fn match_dependency_subject(
        &mut self,
        pattern: &DependencySubject,
        actual: &DependencySubject,
    ) -> Result<bool, SemanticValueStoreError> {
        if !self.match_dependency_root(pattern.subject_root(), actual.subject_root())?
            || pattern.projections().len() != actual.projections().len()
        {
            return Ok(false);
        }

        for (pattern, actual) in pattern.projections().iter().zip(actual.projections()) {
            let matches = match (pattern, actual) {
                (DependencyProjection::Element(pattern), DependencyProjection::Element(actual)) => {
                    self.match_constant(*pattern, *actual)?
                }
                (pattern, actual) => pattern == actual,
            };

            if !matches {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn match_dependency_root(
        &mut self,
        pattern: DependencySubjectRoot,
        actual: DependencySubjectRoot,
    ) -> Result<bool, SemanticValueStoreError> {
        match (pattern, actual) {
            (
                DependencySubjectRoot::ImplementationWitness(pattern),
                DependencySubjectRoot::ImplementationWitness(actual),
            ) => self.match_implementation_instance(pattern, actual),
            (pattern, actual) => Ok(pattern == actual),
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        CallableDependencyContracts, ConstantTermData, DependencyContractTemplateData,
        DependencyProjection, DependencyRequirement, DependencyRequirementKind, DependencySubject,
        DependencySubjectRoot, GenericArgument, GenericConstParameterSymbolId,
        GenericParameterSymbolId, SemanticValueStore, SymbolId, SymbolOrdinal,
    };

    use super::HeaderMatcher;

    #[test]
    fn dependency_contracts_infer_nested_constant_parameters() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic value store must build: {error:?}"));

        let pattern_parameter = GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(1));
        let actual_parameter = GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let parameter = GenericParameterSymbolId::Const(pattern_parameter);

        let pattern_term = values
            .intern_constant_term(ConstantTermData::Parameter(pattern_parameter))
            .unwrap_or_else(|error| panic!("pattern term must be valid: {error:?}"));

        let actual_term = values
            .intern_constant_term(ConstantTermData::Parameter(actual_parameter))
            .unwrap_or_else(|error| panic!("actual term must be valid: {error:?}"));

        let pattern = dependency_contract(&values, pattern_term);
        let actual = dependency_contract(&values, actual_term);
        let mut matcher = HeaderMatcher::new(&[parameter], &values);

        assert_eq!(
            matcher.match_dependency_contracts(pattern, actual),
            Ok(true)
        );

        assert_eq!(
            matcher.arguments.get(&parameter),
            Some(&GenericArgument::Constant(actual_term))
        );
    }

    fn dependency_contract(
        values: &SemanticValueStore,
        element: bray_symbols::ConstantTermId,
    ) -> CallableDependencyContracts {
        let subject = DependencySubject::new(
            DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)),
            [DependencyProjection::Element(element)],
        );

        let requirement =
            DependencyRequirement::direct(subject, DependencyRequirementKind::StorageInitialized);

        let template = values
            .intern_dependency_contract_template(DependencyContractTemplateData::new([requirement]))
            .unwrap_or_else(|error| panic!("dependency template must be valid: {error:?}"));

        CallableDependencyContracts::synchronous(template)
    }
}
