/// Configurable resource category bounded by package-interface validation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceLimit {
    /// Complete artifact byte length.
    FileSize,
    /// Number of entries in the section directory.
    SectionCount,
    /// Number of records declared by one section.
    RecordCount,
    /// Byte length of one decoded string.
    StringLength,
    /// Byte length of one decoded blob.
    BlobLength,
    /// Total allocation charged while decoding one artifact.
    DecodedAllocation,
    /// Nesting depth of one semantic type.
    SemanticTypeDepth,
    /// Number of nodes in one checked template graph.
    TemplateGraphSize,
    /// Number of external references in one artifact.
    ExternalReferenceCount,
}

/// Resource ceilings applied to untrusted package-interface input.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InterfaceValidationLimits {
    file_size: u64,
    section_count: u64,
    records_per_section: u64,
    string_length: u64,
    blob_length: u64,
    decoded_allocation: u64,
    semantic_type_depth: u64,
    template_graph_size: u64,
    external_reference_count: u64,
}

impl InterfaceValidationLimits {
    /// Default ceilings for compiler package-interface loading.
    pub const DEFAULT: Self = Self {
        file_size: 256 * 1024 * 1024,
        section_count: 64,
        records_per_section: 10_000_000,
        string_length: 16 * 1024 * 1024,
        blob_length: 16 * 1024 * 1024,
        decoded_allocation: 512 * 1024 * 1024,
        semantic_type_depth: 256,
        template_graph_size: 1_000_000,
        external_reference_count: 10_000_000,
    };

    /// Returns a copy with the maximum artifact byte length replaced.
    pub const fn with_file_size(mut self, maximum: u64) -> Self {
        self.file_size = maximum;
        self
    }

    /// Returns a copy with the maximum section count replaced.
    pub const fn with_section_count(mut self, maximum: u64) -> Self {
        self.section_count = maximum;
        self
    }

    /// Returns a copy with the maximum records per section replaced.
    pub const fn with_records_per_section(mut self, maximum: u64) -> Self {
        self.records_per_section = maximum;
        self
    }

    /// Returns a copy with the maximum string byte length replaced.
    pub const fn with_string_length(mut self, maximum: u64) -> Self {
        self.string_length = maximum;
        self
    }

    /// Returns a copy with the maximum blob byte length replaced.
    pub const fn with_blob_length(mut self, maximum: u64) -> Self {
        self.blob_length = maximum;
        self
    }

    /// Returns a copy with the total decoded allocation ceiling replaced.
    pub const fn with_decoded_allocation(mut self, maximum: u64) -> Self {
        self.decoded_allocation = maximum;
        self
    }

    /// Returns a copy with the semantic type nesting ceiling replaced.
    pub const fn with_semantic_type_depth(mut self, maximum: u64) -> Self {
        self.semantic_type_depth = maximum;
        self
    }

    /// Returns a copy with the checked-template graph ceiling replaced.
    pub const fn with_template_graph_size(mut self, maximum: u64) -> Self {
        self.template_graph_size = maximum;
        self
    }

    /// Returns a copy with the external-reference ceiling replaced.
    pub const fn with_external_reference_count(mut self, maximum: u64) -> Self {
        self.external_reference_count = maximum;
        self
    }

    /// Returns the configured ceiling for one resource category.
    pub const fn maximum(self, limit: InterfaceLimit) -> u64 {
        match limit {
            InterfaceLimit::FileSize => self.file_size,
            InterfaceLimit::SectionCount => self.section_count,
            InterfaceLimit::RecordCount => self.records_per_section,
            InterfaceLimit::StringLength => self.string_length,
            InterfaceLimit::BlobLength => self.blob_length,
            InterfaceLimit::DecodedAllocation => self.decoded_allocation,
            InterfaceLimit::SemanticTypeDepth => self.semantic_type_depth,
            InterfaceLimit::TemplateGraphSize => self.template_graph_size,
            InterfaceLimit::ExternalReferenceCount => self.external_reference_count,
        }
    }

