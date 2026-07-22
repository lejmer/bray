use std::collections::BTreeSet;

use bray_symbols::InterfaceSymbolId;

use super::super::{contract, directory, facts, surface, value};
use super::model::SelectedRecords;
use super::remap::remap_selected_records;
use crate::semantic::codec::common::SemanticDecodeContext;
use crate::semantic::model::{
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantTermId,
    InterfaceConstantValueId, InterfaceConstantValueKind, InterfaceDependencyGuard,
    InterfaceDependencyProjection, InterfaceDependencyRequirement,
    InterfaceDependencyRequirementValue, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceGenericArgument, InterfaceGenericSubstitutionId,
    InterfaceImplementationInstanceId, InterfaceSemanticFactEntry, InterfaceSemanticFactKind,
    InterfaceSemanticFacts, InterfaceTraitApplicationId, InterfaceType, InterfaceTypeId,
};
use crate::{
    InterfaceSectionTag, InterfaceSymbolReference, InterfaceValidationError,
    InterfaceValidationLimits, PackageInterfaceSurface, ValidatedInterfaceSection,
};

pub(in crate::semantic::codec::decoding) fn decode_selected_fact_graph(
    sections: &[ValidatedInterfaceSection<'_>],
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
    kind: InterfaceSemanticFactKind,
    limits: InterfaceValidationLimits,
) -> Result<InterfaceSemanticFacts, InterfaceValidationError> {
    let mut context = SemanticDecodeContext::new(limits);

    let directory = facts::required_section(sections, InterfaceSectionTag::SymbolFactDirectory)?;
    let directory = directory::decode_fact_directory(directory, limits, &mut context)?;

    let tables = SelectedTables::read(sections, kind, &mut context)?;
    let mut builder = SelectionBuilder::new(tables, context, owner);

    builder.include_requested_facts(&directory, kind)?;

    let facts = remap_selected_records(builder.records)?;

    facts.validate(surface, limits)?;

    Ok(facts)
}

struct SelectedTables<'bytes> {
    types: value::TypeRecordTables<'bytes>,
    constants: value::ConstantRecordTables<'bytes>,
    contracts: contract::ContractRecordTables<'bytes>,
    implementations: Option<surface::ImplementationRecordTables<'bytes>>,
    targets: Option<surface::TargetRecordTables<'bytes>>,
}

impl<'bytes> SelectedTables<'bytes> {
    fn read(
        sections: &'bytes [ValidatedInterfaceSection<'bytes>],
        kind: InterfaceSemanticFactKind,
        context: &mut SemanticDecodeContext,
    ) -> Result<Self, InterfaceValidationError> {
        let types = facts::required_section(sections, InterfaceSectionTag::SemanticTypes)?;
        let constants = facts::required_section(sections, InterfaceSectionTag::Constants)?;
        let contracts = facts::required_section(sections, InterfaceSectionTag::Contracts)?;

        let types = value::decode_type_tables(types, context)?;
        let constants = value::decode_constant_tables(constants, context)?;
        let contracts = contract::decode_contract_tables(contracts, context)?;

        let implementations = if kind == InterfaceSemanticFactKind::Implementation {
            let section = facts::required_section(sections, InterfaceSectionTag::Implementations)?;

            Some(surface::decode_implementation_tables(section, context)?)
        } else {
            None
        };

        let targets = if matches!(
            kind,
            InterfaceSemanticFactKind::Implementation | InterfaceSemanticFactKind::TargetFact
        ) {
            let section =
                facts::required_section(sections, InterfaceSectionTag::TargetDependencies)?;

            Some(surface::decode_target_tables(section, context)?)
        } else {
            None
        };

        Ok(Self {
            types,
            constants,
            contracts,
            implementations,
            targets,
        })
    }
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum RecordKind {
    Substitution,
    TraitApplication,
    CallableInstance,
    ImplementationInstance,
    Type,
    ConstantValue,
    ConstantTerm,
    DependencyContract,
}

