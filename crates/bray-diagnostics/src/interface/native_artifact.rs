/// Exact failure while authenticating native units inside a package implementation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNativeArtifactCause {
    /// index size limit exceeded in the native package artifact.
    IndexSizeLimitExceeded,
    /// index malformed in the native package artifact.
    IndexMalformed,
    /// index unsupported schema in the native package artifact.
    IndexUnsupportedSchema,
    /// index invalid target in the native package artifact.
    IndexInvalidTarget,
    /// index invalid digest in the native package artifact.
    IndexInvalidDigest,
    /// index invalid symbol in the native package artifact.
    IndexInvalidSymbol,
    /// index invalid link in the native package artifact.
    IndexInvalidLink,
    /// index digest mismatch in the native package artifact.
    IndexDigestMismatch,
    /// payload digest mismatch in the native package artifact.
    PayloadDigestMismatch,
    /// wrong target in the native package artifact.
    WrongTarget,
    /// wrong producer in the native package artifact.
    WrongProducer,
    /// read failure in the native package artifact.
    ReadFailure,
    /// duplicate unit in the native package artifact.
    DuplicateUnit,
    /// invalid summary in the native package artifact.
    InvalidSummary,
    /// duplicate definition in the native package artifact.
    DuplicateDefinition,
    /// invalid association in the native package artifact.
    InvalidAssociation,
    /// invalid link option in the native package artifact.
    InvalidLinkOption,
    /// noncanonical summary in the native package artifact.
    NoncanonicalSummary,
    /// missing co retention member in the native package artifact.
    MissingCoRetentionMember,
    /// duplicate co retention group in the native package artifact.
    DuplicateCoRetentionGroup,
    /// invalid co retention group in the native package artifact.
    InvalidCoRetentionGroup,
    /// unsupported target in the native package artifact.
    UnsupportedTarget,
    /// missing index in the native package artifact.
    MissingIndex,
    /// missing unit in the native package artifact.
    MissingUnit,
    /// unindexed unit in the native package artifact.
    UnindexedUnit,
    /// invalid binding in the native package artifact.
    InvalidBinding,
}

impl DiagnosticNativeArtifactCause {
    /// Returns the stable machine key for this native validation failure.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IndexSizeLimitExceeded => "index_size_limit_exceeded",
            Self::IndexMalformed => "index_malformed",
            Self::IndexUnsupportedSchema => "index_unsupported_schema",
            Self::IndexInvalidTarget => "index_invalid_target",
            Self::IndexInvalidDigest => "index_invalid_digest",
            Self::IndexInvalidSymbol => "index_invalid_symbol",
            Self::IndexInvalidLink => "index_invalid_link",
            Self::IndexDigestMismatch => "index_digest_mismatch",
            Self::PayloadDigestMismatch => "payload_digest_mismatch",
            Self::WrongTarget => "wrong_target",
            Self::WrongProducer => "wrong_producer",
            Self::ReadFailure => "read_failure",
            Self::DuplicateUnit => "duplicate_unit",
            Self::InvalidSummary => "invalid_summary",
            Self::DuplicateDefinition => "duplicate_definition",
            Self::InvalidAssociation => "invalid_association",
            Self::InvalidLinkOption => "invalid_link_option",
            Self::NoncanonicalSummary => "noncanonical_summary",
            Self::MissingCoRetentionMember => "missing_co_retention_member",
            Self::DuplicateCoRetentionGroup => "duplicate_co_retention_group",
            Self::InvalidCoRetentionGroup => "invalid_co_retention_group",
            Self::UnsupportedTarget => "unsupported_target",
            Self::MissingIndex => "missing_index",
            Self::MissingUnit => "missing_unit",
            Self::UnindexedUnit => "unindexed_unit",
            Self::InvalidBinding => "invalid_binding",
        }
    }
}
