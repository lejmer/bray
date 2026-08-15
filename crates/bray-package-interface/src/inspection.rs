use std::sync::Arc;

use crate::decode::DecodeBudget;
use crate::{
    InterfaceHeader, InterfaceSectionTag, InterfaceValidationError, ValidatedInterfaceSection,
    ValidatedPackageInterface,
};

/// Deterministic semantic inspection of one validated package interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageInterfaceInspection {
    header: InterfaceHeader,
    byte_len: u64,
    section_count: usize,
    section_index: Arc<[InterfaceSectionIndexEntry]>,
    sections: Arc<[InterfaceInspectionSection]>,
}

impl PackageInterfaceInspection {
    /// Returns the validated public header fields.
    pub const fn header(&self) -> InterfaceHeader {
        self.header
    }

    /// Returns the exact validated artifact byte length.
    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }

    /// Returns the complete number of validated sections.
    pub const fn section_count(&self) -> usize {
        self.section_count
    }

    /// Returns known section categories and record counts in canonical order.
    pub fn section_index(&self) -> &[InterfaceSectionIndexEntry] {
        &self.section_index
    }

    /// Returns decoded selected sections in canonical order.
    pub fn sections(&self) -> &[InterfaceInspectionSection] {
        &self.sections
    }
}

/// One known section available for semantic inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterfaceSectionIndexEntry {
    section: InterfaceSectionTag,
    record_count: u64,
}

impl InterfaceSectionIndexEntry {
    /// Returns the known semantic section category.
    pub const fn section(self) -> InterfaceSectionTag {
        self.section
    }

    /// Returns the validated record count without exposing layout data.
    pub const fn record_count(self) -> u64 {
        self.record_count
    }
}

/// Decoded record summary for one selected section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceInspectionSection {
    section: InterfaceSectionTag,
    record_count: u64,
    records: Arc<[InterfaceInspectionRecord]>,
}

impl InterfaceInspectionSection {
    /// Returns the selected section category.
    pub const fn section(&self) -> InterfaceSectionTag {
        self.section
    }

    /// Returns the validated record count declared by the section.
    pub const fn record_count(&self) -> u64 {
        self.record_count
    }

    /// Returns decoded semantic record categories in wire-table order.
    pub fn records(&self) -> &[InterfaceInspectionRecord] {
        &self.records
    }
}

/// Count of one decoded semantic record category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterfaceInspectionRecord {
    kind: InterfaceInspectionRecordKind,
    count: u64,
}

impl InterfaceInspectionRecord {
    pub(crate) const fn new(kind: InterfaceInspectionRecordKind, count: u64) -> Self {
        Self { kind, count }
    }

    /// Returns the semantic record category.
    pub const fn kind(self) -> InterfaceInspectionRecordKind {
        self.kind
    }

    /// Returns the number of decoded records in this category.
    pub const fn count(self) -> u64 {
        self.count
    }
}

/// Stable semantic category used by structured inspection reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterfaceInspectionRecordKind {
    /// Canonical UTF-8 strings.
    Strings,
    /// Package and product metadata records.
    PackageMetadata,
    /// Direct package dependencies.
    Dependencies,
    /// Exported symbol identities.
    SymbolIdentities,
    /// Typed symbol relationships.
    Relationships,
    /// Exported lookup edges.
    ExportedLookups,
    /// Symbol-owned record addresses.
    SymbolSemantics,
    /// Canonical generic substitutions.
    GenericSubstitutions,
    /// Canonical trait applications.
    TraitApplications,
    /// Canonical callable instances.
    CallableInstances,
    /// Canonical implementation instances.
    ImplementationInstances,
    /// Canonical semantic types.
    SemanticTypes,
    /// Canonical constant values.
    ConstantValues,
    /// Canonical constant terms.
    ConstantTerms,
    /// Portable dependency contracts.
    DependencyContracts,
    /// Generic constraints.
    Constraints,
    /// Callable contracts.
    CallableContracts,
    /// Callable signature semantics.
    CallableSignatures,
    /// Generic declaration semantics.
    GenericDeclarations,
    /// Callable parameter default-availability semantics.
    CallableParameterDefaults,
    /// Predicate definition-state semantics.
    PredicateDefinitions,
    /// Declared type representation semantics.
    TypeRepresentations,
    /// Source-independent checked templates.
    CheckedTemplates,
    /// Declaration-owned template semantics.
    DeclarationTemplates,
    /// Public implementation records.
    Implementations,
    /// Implementation coherence records.
    Coherence,
    /// Required target semantics.
    TargetProperties,
    /// Required callable ABIs.
    AbiDependencies,
    /// Required private runtime ABI surfaces.
    RuntimeRequirements,
    /// Optional source provenance.
    SourceProvenance,
    /// Private support entities summarized without exposing their layout.
    SupportEntities,
}