struct SelectionBuilder<'bytes> {
    tables: SelectedTables<'bytes>,
    context: SemanticDecodeContext,
    owner: InterfaceSymbolReference,
    visiting: BTreeSet<(RecordKind, u32)>,
    records: SelectedRecords,
}

impl<'bytes> SelectionBuilder<'bytes> {
    fn new(
        tables: SelectedTables<'bytes>,
        context: SemanticDecodeContext,
        owner: InterfaceSymbolId,
    ) -> Self {
        Self {
            tables,
            context,
            owner: InterfaceSymbolReference::Local(owner),
            visiting: BTreeSet::new(),
            records: SelectedRecords::new(),
        }
    }

    fn include_requested_facts(
        &mut self,
        directory: &[InterfaceSemanticFactEntry],
        kind: InterfaceSemanticFactKind,
    ) -> Result<(), InterfaceValidationError> {
        if matches!(
            kind,
            InterfaceSemanticFactKind::GenericConstraint
                | InterfaceSemanticFactKind::Implementation
        ) {
            for index in self.record_indexes(
                directory,
                InterfaceSemanticFactKind::GenericConstraint,
                InterfaceSectionTag::Contracts,
            )? {
                self.include_constraint(index)?;
            }
        }

        if kind == InterfaceSemanticFactKind::Implementation {
            let indexes = self.record_indexes(
                directory,
                InterfaceSemanticFactKind::Implementation,
                InterfaceSectionTag::Implementations,
            )?;

            let [index] = indexes.as_slice() else {
                return Err(InterfaceValidationError::Malformed);
            };

            self.include_implementation(*index)?;
        }

        if matches!(
            kind,
            InterfaceSemanticFactKind::Implementation | InterfaceSemanticFactKind::TargetFact
        ) {
            for index in self.record_indexes(
                directory,
                InterfaceSemanticFactKind::TargetFact,
                InterfaceSectionTag::TargetDependencies,
            )? {
                self.include_target(index)?;
            }
        }

        Ok(())
    }

    fn record_indexes(
        &self,
        directory: &[InterfaceSemanticFactEntry],
        kind: InterfaceSemanticFactKind,
        section: InterfaceSectionTag,
    ) -> Result<Vec<u32>, InterfaceValidationError> {
        let entries = directory
            .iter()
            .filter(|entry| entry.owner() == &self.owner && entry.kind() == kind);

        let mut indexes = Vec::new();

        for entry in entries {
            if entry.section() != section {
                return Err(InterfaceValidationError::Malformed);
            }

            indexes.push(entry.record());
        }

        Ok(indexes)
    }

    fn include_constraint(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        if self.records.constraints.contains(index) {
            return Ok(());
        }

        let constraint = self.tables.contracts.constraints.decode(
            index,
            &mut self.context,
            contract::decode_constraint,
        )?;

        if constraint.owner != self.owner {
            return Err(InterfaceValidationError::Malformed);
        }

        self.include_dependency_contract(constraint.predicate.dependency_contract)?;
        self.records.constraints.insert(index, constraint);

        Ok(())
    }

    fn include_implementation(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        if self.records.implementations.contains(index) {
            return Ok(());
        }

        let tables = self
            .tables
            .implementations
            .as_ref()
            .ok_or(InterfaceValidationError::Malformed)?;

        let decoded =
            tables
                .implementations
                .decode(index, &mut self.context, |reader, context| {
                    surface::decode_implementation_record(reader, context.limits(), context)
                })?;

        if decoded.implementation.implementation != self.owner {
            return Err(InterfaceValidationError::Malformed);
        }

        self.include_type(decoded.implementation.subject)?;

        if let Some(application) = decoded.implementation.trait_application {
            self.include_trait_application(application)?;
        }

        for coherence in decoded.coherence {
            self.include_coherence(coherence)?;
        }

        self.records
            .implementations
            .insert(index, decoded.implementation);

        Ok(())
    }

