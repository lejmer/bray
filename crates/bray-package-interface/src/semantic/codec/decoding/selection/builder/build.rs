// rust-style: allow(module-too-large, reason = "selective decoding shares one recursive dependency traversal and remapping state")

use std::collections::BTreeSet;

use bray_symbols::InterfaceSymbolId;

use super::tables::SelectedTables;
use crate::semantic::codec::common::SemanticDecodeContext;
use crate::semantic::codec::decoding::selection::model::SelectedRecords;
use crate::semantic::codec::decoding::selection::remap::{
    remap_selected_records, remap_selected_template_values,
};
use crate::semantic::codec::decoding::{bundle, contract, declaration, directory, surface, template, value};
use crate::semantic::model::{
    InterfaceCallablePhaseBehavior, InterfaceConstantTerm, InterfaceConstantValueKind,
    InterfaceDependencyGuard, InterfaceDependencyProjection, InterfaceDependencyRequirement,
    InterfaceDependencyRequirementValue, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceGenericArgument, InterfacePredicateSummary,
    InterfaceSemanticRecord, InterfaceSemanticRecordKind, InterfaceSemantics, InterfaceSupportEntity,
    InterfaceType,
};
use crate::{
    InterfaceSectionTag, InterfaceSymbolReference, InterfaceValidationError,
    PackageInterfaceSurface, ValidatedInterfaceSection,
};

pub(in crate::semantic::codec::decoding) fn decode_selected_record_graph(
    sections: &[ValidatedInterfaceSection<'_>],
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
    kind: InterfaceSemanticRecordKind,
    mut context: SemanticDecodeContext,
) -> Result<Option<InterfaceSemantics>, InterfaceValidationError> {
    let limits = context.limits();

    let directory =
        bundle::required_section(sections, InterfaceSectionTag::SemanticRecordDirectory)?;

    let directory = directory::decode_semantic_directory(directory, limits, &mut context)?;

    if kind == InterfaceSemanticRecordKind::CallableParameterDefault {
        return crate::semantic::codec::decoding::selection::declaration::decode_callable_parameter_default(
            sections, surface, owner, limits, context, &directory,
        ).map(Some);
    }

    if kind == InterfaceSemanticRecordKind::PredicateDefinition {
        return crate::semantic::codec::decoding::selection::declaration::decode_predicate_definition(
            sections, surface, owner, context, &directory,
        );
    }

    let tables = SelectedTables::read(sections, kind, &mut context)?;
    let mut builder = SelectionBuilder::new(tables, context, owner, &directory);

    builder.include_requested_semantics(&directory, kind)?;

    // Predicate templates pair with their definitions, which can reach the complete graph.
    if builder.records.declaration_templates.values().values()
        .any(|declaration| declaration.kind() == bray_bound_tree::CheckedTemplateKind::PredicateDefinition)
    {
        return Ok(None);
    }

    let semantics = remap_selected_records(builder.records)?;

    semantics.validate(surface, limits)?;

    Ok(Some(semantics))
}

/// Schedules every record that a selected template references, unchanged.
struct ReferenceCollector<'builder, 'bytes, 'records>(&'builder mut SelectionBuilder<'bytes, 'records>);

impl crate::semantic::model::InterfaceTemplateReferenceMap for ReferenceCollector<'_, '_, '_> {
    type Error = InterfaceValidationError;

    fn term(&mut self, term: crate::InterfaceConstantTermId) -> Result<crate::InterfaceConstantTermId, Self::Error> {
        self.0.enqueue(PendingRecord::ConstantTerm(term.raw()));

        Ok(term)
    }

    fn ty(&mut self, ty: crate::InterfaceTypeId) -> Result<crate::InterfaceTypeId, Self::Error> {
        self.0.enqueue(PendingRecord::Type(ty.raw()));

        Ok(ty)
    }

    fn substitution(&mut self, substitution: crate::InterfaceGenericSubstitutionId) -> Result<crate::InterfaceGenericSubstitutionId, Self::Error> {
        self.0.enqueue(PendingRecord::Substitution(substitution.raw()));

        Ok(substitution)
    }

    fn dependency(&mut self, contract: crate::InterfaceDependencyContractId) -> Result<crate::InterfaceDependencyContractId, Self::Error> {
        self.0.enqueue(PendingRecord::DependencyContract(contract.raw()));

        Ok(contract)
    }

    fn support(&mut self, entity: bray_symbols::InterfaceSupportEntityId) -> Result<bray_symbols::InterfaceSupportEntityId, Self::Error> {
        self.0.enqueue(PendingRecord::SupportEntity(entity.raw()));

        Ok(entity)
    }
}