impl InterfaceInspectionRecordKind {
    /// Returns the stable machine-readable record category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strings => "strings",
            Self::PackageMetadata => "package_metadata",
            Self::Dependencies => "dependencies",
            Self::SymbolIdentities => "symbol_identities",
            Self::Relationships => "relationships",
            Self::ExportedLookups => "exported_lookups",
            Self::SymbolSemantics => "symbol_semantics",
            Self::GenericSubstitutions => "generic_substitutions",
            Self::TraitApplications => "trait_applications",
            Self::CallableInstances => "callable_instances",
            Self::ImplementationInstances => "implementation_instances",
            Self::SemanticTypes => "semantic_types",
            Self::ConstantValues => "constant_values",
            Self::ConstantTerms => "constant_terms",
            Self::DependencyContracts => "dependency_contracts",
            Self::Constraints => "constraints",
            Self::CallableContracts => "callable_contracts",
            Self::CallableSignatures => "callable_signatures",
            Self::GenericDeclarations => "generic_declarations",
            Self::CallableParameterDefaults => "callable_parameter_defaults",
            Self::PredicateDefinitions => "predicate_definitions",
            Self::TypeRepresentations => "type_representations",
            Self::CheckedTemplates => "checked_templates",
            Self::DeclarationTemplates => "declaration_templates",
            Self::Implementations => "implementations",
            Self::Coherence => "coherence",
            Self::TargetProperties => "target_properties",
            Self::AbiDependencies => "abi_dependencies",
            Self::RuntimeRequirements => "runtime_requirements",
            Self::SourceProvenance => "source_provenance",
            Self::SupportEntities => "support_entities",
        }
    }
}

impl ValidatedPackageInterface {
    /// Decodes only the requested known sections and the string index they require.
    pub fn inspect(
        &self,
        selected: &[InterfaceSectionTag],
    ) -> Result<PackageInterfaceInspection, InterfaceValidationError> {
        let section_index = self
            .sections()?
            .into_iter()
            .map(|section| InterfaceSectionIndexEntry {
                section: section.tag(),
                record_count: section.record_count(),
            })
            .collect::<Vec<_>>();

        let selected_sections = selected_sections(self, selected)?;

        crate::semantic::validate_decode_allocation(&selected_sections, self.limits())?;

        let mut strings = None;
        let mut surface_budget = DecodeBudget::new(self.limits());
        let mut sections = Vec::new();

        for section in selected_sections {
            let section_tag = section.tag();

            let records = if is_semantic_section(section_tag) {
                crate::semantic::decode_inspection_records(section, self.limits())?
            } else {
                decode_surface_inspection_records(self, section, &mut strings, &mut surface_budget)?
            };

            sections.push(InterfaceInspectionSection {
                section: section_tag,
                record_count: section.record_count(),
                records: records.into(),
            });
        }

        Ok(PackageInterfaceInspection {
            header: self.header(),
            byte_len: self.byte_len(),
            section_count: self.section_count(),
            section_index: section_index.into(),
            sections: sections.into(),
        })
    }
}

fn selected_sections<'interface>(
    interface: &'interface ValidatedPackageInterface,
    selected: &[InterfaceSectionTag],
) -> Result<Vec<ValidatedInterfaceSection<'interface>>, InterfaceValidationError> {
    let mut sections = Vec::new();

    for section_tag in InterfaceSectionTag::ALL {
        if !selected.contains(&section_tag) {
            continue;
        }

        let Some(section) = interface.section(section_tag)? else {
            if section_tag.is_optional() {
                continue;
            }

            return Err(InterfaceValidationError::Malformed);
        };

        sections.push(section);
    }

    Ok(sections)
}

