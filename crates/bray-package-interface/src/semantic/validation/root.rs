use crate::{
    InterfaceLimit, InterfaceSemantics, InterfaceValidationError, InterfaceValidationLimits,
    PackageInterfaceSurface,
};

impl InterfaceSemantics {
    pub(crate) fn validate(
        &self,
        surface: &PackageInterfaceSurface,
        limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        for count in self.table_counts() {
            limits.check(InterfaceLimit::RecordCount, saturating_u64(count))?;
        }

        let symbol_count = surface.symbols().symbols().len();
        let dependency_count = surface.dependencies().len();

        self.validate_value_graph(symbol_count, dependency_count, limits)?;
        self.validate_surface_semantics(surface, symbol_count, dependency_count, limits)?;

        self.validate_template_semantics(surface, limits)
    }

    fn table_counts(&self) -> [usize; 23] {
        [
            self.substitutions.len(),
            self.trait_applications.len(),
            self.callable_instances.len(),
            self.implementation_instances.len(),
            self.dependency_contracts.len(),
            self.types.len(),
            self.constant_values.len(),
            self.constant_terms.len(),
            self.constraints.len(),
            self.callable_contracts.len(),
            self.callable_signatures.len(),
            self.generic_declarations.len(),
            self.callable_parameter_defaults.len(),
            self.predicate_definitions.len(),
            self.type_representations.len(),
            self.checked_templates.len(),
            self.declaration_templates.len(),
            self.support_entities.len(),
            self.implementations.len(),
            self.coherence.len(),
            self.target_dependencies.len(),
            self.abi_dependencies.len(),
            self.runtime_requirements.len(),
        ]
    }
}

pub(in crate::semantic) fn saturating_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

pub(in crate::semantic) fn checked_index(
    index: Option<usize>,
    length: usize,
) -> Result<usize, InterfaceValidationError> {
    match index {
        Some(index) if index < length => Ok(index),
        _ => Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Discriminant,
        )),
    }
}