/// Decodes the closure of the interface values that one executable template references.
pub(in crate::semantic::codec::decoding) fn decode_selected_template_values(
    sections: &[ValidatedInterfaceSection<'_>],
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
    references: &crate::ExecutableTemplateReferences,
    mut context: SemanticDecodeContext,
) -> Result<(InterfaceSemantics, crate::ExecutableTemplateSlots), InterfaceValidationError> {
    use crate::implementation::TemplateTable;

    let limits = context.limits();

    let directory =
        bundle::required_section(sections, InterfaceSectionTag::SemanticRecordDirectory)?;

    let directory = directory::decode_semantic_directory(directory, limits, &mut context)?;

    // Template values need only the value tables, which every selection reads.
    let tables = SelectedTables::read(
        sections,
        InterfaceSemanticRecordKind::GenericConstraint,
        &mut context,
    )?;

    let mut builder = SelectionBuilder::new(tables, context, owner, &directory);

    for (table, record) in [
        (TemplateTable::Type, PendingRecord::Type as fn(u32) -> PendingRecord),
        (TemplateTable::ConstantValue, PendingRecord::ConstantValue),
        (TemplateTable::ConstantTerm, PendingRecord::ConstantTerm),
        (TemplateTable::Substitution, PendingRecord::Substitution),
        (TemplateTable::TraitApplication, PendingRecord::TraitApplication),
        (TemplateTable::ImplementationInstance, PendingRecord::ImplementationInstance),
        (TemplateTable::DependencyContract, PendingRecord::DependencyContract),
    ] {
        for &slot in references.slots(table) {
            builder.enqueue(record(slot));
        }
    }

    builder.include_pending_records()?;

    let (semantics, slots) = remap_selected_template_values(builder.records)?;

    semantics.validate(surface, limits)?;

    Ok((semantics, slots))
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum PendingRecord {
    Constraint(u32),
    CallableSignature(u32, InterfaceSymbolId),
    CallableContract(u32, InterfaceSymbolId),
    // A callable instance named by execution evidence. A local target's
    // contract and signature join the proof graph that validation checks.
    ExecutionTarget(u32),
    GenericDeclaration(u32),
    DeclaredType(u32),
    TypeRepresentation(u32),
    Implementation(u32),
    Coherence(u32),
    Target(u32),
    Runtime(u32),
    Substitution(u32),
    TraitApplication(u32),
    CallableInstance(u32),
    ImplementationInstance(u32),
    Type(u32),
    ConstantValue(u32),
    ConstantTerm(u32),
    DependencyContract(u32),
    DeclarationTemplate(u32),
    SupportEntity(u32),
    CheckedTemplate(u32),
}

struct SelectionBuilder<'bytes, 'records> {
    tables: SelectedTables<'bytes>,
    context: SemanticDecodeContext,
    owner: InterfaceSymbolReference,
    pending: Vec<PendingRecord>,
    scheduled: BTreeSet<PendingRecord>,
    records: SelectedRecords,
    directory: &'records [InterfaceSemanticRecord],
}

impl<'bytes, 'records> SelectionBuilder<'bytes, 'records> {
    fn new(
        tables: SelectedTables<'bytes>,
        context: SemanticDecodeContext,
        owner: InterfaceSymbolId,
        directory: &'records [InterfaceSemanticRecord],
    ) -> Self {
        Self {
            tables,
            context,
            owner: InterfaceSymbolReference::Local(owner),
            pending: Vec::new(),
            scheduled: BTreeSet::new(),
            records: SelectedRecords::new(),
            directory,
        }
    }

    fn include_requested_semantics(
        &mut self,
        directory: &[InterfaceSemanticRecord],
        kind: InterfaceSemanticRecordKind,
    ) -> Result<(), InterfaceValidationError> {
        let InterfaceSymbolReference::Local(owner) = self.owner else {
            unreachable!("selected semantics belong to a local interface symbol");
        };

        if matches!(
            kind,
            InterfaceSemanticRecordKind::GenericConstraint
                | InterfaceSemanticRecordKind::GenericDeclaration
                | InterfaceSemanticRecordKind::Implementation
        ) {
            for index in self.record_indexes(
                directory,
                InterfaceSemanticRecordKind::GenericConstraint,
                InterfaceSectionTag::Contracts,
            )? {
                self.enqueue(PendingRecord::Constraint(index));
            }
        }

        if kind == InterfaceSemanticRecordKind::CallableSignature {
            let index = self.one_record_index(
                directory,
                InterfaceSemanticRecordKind::CallableSignature,
                InterfaceSectionTag::DeclarationSemantics,
            )?;

            self.enqueue(PendingRecord::CallableSignature(index, owner));
        }

        if kind == InterfaceSemanticRecordKind::CallableContracts {
            let index = self.one_record_index(directory, kind, InterfaceSectionTag::Contracts)?;

            self.enqueue(PendingRecord::CallableContract(index, owner));
        }

        if kind == InterfaceSemanticRecordKind::GenericDeclaration {
            let index = self.optional_record_index(
                directory,
                InterfaceSemanticRecordKind::GenericDeclaration,
                InterfaceSectionTag::DeclarationSemantics,
            )?;

            if let Some(index) = index {
                self.enqueue(PendingRecord::GenericDeclaration(index));
            }
        }

        if kind == InterfaceSemanticRecordKind::DeclaredType {
            let index = self.one_record_index(
                directory,
                InterfaceSemanticRecordKind::DeclaredType,
                InterfaceSectionTag::DeclarationSemantics,
            )?;

            self.enqueue(PendingRecord::DeclaredType(index));
        }

        if kind == InterfaceSemanticRecordKind::DeclarationTemplate {
            for index in self.record_indexes(
                directory,
                InterfaceSemanticRecordKind::DeclarationTemplate,
                InterfaceSectionTag::DeclarationTemplates,
            )? {
                self.enqueue(PendingRecord::DeclarationTemplate(index));
            }
        }

        if kind == InterfaceSemanticRecordKind::TypeRepresentation {
            for index in self.record_indexes(
                directory,
                InterfaceSemanticRecordKind::TypeRepresentation,
                InterfaceSectionTag::DeclarationSemantics,
            )? {
                self.enqueue(PendingRecord::TypeRepresentation(index));
            }
        }

        if kind == InterfaceSemanticRecordKind::Implementation {
            let indexes = self.record_indexes(
                directory,
                InterfaceSemanticRecordKind::Implementation,
                InterfaceSectionTag::Implementations,
            )?;

            let [index] = indexes.as_slice() else {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
            };

            self.enqueue(PendingRecord::Implementation(*index));
        }

        if matches!(
            kind,
            InterfaceSemanticRecordKind::Implementation
                | InterfaceSemanticRecordKind::TargetProperty
        ) {
            for index in self.record_indexes(
                directory,
                InterfaceSemanticRecordKind::TargetProperty,
                InterfaceSectionTag::TargetDependencies,
            )? {
                self.enqueue(PendingRecord::Target(index));
            }
        }

        if kind == InterfaceSemanticRecordKind::Runtime {
            let index = self.one_record_index(
                directory,
                InterfaceSemanticRecordKind::Runtime,
                InterfaceSectionTag::TargetDependencies,
            )?;

            self.enqueue(PendingRecord::Runtime(index));
        }

        self.include_pending_records()?;
        self.include_execution_signature_contracts()?;

        if !self.records.callable_signatures.values().is_empty()
            && self.records.dependency_contracts.values().values().any(|contract| {
            contract.captured_values.iter().flat_map(|row| row.ownership.transfers()).any(|transfer| !transfer.preservation().children().is_empty())
        }) {
            self.include_preservation_representations()?;
        }

        Ok(())
    }

    /// Execution validation pairs a signature whose callable type carries execution
    /// behavior with its owner's contract, which can reach further proof targets.
    fn include_execution_signature_contracts(&mut self) -> Result<(), InterfaceValidationError> {
        loop {
            let owners = self.records.callable_signatures.values().values().filter_map(|signature| {
                let InterfaceSymbolReference::Local(owner) = signature.owner else { return None; };

                let Some(InterfaceType::Callable { invocation_behavior, deferred_execution_behavior, .. }) =
                    self.records.types.values().get(&signature.callable_type.raw())
                else {
                    return None;
                };

                std::iter::once(invocation_behavior.as_ref()).chain(deferred_execution_behavior.as_deref())
                    .any(|behavior| !behavior.execution_properties.is_empty() || !behavior.execution_domains.is_empty())
                    .then_some(owner)
            }).collect::<Vec<_>>();

            for owner in owners {
                self.enqueue_owned_record(owner, InterfaceSemanticRecordKind::CallableContracts)?;
            }

            if self.pending.is_empty() {
                return Ok(());
            }

            self.include_pending_records()?;
        }
    }

    fn include_preservation_representations(&mut self) -> Result<(), InterfaceValidationError> {
        loop {
            let representations = self.records.types.values().values().filter_map(|ty| {
                let InterfaceType::Named { definition, .. } = ty else { return None; };

                Some(directory::records_for_owner(
                    self.directory, definition, InterfaceSemanticRecordKind::TypeRepresentation,
                ))
            }).flatten().cloned().collect::<Vec<_>>();

            for record in representations {
                if record.section() != InterfaceSectionTag::DeclarationSemantics {
                    return Err(crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Reference));
                }

                self.enqueue(PendingRecord::TypeRepresentation(record.record()));
            }

            if self.pending.is_empty() {
                return Ok(());
            }

            self.include_pending_records()?;
        }
    }

    fn enqueue(&mut self, record: PendingRecord) {
        if self.scheduled.insert(record) {
            self.pending.push(record);
        }
    }

    fn include_pending_records(&mut self) -> Result<(), InterfaceValidationError> {
        while let Some(record) = self.pending.pop() {
            match record {
                PendingRecord::Constraint(index) => self.include_constraint(index)?,
                PendingRecord::CallableSignature(index, owner) => {
                    self.include_callable_signature(index, owner)?;
                }
                PendingRecord::CallableContract(index, owner) => {
                    self.include_callable_contract(index, owner)?;
                }
                PendingRecord::ExecutionTarget(index) => self.include_execution_target(index)?,
                PendingRecord::GenericDeclaration(index) => {
                    self.include_generic_declaration(index)?;
                }
                PendingRecord::DeclaredType(index) => self.include_declared_type(index)?,
                PendingRecord::TypeRepresentation(index) => self.include_type_representation(index)?,
                PendingRecord::Implementation(index) => self.include_implementation(index)?,
                PendingRecord::Coherence(index) => self.include_coherence(index)?,
                PendingRecord::Target(index) => self.include_target(index)?,
                PendingRecord::Runtime(index) => self.include_runtime(index)?,
                PendingRecord::Substitution(index) => self.include_substitution(index)?,
                PendingRecord::TraitApplication(index) => self.include_trait_application(index)?,
                PendingRecord::CallableInstance(index) => self.include_callable_instance(index)?,
                PendingRecord::ImplementationInstance(index) => {
                    self.include_implementation_instance(index)?;
                }
                PendingRecord::Type(index) => self.include_type(index)?,
                PendingRecord::ConstantValue(index) => self.include_constant_value(index)?,
                PendingRecord::ConstantTerm(index) => self.include_constant_term(index)?,
                PendingRecord::DependencyContract(index) => {
                    self.include_dependency_contract(index)?;
                }
                PendingRecord::DeclarationTemplate(index) => self.include_declaration_template(index)?,
                PendingRecord::SupportEntity(index) => self.include_support_entity(index)?,
                PendingRecord::CheckedTemplate(index) => self.include_checked_template(index)?,
            }
        }

        Ok(())
    }

    fn record_indexes(
        &self,
        directory: &[InterfaceSemanticRecord],
        kind: InterfaceSemanticRecordKind,
        section: InterfaceSectionTag,
    ) -> Result<Vec<u32>, InterfaceValidationError> {
        let entries = directory::records_for_owner(directory, &self.owner, kind);

        let mut indexes = Vec::new();

        for entry in entries {
            if entry.section() != section {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
            }

            indexes.push(entry.record());
        }

        Ok(indexes)
    }

    fn one_record_index(
        &self,
        directory: &[InterfaceSemanticRecord],
        kind: InterfaceSemanticRecordKind,
        section: InterfaceSectionTag,
    ) -> Result<u32, InterfaceValidationError> {
        let indexes = self.record_indexes(directory, kind, section)?;

        let [index] = indexes.as_slice() else {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        };

        Ok(*index)
    }

    fn optional_record_index(
        &self,
        directory: &[InterfaceSemanticRecord],
        kind: InterfaceSemanticRecordKind,
        section: InterfaceSectionTag,
    ) -> Result<Option<u32>, InterfaceValidationError> {
        let indexes = self.record_indexes(directory, kind, section)?;

        match indexes.as_slice() {
            [] => Ok(None),
            [index] => Ok(Some(*index)),
            _ => Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            )),
        }
    }

    fn include_predicate(&mut self, predicate: InterfacePredicateSummary) {
        self.enqueue(PendingRecord::DependencyContract(
            predicate.dependency_contract.raw(),
        ));

        if let Some(term) = predicate.condition {
            self.enqueue(PendingRecord::ConstantTerm(term.raw()));
        }
    }

    fn include_callable_behavior(&mut self, behavior: &InterfaceCallablePhaseBehavior) {
        self.enqueue(PendingRecord::DependencyContract(
            behavior.dependency_contract.raw(),
        ));

        for ty in &*behavior.receiver_cleanup {
            self.enqueue(PendingRecord::Type(ty.raw()));
        }

        for domain in &*behavior.execution_domains {
            for predicate in domain.entry.iter().copied().chain(
                domain
                    .postconditions
                    .iter()
                    .map(|(_, predicate)| *predicate),
            ) {
                self.include_predicate(predicate);
            }
        }

        for predicate in behavior
            .predicate_requirements
            .iter()
            .chain(behavior.predicate_guarantees.iter())
        {
            self.include_predicate(*predicate);
        }
    }

    fn include_callable_contract(
        &mut self,
        index: u32,
        owner: InterfaceSymbolId,
    ) -> Result<(), InterfaceValidationError> {
        let contract = self.tables.contracts.callables.decode(
            index,
            &mut self.context,
            |reader, context| contract::decode_callable_contract(reader, context.limits(), context),
        )?;

        if contract.owner != InterfaceSymbolReference::Local(owner) {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        for clause in contract
            .invocation_preconditions
            .iter()
            .chain(contract.static_constraints.iter())
            .chain(contract.normal_completion_postconditions.iter())
        {
            match clause.value {
                crate::InterfaceCallableContractClauseValue::Predicate(predicate) => {
                    self.include_predicate(predicate);
                }
                crate::InterfaceCallableContractClauseValue::TraitSatisfaction {
                    subject,
                    application,
                } => {
                    self.enqueue(PendingRecord::Type(subject.raw()));
                    self.enqueue(PendingRecord::TraitApplication(application.raw()));
                }
            }
        }

        for behavior in std::iter::once(&contract.invocation_behavior)
            .chain(contract.deferred_execution_behavior.iter())
        {
            self.include_callable_behavior(behavior);
        }

        if contract.has_execution_semantics() {
            // Execution validation reads the owner's signature.
            self.enqueue_owned_record(owner, InterfaceSemanticRecordKind::CallableSignature)?;
            self.include_execution_contract(&contract.execution_contract);
        }

        self.records.callable_contracts.insert(index, contract);

        Ok(())
    }

    fn include_execution_contract(&mut self, contract: &crate::InterfaceCallableExecutionContract) {
        use bray_symbols::CallableExecutionTarget;

        for domain in &*contract.domains {
            for term in domain.entry.iter().chain(domain.postconditions.iter().map(|(_, term)| term)) {
                self.enqueue(PendingRecord::ConstantTerm(term.raw()));
            }
        }

        for (target, _) in contract.evidence.iter().flat_map(|evidence| evidence.dependencies.iter()) {
            match *target {
                CallableExecutionTarget::Callable(callable) => {
                    self.enqueue(PendingRecord::ExecutionTarget(callable.raw()));
                }
                CallableExecutionTarget::Requirement { callable, requirement: (subject, application) } => {
                    self.enqueue(PendingRecord::CallableInstance(callable.raw()));
                    self.enqueue(PendingRecord::Type(subject.raw()));
                    self.enqueue(PendingRecord::TraitApplication(application.raw()));
                }
                CallableExecutionTarget::Indirect(ty) => self.enqueue(PendingRecord::Type(ty.raw())),
            }
        }
    }

    fn include_execution_target(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        self.include_callable_instance(index)?;

        let InterfaceSymbolReference::Local(target) = self.records.callable_instances.values()[&index].definition else {
            // The consumer validates provider evidence against its exact loaded dependency.
            return Ok(());
        };

        self.enqueue_owned_record(target, InterfaceSemanticRecordKind::CallableContracts)
    }

    /// Schedules the single signature or contract record owned by a local callable.
    fn enqueue_owned_record(
        &mut self,
        owner: InterfaceSymbolId,
        kind: InterfaceSemanticRecordKind,
    ) -> Result<(), InterfaceValidationError> {
        let (section, record): (_, fn(u32, InterfaceSymbolId) -> PendingRecord) = match kind {
            InterfaceSemanticRecordKind::CallableSignature => {
                (InterfaceSectionTag::DeclarationSemantics, PendingRecord::CallableSignature)
            }
            InterfaceSemanticRecordKind::CallableContracts => {
                (InterfaceSectionTag::Contracts, PendingRecord::CallableContract)
            }
            _ => unreachable!("only callable signatures and contracts join an execution proof graph"),
        };

        let [entry] = directory::records_for_owner(self.directory, &InterfaceSymbolReference::Local(owner), kind) else {
            return Err(crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Reference));
        };

        if entry.section() != section {
            return Err(crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Reference));
        }

        self.enqueue(record(entry.record(), owner));

        Ok(())
    }

    fn include_declaration_template(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let tables = self.tables.templates.as_ref().expect("selected templates have template tables");

        let declaration = tables.declaration_templates.decode(index, &mut self.context, template::decode_declaration_template)?;

        if declaration.owner() != &self.owner {
            return Err(crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Reference));
        }

        self.enqueue(PendingRecord::SupportEntity(declaration.entity().raw()));
        self.records.declaration_templates.insert(index, declaration);

        Ok(())
    }

    fn include_support_entity(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let entity = self.tables.support.as_ref().expect("selected templates have the support graph")
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .cloned()
            .ok_or(crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Support))?;

        match &entity {
            InterfaceSupportEntity::CheckedTemplate(template) => {
                self.enqueue(PendingRecord::CheckedTemplate(template.raw()));
            }
            InterfaceSupportEntity::Implementation(implementation) => {
                self.enqueue(PendingRecord::Type(implementation.subject().raw()));

                if let Some(application) = implementation.trait_application() {
                    self.enqueue(PendingRecord::TraitApplication(application.raw()));
                }
            }
            InterfaceSupportEntity::Declaration(_) => {}
        }

        self.records.support_entities.insert(index, entity);

        Ok(())
    }

    fn include_checked_template(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let limits = self.context.limits();
        let tables = self.tables.templates.as_ref().expect("selected templates have template tables");

        let checked = tables.checked_templates.decode(index, &mut self.context, |reader, context| {
            template::decode_template(reader, limits, context)
        })?;

        checked.try_map_references(&mut ReferenceCollector(self))?;
        self.records.checked_templates.insert(index, checked);

        Ok(())
    }

    fn include_callable_signature(
        &mut self,
        index: u32,
        owner: InterfaceSymbolId,
    ) -> Result<(), InterfaceValidationError> {
        let tables =
            self.tables
                .declarations
                .as_ref()
                .ok_or(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ))?;

        let signature =
            tables
                .callable_signatures
                .decode(index, &mut self.context, |reader, context| {
                    declaration::decode_callable_signature(reader, context.limits(), context)
                })?;

        if signature.owner != InterfaceSymbolReference::Local(owner) {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        self.enqueue(PendingRecord::Type(signature.callable_type.raw()));
        self.enqueue(PendingRecord::Type(signature.result.raw()));

        self.enqueue(PendingRecord::DependencyContract(
            signature.result_dependencies.raw(),
        ));

        if let Some(receiver) = &signature.receiver {
            self.enqueue(PendingRecord::Type(receiver.ty.raw()));
        }

        self.records.callable_signatures.insert(index, signature);

        Ok(())
    }

    fn include_generic_declaration(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let tables =
            self.tables
                .declarations
                .as_ref()
                .ok_or(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ))?;

        let declaration =
            tables
                .generic_declarations
                .decode(index, &mut self.context, |reader, context| {
                    declaration::decode_generic_declaration(reader, context.limits(), context)
                })?;

        if declaration.owner != self.owner {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        self.records.generic_declarations.insert(index, declaration);

        Ok(())
    }

    fn include_declared_type(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let tables =
            self.tables
                .declarations
                .as_ref()
                .ok_or(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ))?;

        let declared_type = tables.declared_types.decode(
            index,
            &mut self.context,
            declaration::decode_declared_type,
        )?;

        if declared_type.owner() != &self.owner {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        self.enqueue(PendingRecord::Type(declared_type.ty().raw()));
        self.records.declared_types.insert(index, declared_type);

        Ok(())
    }

    fn include_constraint(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let constraint = self.tables.contracts.constraints.decode(
            index,
            &mut self.context,
            contract::decode_constraint,
        )?;

        if constraint.owner != self.owner {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        match constraint.kind {
            crate::InterfaceConstraintKind::Predicate(predicate) => {
                self.include_predicate(predicate);
            }
            crate::InterfaceConstraintKind::TraitSatisfaction {
                subject,
                application,
            } => {
                self.enqueue(PendingRecord::Type(subject.raw()));
                self.enqueue(PendingRecord::TraitApplication(application.raw()));
            }
            crate::InterfaceConstraintKind::TypeEquality { left, right } => {
                self.enqueue(PendingRecord::Type(left.raw()));
                self.enqueue(PendingRecord::Type(right.raw()));
            }
        }

        self.records.constraints.insert(index, constraint);

        Ok(())
    }

    fn include_implementation(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let tables =
            self.tables
                .implementations
                .as_ref()
                .ok_or(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ))?;

        let decoded =
            tables
                .implementations
                .decode(index, &mut self.context, |reader, context| {
                    surface::decode_implementation_record(reader, context.limits(), context)
                })?;

        if decoded.implementation.implementation != self.owner {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        self.enqueue(PendingRecord::Type(decoded.implementation.subject.raw()));

        if let Some(application) = decoded.implementation.trait_application {
            self.enqueue(PendingRecord::TraitApplication(application.raw()));
        }

        for coherence in decoded.coherence {
            self.enqueue(PendingRecord::Coherence(coherence));
        }

        self.records
            .implementations
            .insert(index, decoded.implementation);

        Ok(())
    }

    fn include_coherence(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let tables =
            self.tables
                .implementations
                .as_ref()
                .ok_or(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ))?;

        let coherence = tables
            .coherence
            .decode(index, &mut self.context, |reader, context| {
                surface::decode_coherence_record(reader, context.limits(), context)
            })?;

        if !coherence.implementations.contains(&self.owner) {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        self.enqueue(PendingRecord::Type(coherence.subject.raw()));

        self.enqueue(PendingRecord::TraitApplication(
            coherence.trait_application.raw(),
        ));

        self.records.coherence.insert(index, coherence);

        Ok(())
    }

    fn include_target(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let tables = self
            .tables
            .targets
            .as_ref()
            .ok_or(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ))?;

        let target =
            tables
                .targets
                .decode(index, &mut self.context, surface::decode_target_record)?;

        if target.owner != self.owner {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        self.enqueue(PendingRecord::ConstantValue(target.value.raw()));
        self.records.target_dependencies.insert(index, target);

        Ok(())
    }

    fn include_runtime(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let tables = self
            .tables
            .targets
            .as_ref()
            .ok_or(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ))?;

        let runtime = tables
            .runtimes
            .decode(index, &mut self.context, |reader, context| {
                surface::decode_runtime_record(reader, context.limits(), context)
            })?;

        if runtime.owner != self.owner {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        self.records.runtime_requirements.insert(index, runtime);

        Ok(())
    }

    fn include_substitution(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let substitution = self.tables.types.substitutions.decode(
            index,
            &mut self.context,
            |reader, context| value::decode_substitution(reader, context.limits(), context),
        )?;

        for binding in &*substitution.bindings {
            match binding.argument {
                InterfaceGenericArgument::Type(ty) => {
                    self.enqueue(PendingRecord::Type(ty.raw()));
                }
                InterfaceGenericArgument::Constant(term) => {
                    self.enqueue(PendingRecord::ConstantTerm(term.raw()));
                }
            }
        }

        self.records.substitutions.insert(index, substitution);

        Ok(())
    }

    fn include_trait_application(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let application = self.tables.types.trait_applications.decode(
            index,
            &mut self.context,
            value::decode_trait_application,
        )?;

        self.enqueue(PendingRecord::Substitution(application.substitution.raw()));

        self.records.trait_applications.insert(index, application);

        Ok(())
    }

    fn include_callable_instance(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let instance = self.tables.types.callable_instances.decode(
            index,
            &mut self.context,
            value::decode_callable_instance,
        )?;

        self.enqueue(PendingRecord::Substitution(instance.substitution.raw()));

        self.records.callable_instances.insert(index, instance);

        Ok(())
    }

    fn include_implementation_instance(
        &mut self,
        index: u32,
    ) -> Result<(), InterfaceValidationError> {
        let instance = self.tables.types.implementation_instances.decode(
            index,
            &mut self.context,
            value::decode_implementation_instance,
        )?;

        self.enqueue(PendingRecord::Substitution(instance.substitution.raw()));

        self.records
            .implementation_instances
            .insert(index, instance);

        Ok(())
    }

    fn include_type(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let ty = self
            .tables
            .types
            .types
            .decode(index, &mut self.context, |reader, context| {
                value::decode_type(reader, context.limits(), context)
            })?;

        match &ty {
            InterfaceType::Named { substitution, .. } => {
                self.enqueue(PendingRecord::Substitution(substitution.raw()));
            }
            InterfaceType::TypeValuedMemberProjection {
                subject,
                application,
                ..
            } => {
                self.enqueue(PendingRecord::Type(subject.raw()));
                self.enqueue(PendingRecord::TraitApplication(application.raw()));
            }
            InterfaceType::Tuple(elements) => {
                for element in &**elements {
                    self.enqueue(PendingRecord::Type(element.raw()));
                }
            }
            InterfaceType::Array { element, length } => {
                self.enqueue(PendingRecord::Type(element.raw()));
                self.enqueue(PendingRecord::ConstantTerm(length.raw()));
            }
            InterfaceType::FlexibleArray(target)
            | InterfaceType::Slice(target)
            | InterfaceType::Generator(target)
            | InterfaceType::Nullable(target)
            | InterfaceType::Borrow { target, .. } => {
                self.enqueue(PendingRecord::Type(target.raw()));
            }
            InterfaceType::TraitView(application) => {
                self.enqueue(PendingRecord::TraitApplication(application.raw()));
            }
            InterfaceType::OwnedIndirection { storage, target } => {
                self.enqueue(PendingRecord::Type(storage.raw()));
                self.enqueue(PendingRecord::Type(target.raw()));
            }
            InterfaceType::Callable {
                parameters,
                result,
                invocation_behavior,
                deferred_execution_behavior,
                ..
            } => {
                for parameter in &**parameters {
                    self.enqueue(PendingRecord::Type(parameter.ty.raw()));
                }

                self.enqueue(PendingRecord::Type(result.raw()));

                for behavior in
                    std::iter::once(invocation_behavior).chain(deferred_execution_behavior.iter())
                {
                    self.include_callable_behavior(behavior);
                }
            }
            InterfaceType::TypeParameter(_) | InterfaceType::ContextualSelf(_) => {}
        }

        self.records.types.insert(index, ty);

        Ok(())
    }

    fn include_type_representation(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let tables = self.tables.declarations.as_ref()
            .expect("selected representation has declaration tables");

        let representation = tables.type_representations.decode(index, &mut self.context, |reader, context| {
            declaration::decode_type_representation(reader, context.limits(), context)
        })?;

        for ty in representation.referenced_types() {
            self.enqueue(PendingRecord::Type(ty.raw()));
        }

        self.records.type_representations.insert(index, representation);

        Ok(())
    }

    fn include_constant_value(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let value =
            self.tables
                .constants
                .values
                .decode(index, &mut self.context, |reader, context| {
                    value::decode_constant_value_record(reader, context.limits(), context)
                })?;

        self.enqueue(PendingRecord::Type(value.ty.raw()));

        match &value.kind {
            InterfaceConstantValueKind::NullablePresent(value) => {
                self.enqueue(PendingRecord::ConstantValue(value.raw()));
            }
            InterfaceConstantValueKind::Tuple(values)
            | InterfaceConstantValueKind::Array(values) => {
                for value in &**values {
                    self.enqueue(PendingRecord::ConstantValue(value.raw()));
                }
            }
            InterfaceConstantValueKind::Product(fields) => {
                for field in fields.iter() {
                    self.enqueue(PendingRecord::ConstantValue(field.value().raw()));
                }
            }
            InterfaceConstantValueKind::Union { fields, .. } => {
                for field in fields.iter() {
                    self.enqueue(PendingRecord::ConstantValue(field.value().raw()));
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

        self.records.constant_values.insert(index, value);

        Ok(())
    }

    fn include_constant_term(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let term = self.tables.constants.terms.decode(
            index,
            &mut self.context,
            value::decode_constant_term,
        )?;

        match &term {
            InterfaceConstantTerm::Typed { term, ty } => {
                self.enqueue(PendingRecord::ConstantTerm(term.raw()));
                self.enqueue(PendingRecord::Type(ty.raw()));
            }
            InterfaceConstantTerm::Value(value) => {
                self.enqueue(PendingRecord::ConstantValue(value.raw()));
            }
            InterfaceConstantTerm::Unary { operand, .. } => {
                self.enqueue(PendingRecord::ConstantTerm(operand.raw()));
            }
            InterfaceConstantTerm::RepeatedArray {
                value: left,
                count: right,
            }
            | InterfaceConstantTerm::Binary { left, right, .. } => {
                self.enqueue(PendingRecord::ConstantTerm(left.raw()));
                self.enqueue(PendingRecord::ConstantTerm(right.raw()));
            }
            InterfaceConstantTerm::Conversion { operand, target } => {
                self.enqueue(PendingRecord::ConstantTerm(operand.raw()));
                self.enqueue(PendingRecord::Type(target.raw()));
            }
            InterfaceConstantTerm::VariantTest { subject: value, .. }
            | InterfaceConstantTerm::NullablePresent(value) => {
                self.enqueue(PendingRecord::ConstantTerm(value.raw()));
            }
            InterfaceConstantTerm::Tuple(values) | InterfaceConstantTerm::Array(values) => {
                for value in values.iter() {
                    self.enqueue(PendingRecord::ConstantTerm(value.raw()));
                }
            }
            InterfaceConstantTerm::Product(fields) => {
                for field in fields.iter() {
                    self.enqueue(PendingRecord::ConstantTerm(field.value().raw()));
                }
            }
            InterfaceConstantTerm::Union { fields, .. } => {
                for field in fields.iter() {
                    self.enqueue(PendingRecord::ConstantTerm(field.value().raw()));
                }
            }
            InterfaceConstantTerm::TraitConstant {
                subject,
                application,
                ..
            } => {
                self.enqueue(PendingRecord::Type(subject.raw()));
                self.enqueue(PendingRecord::TraitApplication(application.raw()));
            }
            InterfaceConstantTerm::DefinitionApplication {
                substitution,
                selected_implementation,
                ..
            } => {
                self.enqueue(PendingRecord::Substitution(substitution.raw()));

                if let Some(implementation) = selected_implementation {
                    self.enqueue(PendingRecord::ImplementationInstance(implementation.raw()));
                }
            }
            InterfaceConstantTerm::Call {
                callable,
                selected_implementation,
                arguments,
                implementation_witnesses,
            } => {
                self.enqueue(PendingRecord::CallableInstance(callable.raw()));

                if let Some(implementation) = selected_implementation {
                    self.enqueue(PendingRecord::ImplementationInstance(implementation.raw()));
                }

                for witness in &**implementation_witnesses {
                    self.enqueue(PendingRecord::ImplementationInstance(witness.raw()));
                }

                for argument in &**arguments {
                    self.enqueue(PendingRecord::ConstantTerm(argument.raw()));
                }
            }
            InterfaceConstantTerm::PredicateCall {
                substitution,
                arguments,
                ..
            } => {
                self.enqueue(PendingRecord::Substitution(substitution.raw()));

                for argument in &**arguments {
                    self.enqueue(PendingRecord::ConstantTerm(argument.raw()));
                }
            }
            InterfaceConstantTerm::Projection { subject, kind } => {
                self.enqueue(PendingRecord::ConstantTerm(subject.raw()));

                for term in kind.term_references() {
                    self.enqueue(PendingRecord::ConstantTerm(term.raw()));
                }
            }
            InterfaceConstantTerm::IntegerLiteral { .. }
            | InterfaceConstantTerm::CallableArgument(_)
            | InterfaceConstantTerm::Parameter(_)
            | InterfaceConstantTerm::TargetProperty(_) => {}
        }

        self.records.constant_terms.insert(index, term);

        Ok(())
    }

    fn include_dependency_contract(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
        let contract = self.tables.contracts.dependencies.decode(
            index,
            &mut self.context,
            |reader, context| {
                contract::decode_dependency_contract(reader, context.limits(), context)
            },
        )?;

        for requirement in contract.all_requirements() {
            self.include_dependency_requirement(requirement);
        }

        for guard in contract.failure_outcomes.as_deref().into_iter().flatten().filter_map(|row| row.guard) {
            self.enqueue(PendingRecord::ConstantTerm(guard.raw()));
        }

        let storage_rows = contract.captured_values.iter().map(|outcome| {
            (outcome.references.as_ref(), outcome.stored_values.as_ref())
        }).chain(contract.failure_outcomes.as_deref().into_iter().flatten().map(|outcome| {
            (outcome.references.as_ref(), outcome.stored_values.as_ref())
        }));

        for stored in storage_rows.clone().flat_map(|(_, stored)| stored) {
            self.include_dependency_subject(stored.destination().subject());

            for source in stored.sources() {
                self.include_dependency_subject(source.subject());
            }
        }

        for relation in storage_rows.flat_map(|(references, _)| references) {
            if let Some(entry) = relation.source_subject() {
                self.include_dependency_subject(entry);
            }

            self.include_dependency_subject(relation.exit());
            self.include_dependency_projections(relation.target_projections());
        }

        for binding in &*contract.captured_values {
            for payload in &*binding.report_payloads {
                self.include_dependency_subject(&payload.destination);
            }

            for payload in binding.inactive_payloads.iter() {
                self.include_dependency_subject(payload);
            }

            if let Some(guard) = binding.guard { self.enqueue(PendingRecord::ConstantTerm(guard.raw())); }
        }

        let values = contract.captured_values.iter().flat_map(|row| row.bindings.iter())
            .chain(contract.failure_outcomes.as_deref().into_iter().flatten().flat_map(|row| row.writes.iter()));

        for (destination, value) in values {
            self.include_dependency_subject(destination);

            for input in value.terms() { self.enqueue(PendingRecord::ConstantTerm(input.raw())); }

            for call in value.callees() {
                self.enqueue(PendingRecord::CallableInstance(call.instance.raw()));
                self.enqueue(PendingRecord::Type(call.signature.raw()));

                if let Some(receiver) = call.receiver { self.enqueue(PendingRecord::Type(receiver.raw())); }

                if let Some((ty, application)) = call.requirement {
                    self.enqueue(PendingRecord::Type(ty.raw()));
                    self.enqueue(PendingRecord::TraitApplication(application.raw()));
                }

                for w in call.dispatch.iter().chain(&*call.witnesses) { self.enqueue(PendingRecord::ImplementationInstance(w.raw())); }
            }

            for root in value.static_roots() {
                self.include_static_reference(root);
            }

            self.include_dependency_projections(&value.projections().cloned().collect::<Vec<_>>());
        }

        for condition in contract.captured_values.iter().flat_map(|row| row.ownership.conditions()) {
            self.enqueue(PendingRecord::ConstantTerm(condition.raw()));
        }

        for transfer in contract.captured_values.iter().flat_map(|row| row.ownership.transfers()) {
            self.enqueue(PendingRecord::ConstantTerm(transfer.input().raw()));
        }

        for carrier in contract.captured_values.iter().flat_map(|row| row.carriers.iter()) {
            self.enqueue(PendingRecord::ConstantTerm(carrier.guarantee().raw()));
            self.include_dependency_projections(carrier.path());

            if let Some(support) = carrier.support() {
                for owner in support.owners() { self.include_dependency_subject(owner); }

                for (address, owners) in support.addresses() {
                    self.enqueue(PendingRecord::ConstantTerm(address.raw()));

                    for owner in owners { self.include_dependency_subject(owner); }
                }
            }
        }

        for effect in contract.effects.iter().chain(contract.cell_changes()) {

            for ty in effect.referenced_types() {
                self.enqueue(PendingRecord::Type(ty.raw()));
            }

            for subject in effect.subjects() {
                self.include_dependency_subject(subject);
            }

            for projections in effect.projection_paths() {
                self.include_dependency_projections(projections);
            }
        }

        self.records.dependency_contracts.insert(index, contract);

        Ok(())
    }

    fn include_dependency_requirement(&mut self, requirement: &InterfaceDependencyRequirement) {
        let mut pending = vec![requirement];

        while let Some(requirement) = pending.pop() {
            match &requirement.value {
                InterfaceDependencyRequirementValue::CleanupDependencies { ty, requirements } => {
                    self.enqueue(PendingRecord::Type(ty.raw()));
                    pending.extend(requirements.iter());
                }
                InterfaceDependencyRequirementValue::SynchronousCleanup(ty)
                | InterfaceDependencyRequirementValue::FinalizerBodyCleanup(ty) => {
                    self.enqueue(PendingRecord::Type(ty.raw()));
                }
                InterfaceDependencyRequirementValue::DefaultCleanup {
                    substitution,
                    contextual_self,
                    ..
                } => {
                    self.enqueue(PendingRecord::Substitution(substitution.raw()));

                    if let Some((_, subject)) = contextual_self {
                        self.enqueue(PendingRecord::Type(subject.raw()));
                    }
                }
                InterfaceDependencyRequirementValue::Variable { .. } => {}
                InterfaceDependencyRequirementValue::FixedPoint {
                    definitions,
                    result,
                } => pending.extend(
                    definitions
                        .iter()
                        .flat_map(|definition| definition.iter())
                        .chain(result.iter()),
                ),
                InterfaceDependencyRequirementValue::ResultCall {
                    output: _,
                    callable,
                    requirement,
                    inputs,
                } => {
                    self.enqueue(PendingRecord::CallableInstance(callable.raw()));

                    if let Some((subject, application)) = requirement {
                        self.enqueue(PendingRecord::Type(subject.raw()));
                        self.enqueue(PendingRecord::TraitApplication(application.raw()));
                    }

                    for input in inputs.iter() {
                        pending.extend(input.values.iter().chain(input.storage.iter()));
                    }
                }
                InterfaceDependencyRequirementValue::Direct { subject, .. } => {
                    self.include_dependency_subject(subject);
                }
                InterfaceDependencyRequirementValue::Guarded {
                    guard,
                    requirements,
                } => {
                    match guard {
                        InterfaceDependencyGuard::NullablePresent(subject)
                        | InterfaceDependencyGuard::ActiveUnionVariant { subject, .. } => {
                            self.include_dependency_subject(subject);
                        }
                    }

                    pending.extend(requirements.iter().rev());
                }
            }
        }
    }

    fn include_dependency_subject(&mut self, subject: &InterfaceDependencySubject) {
        if let InterfaceDependencySubjectRoot::ImplementationWitness(implementation) = subject.root
        {
            self.enqueue(PendingRecord::ImplementationInstance(implementation.raw()));
        }

        if let InterfaceDependencySubjectRoot::ImplementationRequirement(ty, application) =
            subject.root
        {
            self.enqueue(PendingRecord::Type(ty.raw()));
            self.enqueue(PendingRecord::TraitApplication(application.raw()));
        }

        if let InterfaceDependencySubjectRoot::StaticInstance(reference, _) = &subject.root {
            self.include_static_reference(reference);
        }

        self.include_dependency_projections(&subject.projections);
    }

    fn include_static_reference(&mut self, reference: &crate::InterfaceStaticReference) {
        self.enqueue(PendingRecord::Substitution(reference.substitution.raw()));

        for witness in reference.selected_witnesses.iter() {
            self.enqueue(PendingRecord::ImplementationInstance(witness.raw()));
        }
    }

    fn include_dependency_projections(&mut self, projections: &[InterfaceDependencyProjection]) {
        for projection in projections {
            if let InterfaceDependencyProjection::Element(term) = projection {
                self.enqueue(PendingRecord::ConstantTerm(term.raw()));
            }
        }
    }
}