fn decode_surface_inspection_records(
    interface: &ValidatedPackageInterface,
    section: ValidatedInterfaceSection<'_>,
    strings: &mut Option<Vec<Arc<str>>>,
    budget: &mut DecodeBudget,
) -> Result<Vec<InterfaceInspectionRecord>, InterfaceValidationError> {
    let (kind, count) = match section.tag() {
        InterfaceSectionTag::Strings => {
            let strings = decode_strings_index(interface, strings, budget)?;

            (InterfaceInspectionRecordKind::Strings, strings.len())
        }
        InterfaceSectionTag::PackageMetadata => {
            let strings = decode_strings_index(interface, strings, budget)?;

            crate::surface::decode_metadata(section, strings)?;

            (InterfaceInspectionRecordKind::PackageMetadata, 1)
        }
        InterfaceSectionTag::Dependencies => {
            let strings = decode_strings_index(interface, strings, budget)?;
            let dependencies = crate::surface::decode_dependencies(section, strings, budget)?;

            (
                InterfaceInspectionRecordKind::Dependencies,
                dependencies.len(),
            )
        }
        InterfaceSectionTag::SymbolIdentities => {
            let strings = decode_strings_index(interface, strings, budget)?;
            let symbols = crate::surface::decode_symbols(section, strings, budget)?;

            (
                InterfaceInspectionRecordKind::SymbolIdentities,
                symbols.len(),
            )
        }
        InterfaceSectionTag::Relationships => {
            let relationships = crate::surface::decode_relationships(section, budget)?;

            (
                InterfaceInspectionRecordKind::Relationships,
                relationships.len(),
            )
        }
        InterfaceSectionTag::ExportedLookup => {
            let strings = decode_strings_index(interface, strings, budget)?;
            let exports = crate::surface::decode_exports(section, strings, budget)?;

            (
                InterfaceInspectionRecordKind::ExportedLookups,
                exports.len(),
            )
        }
        InterfaceSectionTag::SemanticRecordDirectory
        | InterfaceSectionTag::SemanticTypes
        | InterfaceSectionTag::Constants
        | InterfaceSectionTag::Contracts
        | InterfaceSectionTag::DeclarationSemantics
        | InterfaceSectionTag::DeclarationTemplates
        | InterfaceSectionTag::Implementations
        | InterfaceSectionTag::TargetDependencies
        | InterfaceSectionTag::SourceProvenance
        | InterfaceSectionTag::SupportGraph => {
            return Err(InterfaceValidationError::Malformed);
        }
    };

    Ok(vec![InterfaceInspectionRecord::new(
        kind,
        usize_to_u64(count),
    )])
}

fn decode_strings_index<'strings>(
    interface: &ValidatedPackageInterface,
    strings: &'strings mut Option<Vec<Arc<str>>>,
    budget: &mut DecodeBudget,
) -> Result<&'strings [Arc<str>], InterfaceValidationError> {
    if strings.is_none() {
        let section = interface
            .section(InterfaceSectionTag::Strings)?
            .ok_or(InterfaceValidationError::Malformed)?;

        *strings = Some(crate::surface::decode_strings(section, budget)?);
    }

    strings
        .as_deref()
        .ok_or(InterfaceValidationError::Malformed)
}

