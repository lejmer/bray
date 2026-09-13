// rust-style: allow(module-too-large, reason = "selective decoding shares one recursive dependency traversal and remapping state")

use std::collections::BTreeSet;

use bray_symbols::InterfaceSymbolId;

use super::tables::SelectedTables;
use crate::semantic::codec::common::SemanticDecodeContext;
use crate::semantic::codec::decoding::selection::model::SelectedRecords;
use crate::semantic::codec::decoding::selection::remap::remap_selected_records;
use crate::semantic::codec::decoding::{bundle, contract, declaration, directory, surface, value};
use crate::semantic::model::{
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantValueKind,
    InterfaceDependencyGuard, InterfaceDependencyProjection, InterfaceDependencyRequirement,
    InterfaceDependencyRequirementValue, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceGenericArgument, InterfaceSemanticRecord,
    InterfaceSemanticRecordKind, InterfaceSemantics, InterfaceType,
};
use crate::{
    InterfaceSectionTag, InterfaceSymbolReference, InterfaceValidationError,
    InterfaceValidationLimits, PackageInterfaceSurface, ValidatedInterfaceSection,
};

pub(in crate::semantic::codec::decoding) fn decode_selected_record_graph(
    sections: &[ValidatedInterfaceSection<'_>],
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
    kind: InterfaceSemanticRecordKind,
    limits: InterfaceValidationLimits,
) -> Result<InterfaceSemantics, InterfaceValidationError> {
    let mut context = SemanticDecodeContext::new(limits);

    let directory =
        bundle::required_section(sections, InterfaceSectionTag::SemanticRecordDirectory)?;

    let directory = directory::decode_semantic_directory(directory, limits, &mut context)?;

    if kind == InterfaceSemanticRecordKind::CallableParameterDefault {
        return crate::semantic::codec::decoding::selection::declaration::decode_callable_parameter_default(
            sections, surface, owner, limits, context, &directory,
        );
    }

    if kind == InterfaceSemanticRecordKind::PredicateDefinition {
        return crate::semantic::codec::decoding::selection::declaration::decode_predicate_definition(
            sections, surface, owner, limits, context, &directory,
        );
    }

    let tables = SelectedTables::read(sections, kind, &mut context)?;
    let mut builder = SelectionBuilder::new(tables, context, owner);

    builder.include_requested_semantics(&directory, kind)?;

    let semantics = remap_selected_records(builder.records)?;

    if semantics.callable_signatures.iter().any(|signature| {
        matches!(
            signature.callable_type.to_index().and_then(|index| semantics.types.get(index)),
            Some(InterfaceType::Callable { invocation_behavior, deferred_execution_behavior, .. })
                if !invocation_behavior.execution_properties.is_empty()
                    || deferred_execution_behavior.as_ref().is_some_and(|behavior| !behavior.execution_properties.is_empty())
        )
    }) {
        return bundle::decode_semantics(sections, surface, limits);
    }

    semantics.validate(surface, limits)?;

    Ok(semantics)
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum PendingRecord {
    Constraint(u32),
    CallableSignature(u32),
    GenericDeclaration(u32),
    DeclaredType(u32),
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
}

struct SelectionBuilder<'bytes> {
    tables: SelectedTables<'bytes>,
    context: SemanticDecodeContext,
    owner: InterfaceSymbolReference,
    pending: Vec<PendingRecord>,
    scheduled: BTreeSet<PendingRecord>,
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
            pending: Vec::new(),
            scheduled: BTreeSet::new(),
            records: SelectedRecords::new(),
        }
    }

    fn include_requested_semantics(
        &mut self,
        directory: &[InterfaceSemanticRecord],
        kind: InterfaceSemanticRecordKind,
    ) -> Result<(), InterfaceValidationError> {
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

            self.enqueue(PendingRecord::CallableSignature(index));
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

        self.include_pending_records()
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
                PendingRecord::CallableSignature(index) => {
                    self.include_callable_signature(index)?;
                }
                PendingRecord::GenericDeclaration(index) => {
                    self.include_generic_declaration(index)?;
                }
                PendingRecord::DeclaredType(index) => self.include_declared_type(index)?,
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
        let entries = directory
            .iter()
            .filter(|entry| entry.owner() == &self.owner && entry.kind() == kind);

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

    fn include_callable_signature(&mut self, index: u32) -> Result<(), InterfaceValidationError> {
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

        if signature.owner != self.owner {
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
            crate::InterfaceConstraintKind::Predicate(predicate) => self.enqueue(
                PendingRecord::DependencyContract(predicate.dependency_contract.raw()),
            ),
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

                self.enqueue(PendingRecord::DependencyContract(
                    invocation_behavior.dependency_contract.raw(),
                ));

                if let Some(behavior) = deferred_execution_behavior {
                    self.enqueue(PendingRecord::DependencyContract(
                        behavior.dependency_contract.raw(),
                    ));
                }
            }
            InterfaceType::TypeParameter(_) | InterfaceType::ContextualSelf(_) => {}
        }

        self.records.types.insert(index, ty);

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
            InterfaceConstantTerm::Binary { left, right, .. } => {
                self.enqueue(PendingRecord::ConstantTerm(left.raw()));
                self.enqueue(PendingRecord::ConstantTerm(right.raw()));
            }
            InterfaceConstantTerm::Conversion { operand, target } => {
                self.enqueue(PendingRecord::ConstantTerm(operand.raw()));
                self.enqueue(PendingRecord::Type(target.raw()));
            }
            InterfaceConstantTerm::NullablePresent(value) => {
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
            } => {
                self.enqueue(PendingRecord::CallableInstance(callable.raw()));

                if let Some(implementation) = selected_implementation {
                    self.enqueue(PendingRecord::ImplementationInstance(implementation.raw()));
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

                if let InterfaceConstantProjection::ArrayElement(index) = kind {
                    self.enqueue(PendingRecord::ConstantTerm(index.raw()));
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

        for requirement in &*contract.requirements {
            self.include_dependency_requirement(requirement);
        }

        self.records.dependency_contracts.insert(index, contract);

        Ok(())
    }

    fn include_dependency_requirement(&mut self, requirement: &InterfaceDependencyRequirement) {
        let mut pending = vec![requirement];

        while let Some(requirement) = pending.pop() {
            match &requirement.value {
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

        for projection in &*subject.projections {
            if let InterfaceDependencyProjection::Element(term) = projection {
                self.enqueue(PendingRecord::ConstantTerm(term.raw()));
            }
        }
    }
}
