use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an interface-local record index argument.
    pub const fn interface_record_index(index: u32) -> Self {
        Self::new(
            DiagnosticArgName::InterfaceRecordIndex,
            DiagnosticArgValue::Count(index as u64),
        )
    }

    /// Creates an expected package-interface record-index argument.
    pub const fn expected_interface_record_index(index: u32) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedInterfaceRecordIndex,
            DiagnosticArgValue::Count(index as u64),
        )
    }

    /// Creates an actual package-interface record-index argument.
    pub const fn actual_interface_record_index(index: u32) -> Self {
        Self::new(
            DiagnosticArgName::ActualInterfaceRecordIndex,
            DiagnosticArgValue::Count(index as u64),
        )
    }

    /// Creates a related package-interface record-index argument.
    pub const fn related_interface_record_index(index: u32) -> Self {
        Self::new(
            DiagnosticArgName::RelatedInterfaceRecordIndex,
            DiagnosticArgValue::Count(index as u64),
        )
    }

    /// Creates an interface dependency-table index argument.
    pub const fn interface_dependency_index(index: u32) -> Self {
        Self::new(
            DiagnosticArgName::InterfaceDependencyIndex,
            DiagnosticArgValue::Count(index as u64),
        )
    }
}