    /// Checks one externally controlled count against its configured ceiling.
    pub fn check(
        self,
        limit: InterfaceLimit,
        actual: u64,
    ) -> Result<(), crate::InterfaceValidationError> {
        let maximum = self.maximum(limit);

        if actual > maximum {
            return Err(crate::InterfaceValidationError::ResourceLimitExceeded {
                limit,
                actual,
                maximum,
            });
        }

        Ok(())
    }
}

impl Default for InterfaceValidationLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Exact compatibility and resource policy used to validate one artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InterfaceValidationPolicy {
    language_revision: crate::InterfaceLanguageRevision,
    limits: InterfaceValidationLimits,
}

impl InterfaceValidationPolicy {
    /// Creates a validation policy for one exact language semantic revision.
    pub const fn new(language_revision: crate::InterfaceLanguageRevision) -> Self {
        Self {
            language_revision,
            limits: InterfaceValidationLimits::DEFAULT,
        }
    }

    /// Returns a copy using `limits` for untrusted input.
    pub const fn with_limits(mut self, limits: InterfaceValidationLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Returns the exact accepted language semantic revision.
    pub const fn language_revision(self) -> crate::InterfaceLanguageRevision {
        self.language_revision
    }

    /// Returns the configured validation limits.
    pub const fn limits(self) -> InterfaceValidationLimits {
        self.limits
    }
}

#[cfg(test)]
mod tests {
    use super::{InterfaceLimit, InterfaceValidationLimits};
    use crate::InterfaceValidationError;

    #[test]
    fn limits_are_independently_configurable() {
        let limits = InterfaceValidationLimits::default()
            .with_file_size(1)
            .with_section_count(2)
            .with_records_per_section(3)
            .with_string_length(4)
            .with_blob_length(5)
            .with_decoded_allocation(6)
            .with_semantic_type_depth(7)
            .with_template_graph_size(8)
            .with_external_reference_count(9);

        assert_eq!(limits.maximum(InterfaceLimit::FileSize), 1);
        assert_eq!(limits.maximum(InterfaceLimit::SectionCount), 2);
        assert_eq!(limits.maximum(InterfaceLimit::RecordCount), 3);
        assert_eq!(limits.maximum(InterfaceLimit::StringLength), 4);
        assert_eq!(limits.maximum(InterfaceLimit::BlobLength), 5);
        assert_eq!(limits.maximum(InterfaceLimit::DecodedAllocation), 6);
        assert_eq!(limits.maximum(InterfaceLimit::SemanticTypeDepth), 7);
        assert_eq!(limits.maximum(InterfaceLimit::TemplateGraphSize), 8);
        assert_eq!(limits.maximum(InterfaceLimit::ExternalReferenceCount), 9);
    }

    #[test]
    fn every_limit_category_uses_the_same_inclusive_boundary_contract() {
        let limits = InterfaceValidationLimits::default()
            .with_file_size(1)
            .with_section_count(1)
            .with_records_per_section(1)
            .with_string_length(1)
            .with_blob_length(1)
            .with_decoded_allocation(1)
            .with_semantic_type_depth(1)
            .with_template_graph_size(1)
            .with_external_reference_count(1);

        let categories = [
            InterfaceLimit::FileSize,
            InterfaceLimit::SectionCount,
            InterfaceLimit::RecordCount,
            InterfaceLimit::StringLength,
            InterfaceLimit::BlobLength,
            InterfaceLimit::DecodedAllocation,
            InterfaceLimit::SemanticTypeDepth,
            InterfaceLimit::TemplateGraphSize,
            InterfaceLimit::ExternalReferenceCount,
        ];

        for limit in categories {
            assert_eq!(limits.check(limit, 1), Ok(()));
            assert_eq!(
                limits.check(limit, 2),
                Err(InterfaceValidationError::ResourceLimitExceeded {
                    limit,
                    actual: 2,
                    maximum: 1,
                })
            );
        }
    }
}