    fn include_coherence(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        if self.records.coherence.contains(index) {
            return Ok(());
        }

        let tables = self
            .tables
            .implementations
            .as_ref()
            .ok_or(InterfaceValidationError::Malformed)?;

        let coherence = tables
            .coherence
            .decode(index, &mut self.context, |reader, context| {
                surface::decode_coherence_record(reader, context.limits(), context)
            })?;

        if !coherence.implementations.contains(&self.owner) {
            return Err(InterfaceValidationError::Malformed);
        }

        self.include_type(coherence.subject)?;
        self.include_trait_application(coherence.trait_application)?;

        self.records.coherence.insert(index, coherence);

        Ok(())
    }

    fn include_target(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        if self.records.target_dependencies.contains(index) {
            return Ok(());
        }

        let tables = self
            .tables
            .targets
            .as_ref()
            .ok_or(InterfaceValidationError::Malformed)?;

        let target =
            tables
                .targets
                .decode(index, &mut self.context, surface::decode_target_record)?;

        if target.owner != self.owner {
            return Err(InterfaceValidationError::Malformed);
        }

        self.include_constant_value(target.value)?;
        self.records.target_dependencies.insert(index, target);

        Ok(())
    }

    fn include_substitution(
        &mut self,
        id: InterfaceGenericSubstitutionId,
    ) -> Result<(), InterfaceValidationError> {
        let index = id.raw();

        if self.records.substitutions.contains(index)
            || !self.visiting.insert((RecordKind::Substitution, index))
        {
            return Ok(());
        }

        let substitution = self.tables.types.substitutions.decode(
            index,
            &mut self.context,
            |reader, context| value::decode_substitution(reader, context.limits(), context),
        )?;

        for binding in &*substitution.bindings {
            match binding.argument {
                InterfaceGenericArgument::Type(ty) => self.include_type(ty)?,
                InterfaceGenericArgument::Constant(term) => self.include_constant_term(term)?,
            }
        }

        self.visiting.remove(&(RecordKind::Substitution, index));
        self.records.substitutions.insert(index, substitution);

        Ok(())
    }

    fn include_trait_application(
        &mut self,
        id: InterfaceTraitApplicationId,
    ) -> Result<(), InterfaceValidationError> {
        let index = id.raw();

        if self.records.trait_applications.contains(index)
            || !self.visiting.insert((RecordKind::TraitApplication, index))
        {
            return Ok(());
        }

        let application = self.tables.types.trait_applications.decode(
            index,
            &mut self.context,
            value::decode_trait_application,
        )?;

        self.include_substitution(application.substitution)?;

        self.visiting.remove(&(RecordKind::TraitApplication, index));
        self.records.trait_applications.insert(index, application);

        Ok(())
    }

    fn include_callable_instance(
        &mut self,
        id: crate::InterfaceCallableInstanceId,
    ) -> Result<(), InterfaceValidationError> {
        let index = id.raw();

        if self.records.callable_instances.contains(index)
            || !self.visiting.insert((RecordKind::CallableInstance, index))
        {
            return Ok(());
        }

        let instance = self.tables.types.callable_instances.decode(
            index,
            &mut self.context,
            value::decode_callable_instance,
        )?;

        self.include_substitution(instance.substitution)?;

        self.visiting.remove(&(RecordKind::CallableInstance, index));
        self.records.callable_instances.insert(index, instance);

        Ok(())
    }

    fn include_implementation_instance(
        &mut self,
        id: InterfaceImplementationInstanceId,
    ) -> Result<(), InterfaceValidationError> {
        let index = id.raw();

        if self.records.implementation_instances.contains(index)
            || !self
                .visiting
                .insert((RecordKind::ImplementationInstance, index))
        {
            return Ok(());
        }

        let instance = self.tables.types.implementation_instances.decode(
            index,
            &mut self.context,
            value::decode_implementation_instance,
        )?;

        self.include_substitution(instance.substitution)?;

        self.visiting
            .remove(&(RecordKind::ImplementationInstance, index));

        self.records
            .implementation_instances
            .insert(index, instance);

        Ok(())
    }