const fn is_semantic_section(section: InterfaceSectionTag) -> bool {
    matches!(
        section,
        InterfaceSectionTag::SemanticRecordDirectory
            | InterfaceSectionTag::SemanticTypes
            | InterfaceSectionTag::Constants
            | InterfaceSectionTag::Contracts
            | InterfaceSectionTag::DeclarationSemantics
            | InterfaceSectionTag::DeclarationTemplates
            | InterfaceSectionTag::Implementations
            | InterfaceSectionTag::TargetDependencies
            | InterfaceSectionTag::SourceProvenance
            | InterfaceSectionTag::SupportGraph
    )
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::InterfaceInspectionRecordKind;
    use crate::artifact::{EncodedArtifactSection, assemble_sections};
    use crate::surface::encode_surface;
    use crate::test_support::package_interface_export_bundle;
    use crate::{
        InterfaceLanguageRevision, InterfaceSectionTag, InterfaceValidationPolicy,
        ValidatedPackageInterface,
    };

    #[test]
    fn inspection_reports_every_current_section_in_canonical_order() {
        let interface = validated_test_interface();

        let inspection = interface
            .inspect(&InterfaceSectionTag::ALL)
            .unwrap_or_else(|error| panic!("test interface must inspect: {error:?}"));

        let sections = inspection
            .sections()
            .iter()
            .map(|section| section.section())
            .collect::<Vec<_>>();

        assert_eq!(sections, InterfaceSectionTag::ALL);

        assert_eq!(
            inspection.section_index().len(),
            InterfaceSectionTag::ALL.len()
        );

        assert_eq!(inspection.section_count(), InterfaceSectionTag::ALL.len());
    }

    #[test]
    fn selected_section_order_and_duplicates_do_not_change_inspection() {
        let interface = validated_test_interface();

        let forward = interface.inspect(&[
            InterfaceSectionTag::Strings,
            InterfaceSectionTag::TargetDependencies,
        ]);

        let reordered = interface.inspect(&[
            InterfaceSectionTag::TargetDependencies,
            InterfaceSectionTag::Strings,
            InterfaceSectionTag::TargetDependencies,
        ]);

        assert_eq!(forward, reordered);
    }

    #[test]
    fn declaration_record_inspection_reports_each_record_category() {
        let interface = validated_test_interface();

        let inspection = interface
            .inspect(&[InterfaceSectionTag::DeclarationSemantics])
            .unwrap_or_else(|error| panic!("declaration semantics must inspect: {error:?}"));

        assert_eq!(
            inspection.sections()[0]
                .records()
                .iter()
                .map(|record| record.kind())
                .collect::<Vec<_>>(),
            [
                InterfaceInspectionRecordKind::CallableSignatures,
                InterfaceInspectionRecordKind::GenericDeclarations,
                InterfaceInspectionRecordKind::CallableParameterDefaults,
                InterfaceInspectionRecordKind::PredicateDefinitions,
                InterfaceInspectionRecordKind::TypeRepresentations,
            ]
        );
    }

    #[test]
    fn selected_semantic_sections_share_one_aggregate_allocation_preflight() {
        let bundle = package_interface_export_bundle();

        let sections = [
            EncodedArtifactSection::new(InterfaceSectionTag::SemanticTypes, 0, vec![0; 4]),
            EncodedArtifactSection::new(InterfaceSectionTag::Constants, 0, vec![0; 4]),
        ];

        let artifact = assemble_sections(
            &sections,
            bundle.surface().identity().clone(),
            bundle.language_revision(),
        )
        .unwrap_or_else(|error| panic!("test sections must assemble: {error:?}"));

        let limits = crate::InterfaceValidationLimits::default().with_decoded_allocation(64);

        let interface = ValidatedPackageInterface::try_new(
            artifact.shared_bytes(),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)).with_limits(limits),
        )
        .unwrap_or_else(|error| panic!("test interface envelope must validate: {error:?}"));

        let error = match interface.inspect(&[
            InterfaceSectionTag::SemanticTypes,
            InterfaceSectionTag::Constants,
        ]) {
            Ok(_) => panic!("aggregate semantic allocation must be rejected"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            crate::InterfaceValidationError::ResourceLimitExceeded {
                limit: crate::InterfaceLimit::DecodedAllocation,
                actual: 128,
                maximum: 64,
            }
        );
    }

    #[test]
    fn inspection_decodes_only_selected_section_payloads() {
        let bundle = package_interface_export_bundle();
        let mut sections = encoded_sections(&bundle);

        let template_section = sections
            .iter_mut()
            .find(|section| section.tag() == InterfaceSectionTag::DeclarationTemplates)
            .unwrap_or_else(|| panic!("test template section must be present"));

        template_section.payload_mut().fill(u8::MAX);

        let artifact = assemble_sections(
            &sections,
            bundle.surface().identity().clone(),
            bundle.language_revision(),
        )
        .unwrap_or_else(|error| panic!("test sections must assemble: {error:?}"));

        let interface = ValidatedPackageInterface::try_new(
            artifact.shared_bytes(),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
        .unwrap_or_else(|error| panic!("test interface envelope must validate: {error:?}"));

        let inspection = interface
            .inspect(&[InterfaceSectionTag::TargetDependencies])
            .unwrap_or_else(|error| panic!("selected section must inspect: {error:?}"));

        assert_eq!(inspection.sections().len(), 1);

        assert_eq!(
            inspection.sections()[0]
                .records()
                .iter()
                .map(|record| record.kind())
                .collect::<Vec<_>>(),
            [
                InterfaceInspectionRecordKind::TargetProperties,
                InterfaceInspectionRecordKind::AbiDependencies,
                InterfaceInspectionRecordKind::RuntimeRequirements,
            ]
        );

        assert!(
            interface
                .inspect(&[InterfaceSectionTag::DeclarationTemplates])
                .is_err()
        );

        assert!(interface.validate_complete().is_err());
    }

    fn validated_test_interface() -> ValidatedPackageInterface {
        let bundle = package_interface_export_bundle();

        let artifact = crate::encode_package_interface(&bundle)
            .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

        ValidatedPackageInterface::try_new(
            artifact.shared_bytes(),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
        .unwrap_or_else(|error| panic!("test interface must validate: {error:?}"))
    }

    fn encoded_sections(
        bundle: &crate::PackageInterfaceExportBundle,
    ) -> Vec<EncodedArtifactSection> {
        let mut sections = encode_surface(bundle.surface())
            .into_iter()
            .map(EncodedArtifactSection::from_surface)
            .chain(
                crate::semantic::encode_validated_semantics(bundle.semantics())
                    .into_iter()
                    .map(EncodedArtifactSection::from_semantic),
            )
            .collect::<Vec<_>>();

        sections.sort_by_key(|section| section.tag());

        sections
    }
}