    fn include_type(&mut self, id: InterfaceTypeId) -> Result<(), InterfaceValidationError> {
        let index = id.raw();

        if self.records.types.contains(index) || !self.visiting.insert((RecordKind::Type, index)) {
            return Ok(());
        }

        let ty = self
            .tables
            .types
            .types
            .decode(index, &mut self.context, |reader, context| {
                value::decode_type(reader, context.limits(), context)
            })?;

        match &ty {
            InterfaceType::Named { substitution, .. } => {
                self.include_substitution(*substitution)?;
            }
            InterfaceType::AssociatedTypeProjection {
                subject,
                application,
                ..
            } => {
                self.include_type(*subject)?;
                self.include_trait_application(*application)?;
            }
            InterfaceType::Tuple(elements) => {
                for element in &**elements {
                    self.include_type(*element)?;
                }
            }
            InterfaceType::Array { element, length } => {
                self.include_type(*element)?;
                self.include_constant_term(*length)?;
            }
            InterfaceType::Slice(target)
            | InterfaceType::Nullable(target)
            | InterfaceType::Borrow { target, .. } => self.include_type(*target)?,
            InterfaceType::TraitView(application) => {
                self.include_trait_application(*application)?;
            }
            InterfaceType::OwnedIndirection { storage, target } => {
                self.include_type(*storage)?;
                self.include_type(*target)?;
            }
            InterfaceType::Callable {
                parameters,
                result,
                invocation_dependency_contract,
                deferred_dependency_contract,
                ..
            } => {
                for parameter in &**parameters {
                    self.include_type(parameter.ty)?;
                }

                self.include_type(*result)?;
                self.include_dependency_contract(*invocation_dependency_contract)?;

                if let Some(contract) = deferred_dependency_contract {
                    self.include_dependency_contract(*contract)?;
                }
            }
            InterfaceType::TypeParameter(_) | InterfaceType::ContextualSelf(_) => {}
        }

        self.visiting.remove(&(RecordKind::Type, index));
        self.records.types.insert(index, ty);

        Ok(())
    }

    fn include_constant_value(
        &mut self,
        id: InterfaceConstantValueId,
    ) -> Result<(), InterfaceValidationError> {
        let index = id.raw();

        if self.records.constant_values.contains(index)
            || !self.visiting.insert((RecordKind::ConstantValue, index))
        {
            return Ok(());
        }

        let value =
            self.tables
                .constants
                .values
                .decode(index, &mut self.context, |reader, context| {
                    value::decode_constant_value_record(reader, context.limits(), context)
                })?;

        self.include_type(value.ty)?;

        match &value.kind {
            InterfaceConstantValueKind::NullablePresent(value) => {
                self.include_constant_value(*value)?;
            }
            InterfaceConstantValueKind::Tuple(values)
            | InterfaceConstantValueKind::Array(values)
            | InterfaceConstantValueKind::Product(values) => {
                for value in &**values {
                    self.include_constant_value(*value)?;
                }
            }
            InterfaceConstantValueKind::Union { fields, .. } => {
                for field in &**fields {
                    self.include_constant_value(*field)?;
                }
            }
            InterfaceConstantValueKind::Boolean(_)
            | InterfaceConstantValueKind::Character(_)
            | InterfaceConstantValueKind::Integer(_)
            | InterfaceConstantValueKind::Real(_)
            | InterfaceConstantValueKind::Complex { .. }
            | InterfaceConstantValueKind::String(_)
            | InterfaceConstantValueKind::Unit
            | InterfaceConstantValueKind::NullableAbsent => {}
        }

        self.visiting.remove(&(RecordKind::ConstantValue, index));
        self.records.constant_values.insert(index, value);

        Ok(())
    }

    fn include_constant_term(
        &mut self,
        id: InterfaceConstantTermId,
    ) -> Result<(), InterfaceValidationError> {
        let index = id.raw();

        if self.records.constant_terms.contains(index)
            || !self.visiting.insert((RecordKind::ConstantTerm, index))
        {
            return Ok(());
        }

        let term = self.tables.constants.terms.decode(
            index,
            &mut self.context,
            value::decode_constant_term,
        )?;

        match &term {
            InterfaceConstantTerm::Value(value) => self.include_constant_value(*value)?,
            InterfaceConstantTerm::Unary { operand, .. } => self.include_constant_term(*operand)?,
            InterfaceConstantTerm::Binary { left, right, .. } => {
                self.include_constant_term(*left)?;
                self.include_constant_term(*right)?;
            }
            InterfaceConstantTerm::DefinitionApplication {
                substitution,
                selected_implementation,
                ..
            } => {
                self.include_substitution(*substitution)?;

                if let Some(implementation) = selected_implementation {
                    self.include_implementation_instance(*implementation)?;
                }
            }
            InterfaceConstantTerm::Call {
                callable,
                arguments,
            } => {
                self.include_callable_instance(*callable)?;

                for argument in &**arguments {
                    self.include_constant_term(*argument)?;
                }
            }
            InterfaceConstantTerm::Projection { subject, kind } => {
                self.include_constant_term(*subject)?;

                if let InterfaceConstantProjection::ArrayElement(index) = kind {
                    self.include_constant_term(*index)?;
                }
            }
            InterfaceConstantTerm::IntegerLiteral { .. }
            | InterfaceConstantTerm::Parameter(_)
            | InterfaceConstantTerm::TargetFact(_) => {}
        }

        self.visiting.remove(&(RecordKind::ConstantTerm, index));
        self.records.constant_terms.insert(index, term);

        Ok(())
    }

    fn include_dependency_contract(
        &mut self,
        id: crate::InterfaceDependencyContractId,
    ) -> Result<(), InterfaceValidationError> {
        let index = id.raw();

        if self.records.dependency_contracts.contains(index)
            || !self
                .visiting
                .insert((RecordKind::DependencyContract, index))
        {
            return Ok(());
        }

        let contract = self.tables.contracts.dependencies.decode(
            index,
            &mut self.context,
            |reader, context| {
                contract::decode_dependency_contract(reader, context.limits(), context)
            },
        )?;

        for requirement in &*contract.requirements {
            self.include_dependency_requirement(requirement)?;
        }

        self.visiting
            .remove(&(RecordKind::DependencyContract, index));

        self.records.dependency_contracts.insert(index, contract);

        Ok(())
    }

    fn include_dependency_requirement(
        &mut self,
        requirement: &InterfaceDependencyRequirement,
    ) -> Result<(), InterfaceValidationError> {
        match &requirement.value {
            InterfaceDependencyRequirementValue::Direct { subject, .. } => {
                self.include_dependency_subject(subject)?;
            }
            InterfaceDependencyRequirementValue::Guarded {
                guard,
                requirements,
            } => {
                match guard {
                    InterfaceDependencyGuard::NullablePresent(subject)
                    | InterfaceDependencyGuard::ActiveUnionVariant { subject, .. } => {
                        self.include_dependency_subject(subject)?;
                    }
                }

                for requirement in &**requirements {
                    self.include_dependency_requirement(requirement)?;
                }
            }
        }

        Ok(())
    }

    fn include_dependency_subject(
        &mut self,
        subject: &InterfaceDependencySubject,
    ) -> Result<(), InterfaceValidationError> {
        if let InterfaceDependencySubjectRoot::ImplementationWitness(implementation) = subject.root
        {
            self.include_implementation_instance(implementation)?;
        }

        for projection in &*subject.projections {
            if let InterfaceDependencyProjection::Element(term) = projection {
                self.include_constant_term(*term)?;
            }
        }

        Ok(())
    }
}
