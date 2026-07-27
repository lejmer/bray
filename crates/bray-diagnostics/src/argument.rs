// rust-style: allow(module-too-large, reason = "diagnostic argument names, values, and typed constructors form one cohesive protocol inventory")

use std::path::PathBuf;

use bray_source::{SourceInputKind, SourceSpan, TextSize};
use bray_syntax::SyntaxKind;

use crate::{DiagnosticInterfaceLimit, DiagnosticInterfaceSection};

/// Stable typed argument attached to a diagnostic message component.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticArg {
    name: DiagnosticArgName,
    value: DiagnosticArgValue,
}

impl DiagnosticArg {
    /// Creates a typed diagnostic argument.
    pub const fn new(name: DiagnosticArgName, value: DiagnosticArgValue) -> Self {
        Self { name, value }
    }

    /// Creates a byte-count argument from a platform byte count.
    pub fn byte_count(byte_count: usize) -> Option<Self> {
        let byte_count = u64::try_from(byte_count).ok()?;

        Some(Self::new(
            DiagnosticArgName::ByteCount,
            DiagnosticArgValue::ByteCount(byte_count),
        ))
    }

    /// Creates a file-path argument.
    pub fn file_path(path: impl Into<PathBuf>) -> Self {
        Self::new(
            DiagnosticArgName::FilePath,
            DiagnosticArgValue::FilePath(path.into()),
        )
    }

    /// Creates an external package-interface artifact-path argument.
    pub fn artifact_path(path: impl Into<PathBuf>) -> Self {
        Self::new(
            DiagnosticArgName::ArtifactPath,
            DiagnosticArgValue::FilePath(path.into()),
        )
    }

    /// Creates an expected semantic-type argument.
    pub const fn expected_type(ty: DiagnosticType) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedType,
            DiagnosticArgValue::Type(ty),
        )
    }

    /// Creates an actual semantic-type argument.
    pub const fn actual_type(ty: DiagnosticType) -> Self {
        Self::new(DiagnosticArgName::ActualType, DiagnosticArgValue::Type(ty))
    }

    /// Creates a semantic-selection category argument.
    pub const fn selection_kind(kind: DiagnosticSelectionKind) -> Self {
        Self::new(
            DiagnosticArgName::SelectionKind,
            DiagnosticArgValue::SelectionKind(kind),
        )
    }

    /// Creates a target representation argument.
    pub const fn target_representation(kind: DiagnosticTargetRepresentation) -> Self {
        Self::new(
            DiagnosticArgName::TargetRepresentation,
            DiagnosticArgValue::TargetRepresentation(kind),
        )
    }

    /// Creates a callable ABI argument.
    pub const fn callable_abi(abi: DiagnosticCallableAbi) -> Self {
        Self::new(
            DiagnosticArgName::CallableAbi,
            DiagnosticArgValue::CallableAbi(abi),
        )
    }

    /// Creates an alignment-purpose argument.
    pub const fn alignment_kind(kind: DiagnosticAlignmentKind) -> Self {
        Self::new(
            DiagnosticArgName::AlignmentKind,
            DiagnosticArgValue::AlignmentKind(kind),
        )
    }

    /// Creates a required alignment argument.
    pub const fn required_alignment(alignment: u64) -> Self {
        Self::new(
            DiagnosticArgName::RequiredAlignment,
            DiagnosticArgValue::Count(alignment),
        )
    }

    /// Creates a maximum supported alignment argument.
    pub const fn maximum_alignment(alignment: u64) -> Self {
        Self::new(
            DiagnosticArgName::MaximumAlignment,
            DiagnosticArgValue::Count(alignment),
        )
    }

    /// Creates an artifact-kind argument.
    pub const fn artifact_kind(kind: DiagnosticArtifactKind) -> Self {
        Self::new(
            DiagnosticArgName::ArtifactKind,
            DiagnosticArgValue::ArtifactKind(kind),
        )
    }

    /// Creates a same-kind artifact ordinal argument.
    pub const fn artifact_ordinal(ordinal: u32) -> Self {
        Self::new(
            DiagnosticArgName::ArtifactOrdinal,
            DiagnosticArgValue::ArtifactOrdinal(ordinal),
        )
    }

    /// Creates an expected artifact byte-count argument.
    pub const fn expected_byte_count(byte_count: u64) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedByteCount,
            DiagnosticArgValue::ByteCount(byte_count),
        )
    }

    /// Creates an actual artifact byte-count argument.
    pub const fn actual_byte_count(byte_count: u64) -> Self {
        Self::new(
            DiagnosticArgName::ActualByteCount,
            DiagnosticArgValue::ByteCount(byte_count),
        )
    }

    /// Creates an expected artifact-digest argument.
    pub const fn expected_artifact_digest(digest: DiagnosticArtifactDigest) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedArtifactDigest,
            DiagnosticArgValue::ArtifactDigest(digest),
        )
    }

    /// Creates an actual artifact-digest argument.
    pub const fn actual_artifact_digest(digest: DiagnosticArtifactDigest) -> Self {
        Self::new(
            DiagnosticArgName::ActualArtifactDigest,
            DiagnosticArgValue::ArtifactDigest(digest),
        )
    }

    /// Creates an output-sink argument.
    pub const fn output_sink(sink: DiagnosticOutputSink) -> Self {
        Self::new(
            DiagnosticArgName::OutputSink,
            DiagnosticArgValue::OutputSink(sink),
        )
    }

    /// Creates an expected package-identity argument.
    pub fn expected_package_identity(identity: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedPackageIdentity,
            DiagnosticArgValue::PackageIdentity(identity.into()),
        )
    }

    /// Creates an expected package-product identity argument.
    pub fn expected_product_identity(identity: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedProductIdentity,
            DiagnosticArgValue::ProductIdentity(identity.into()),
        )
    }

    /// Creates a zero-based input-index argument from a platform index.
    pub fn input_index(input_index: usize) -> Option<Self> {
        let input_index = u64::try_from(input_index).ok()?;

        Some(Self::new(
            DiagnosticArgName::InputIndex,
            DiagnosticArgValue::InputIndex(input_index),
        ))
    }

    /// Creates an I/O error-kind argument.
    pub const fn io_error_kind(kind: DiagnosticIoErrorKind) -> Self {
        Self::new(
            DiagnosticArgName::IoErrorKind,
            DiagnosticArgValue::IoErrorKind(kind),
        )
    }

    /// Creates a source-count argument.
    pub const fn source_count(source_count: u64) -> Self {
        Self::new(
            DiagnosticArgName::SourceCount,
            DiagnosticArgValue::SourceCount(source_count),
        )
    }

    /// Creates a maximum accepted count argument.
    pub const fn maximum_count(maximum_count: u64) -> Self {
        Self::new(
            DiagnosticArgName::MaximumCount,
            DiagnosticArgValue::Count(maximum_count),
        )
    }

    /// Creates a source-input-kind argument.
    pub const fn source_input_kind(kind: SourceInputKind) -> Self {
        Self::new(
            DiagnosticArgName::SourceInputKind,
            DiagnosticArgValue::SourceInputKind(kind),
        )
    }

    /// Creates an expected syntax-kind argument.
    pub const fn expected_syntax_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedSyntaxKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates an actual syntax-kind argument.
    pub const fn actual_syntax_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::ActualSyntaxKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates a declaration-modifier syntax-kind argument.
    pub const fn modifier_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::ModifierKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates a conflicting declaration-modifier syntax-kind argument.
    pub const fn conflicting_modifier_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::ConflictingModifierKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates a declaration-directive syntax-kind argument.
    pub const fn directive_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::DirectiveKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates a conflicting declaration-directive syntax-kind argument.
    pub const fn conflicting_directive_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::ConflictingDirectiveKind,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates a declaration-name argument.
    pub fn declaration_name(name: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::DeclarationName,
            DiagnosticArgValue::DeclarationName(name.into()),
        )
    }

    /// Creates a named trait-member argument.
    pub fn trait_member_name(name: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::TraitMemberName,
            DiagnosticArgValue::DeclarationName(name.into()),
        )
    }

    /// Creates a fixed-keyword trait-member argument.
    pub const fn trait_member_kind(kind: SyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::TraitMemberName,
            DiagnosticArgValue::SyntaxKind(kind),
        )
    }

    /// Creates a referenced-name argument.
    pub fn referenced_name(name: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ReferencedName,
            DiagnosticArgValue::ReferencedName(name.into()),
        )
    }

    /// Creates an expected semantic-name category argument.
    pub const fn expected_name_kind(kind: DiagnosticNameKind) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedNameKind,
            DiagnosticArgValue::NameKind(kind),
        )
    }

    /// Creates an expected declaration-visibility argument.
    pub const fn expected_visibility(visibility: DiagnosticVisibility) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedVisibility,
            DiagnosticArgValue::Visibility(visibility),
        )
    }

    /// Creates an actual declaration-visibility argument.
    pub const fn actual_visibility(visibility: DiagnosticVisibility) -> Self {
        Self::new(
            DiagnosticArgName::ActualVisibility,
            DiagnosticArgValue::Visibility(visibility),
        )
    }

    /// Creates an expected module-trust argument.
    pub const fn expected_module_trust(trust: DiagnosticModuleTrust) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedModuleTrust,
            DiagnosticArgValue::ModuleTrust(trust),
        )
    }

    /// Creates an actual module-trust argument.
    pub const fn actual_module_trust(trust: DiagnosticModuleTrust) -> Self {
        Self::new(
            DiagnosticArgName::ActualModuleTrust,
            DiagnosticArgValue::ModuleTrust(trust),
        )
    }

    /// Creates a source-name argument.
    pub fn source_name(name: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::SourceName,
            DiagnosticArgValue::SourceName(name.into()),
        )
    }

    /// Creates a source text-offset argument.
    pub const fn text_offset(offset: TextSize) -> Self {
        Self::new(
            DiagnosticArgName::TextOffset,
            DiagnosticArgValue::TextOffset(offset),
        )
    }

    /// Creates an exact source-token text argument.
    pub fn token_text(text: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::TokenText,
            DiagnosticArgValue::TokenText(text.into()),
        )
    }

    /// Creates a URI argument.
    pub fn uri(uri: impl Into<String>) -> Self {
        Self::new(DiagnosticArgName::Uri, DiagnosticArgValue::Uri(uri.into()))
    }

    /// Creates a worker-count argument.
    pub const fn worker_count(worker_count: u64) -> Self {
        Self::new(
            DiagnosticArgName::WorkerCount,
            DiagnosticArgValue::WorkerCount(worker_count),
        )
    }

    /// Returns the stable argument name.
    pub const fn name(&self) -> DiagnosticArgName {
        self.name
    }

    /// Returns the typed argument value.
    pub const fn value(&self) -> &DiagnosticArgValue {
        &self.value
    }
}

/// Stable name for a diagnostic argument.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArgName {
    /// Actual externally supplied count or size.
    ActualCount,
    /// Actual completed artifact byte count.
    ActualByteCount,
    /// Actual digest measured from completed artifact bytes.
    ActualArtifactDigest,
    /// Path of an external compiler artifact.
    ArtifactPath,
    /// Category of compiler artifact involved in an operation.
    ArtifactKind,
    /// Stable same-category artifact ordinal.
    ArtifactOrdinal,
    /// Target representation rejected by the selected target or ABI.
    TargetRepresentation,
    /// Selected foreign callable ABI.
    CallableAbi,
    /// Storage, allocation, or ABI alignment surface.
    AlignmentKind,
    /// Alignment required by a selected layout.
    RequiredAlignment,
    /// Maximum alignment accepted by the target surface.
    MaximumAlignment,
    /// Actual interface or language revision.
    ActualRevision,
    /// Semantic type found by checking.
    ActualType,
    /// Byte that participates in the diagnostic.
    Byte,
    /// Number of bytes that participate in the diagnostic.
    ByteCount,
    /// Character that participates in the diagnostic.
    Character,
    /// Start location of a block comment or another paired source construct.
    ConstructStart,
    /// Source-level declaration name.
    DeclarationName,
    /// Named or fixed-keyword trait member.
    TraitMemberName,
    /// Name spelling used by a semantic reference.
    ReferencedName,
    /// Semantic category required at a name reference.
    ExpectedNameKind,
    /// Expected completed artifact byte count.
    ExpectedByteCount,
    /// Expected producer-supplied artifact digest.
    ExpectedArtifactDigest,
    /// Syntax kind that was present in source.
    ActualSyntaxKind,
    /// Syntax kind that was expected by the compiler phase.
    ExpectedSyntaxKind,
    /// Declaration modifier involved in a declaration diagnostic.
    ModifierKind,
    /// Second modifier that conflicts with another declaration modifier.
    ConflictingModifierKind,
    /// Declaration directive involved in a declaration diagnostic.
    DirectiveKind,
    /// Second directive that conflicts with another declaration directive.
    ConflictingDirectiveKind,
    /// Package identity selected by package resolution.
    ExpectedPackageIdentity,
    /// Package-product identity selected by package resolution.
    ExpectedProductIdentity,
    /// Effective visibility established by an earlier declaration.
    ExpectedVisibility,
    /// Effective visibility supplied by the current declaration.
    ActualVisibility,
    /// Module trust state established by an earlier declaration.
    ExpectedModuleTrust,
    /// Module trust state supplied by the current declaration.
    ActualModuleTrust,
    /// Path of a source file or external artifact.
    FilePath,
    /// Zero-based source input index from the request boundary.
    InputIndex,
    /// Package-interface resource category.
    InterfaceLimit,
    /// Package-interface section category.
    InterfaceSection,
    /// Stable I/O error category from the host.
    IoErrorKind,
    /// Typed external output destination.
    OutputSink,
    /// Maximum accepted count or size.
    MaximumCount,
    /// Expected interface or language revision.
    ExpectedRevision,
    /// Semantic type required by checking.
    ExpectedType,
    /// Name of a virtual, generated, or test-fixture source.
    SourceName,
    /// Number of source inputs involved in the diagnostic.
    SourceCount,
    /// Semantic operation category being selected.
    SelectionKind,
    /// Stable source input category.
    SourceInputKind,
    /// Byte offset inside a source input.
    TextOffset,
    /// Exact token source text that participates in the diagnostic.
    TokenText,
    /// URI of an LSP or other URI-backed source input.
    Uri,
    /// Source span that participates in the diagnostic.
    SourceSpan,
    /// Worker count requested at the driver or compilation boundary.
    WorkerCount,
}

impl DiagnosticArgName {
    /// Returns the stable machine key for this argument name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ActualCount => "actual_count",
            Self::ActualByteCount => "actual_byte_count",
            Self::ActualArtifactDigest => "actual_artifact_digest",
            Self::ArtifactPath => "artifact_path",
            Self::ArtifactKind => "artifact_kind",
            Self::ArtifactOrdinal => "artifact_ordinal",
            Self::TargetRepresentation => "target_representation",
            Self::CallableAbi => "callable_abi",
            Self::AlignmentKind => "alignment_kind",
            Self::RequiredAlignment => "required_alignment",
            Self::MaximumAlignment => "maximum_alignment",
            Self::ActualRevision => "actual_revision",
            Self::ActualType => "actual_type",
            Self::Byte => "byte",
            Self::ByteCount => "byte_count",
            Self::Character => "character",
            Self::ConstructStart => "construct_start",
            Self::DeclarationName => "declaration_name",
            Self::TraitMemberName => "trait_member_name",
            Self::ReferencedName => "referenced_name",
            Self::ExpectedNameKind => "expected_name_kind",
            Self::ExpectedByteCount => "expected_byte_count",
            Self::ExpectedArtifactDigest => "expected_artifact_digest",
            Self::ActualSyntaxKind => "actual_syntax_kind",
            Self::ExpectedSyntaxKind => "expected_syntax_kind",
            Self::ModifierKind => "modifier_kind",
            Self::ConflictingModifierKind => "conflicting_modifier_kind",
            Self::DirectiveKind => "directive_kind",
            Self::ConflictingDirectiveKind => "conflicting_directive_kind",
            Self::ExpectedPackageIdentity => "expected_package_identity",
            Self::ExpectedProductIdentity => "expected_product_identity",
            Self::ExpectedVisibility => "expected_visibility",
            Self::ActualVisibility => "actual_visibility",
            Self::ExpectedModuleTrust => "expected_module_trust",
            Self::ActualModuleTrust => "actual_module_trust",
            Self::FilePath => "file_path",
            Self::InputIndex => "input_index",
            Self::InterfaceLimit => "interface_limit",
            Self::InterfaceSection => "interface_section",
            Self::IoErrorKind => "io_error_kind",
            Self::OutputSink => "output_sink",
            Self::MaximumCount => "maximum_count",
            Self::ExpectedRevision => "expected_revision",
            Self::ExpectedType => "expected_type",
            Self::SourceName => "source_name",
            Self::SourceCount => "source_count",
            Self::SelectionKind => "selection_kind",
            Self::SourceInputKind => "source_input_kind",
            Self::TextOffset => "text_offset",
            Self::TokenText => "token_text",
            Self::Uri => "uri",
            Self::SourceSpan => "source_span",
            Self::WorkerCount => "worker_count",
        }
    }
}

/// Locale-neutral typed value for a diagnostic argument.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArgValue {
    /// Externally supplied or configured count.
    Count(u64),
    /// Raw source byte.
    Byte(u8),
    /// Source byte count.
    ByteCount(u64),
    /// Deterministic compiler artifact digest.
    ArtifactDigest(DiagnosticArtifactDigest),
    /// Compiler artifact category.
    ArtifactKind(DiagnosticArtifactKind),
    /// Same-category artifact ordinal.
    ArtifactOrdinal(u32),
    /// Target representation category.
    TargetRepresentation(DiagnosticTargetRepresentation),
    /// Foreign callable ABI mode.
    CallableAbi(DiagnosticCallableAbi),
    /// Alignment validation surface.
    AlignmentKind(DiagnosticAlignmentKind),
    /// Source character.
    Character(char),
    /// Source-level declaration name.
    DeclarationName(String),
    /// Name spelling used by a semantic reference.
    ReferencedName(String),
    /// Stable package identity.
    PackageIdentity(String),
    /// Stable package-product identity.
    ProductIdentity(String),
    /// Semantic category required at a name reference.
    NameKind(DiagnosticNameKind),
    /// Source file or external artifact path.
    FilePath(PathBuf),
    /// Zero-based source input index.
    InputIndex(u64),
    /// Package-interface resource category.
    InterfaceLimit(DiagnosticInterfaceLimit),
    /// Package-interface section category.
    InterfaceSection(DiagnosticInterfaceSection),
    /// Stable I/O error category from the host.
    IoErrorKind(DiagnosticIoErrorKind),
    /// Typed external output destination.
    OutputSink(DiagnosticOutputSink),
    /// Effective declaration visibility.
    Visibility(DiagnosticVisibility),
    /// Effective module trust state.
    ModuleTrust(DiagnosticModuleTrust),
    /// Source name.
    SourceName(String),
    /// Source input count.
    SourceCount(u64),
    /// Stable source input category.
    SourceInputKind(SourceInputKind),
    /// Syntax vocabulary kind.
    SyntaxKind(SyntaxKind),
    /// Byte offset inside source text.
    TextOffset(TextSize),
    /// Exact token source text.
    TokenText(String),
    /// URI string.
    Uri(String),
    /// Source span.
    SourceSpan(SourceSpan),
    /// Requested worker count.
    WorkerCount(u64),
    /// Interface or language revision.
    Revision(u64),
    /// Locale-neutral semantic type shape.
    Type(DiagnosticType),
    /// Semantic operation category being selected.
    SelectionKind(DiagnosticSelectionKind),
}

/// Locale-neutral target representation categories used by target diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticTargetRepresentation {
    Bool,
    Char,
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    Isize,
    Usize,
    R16,
    R32,
    R64,
    R128,
    C32,
    C64,
    C128,
    C256,
    RawPointer,
    AbiQualifiedCallable,
    DefaultLayoutAggregate,
    StableLayoutAggregate,
    CLayoutAggregate,
    TransparentLayoutAggregate,
}

impl DiagnosticTargetRepresentation {
    /// Returns the stable machine key for this representation category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Char => "char",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::Isize => "isize",
            Self::Usize => "usize",
            Self::R16 => "r16",
            Self::R32 => "r32",
            Self::R64 => "r64",
            Self::R128 => "r128",
            Self::C32 => "c32",
            Self::C64 => "c64",
            Self::C128 => "c128",
            Self::C256 => "c256",
            Self::RawPointer => "raw_pointer",
            Self::AbiQualifiedCallable => "abi_qualified_callable",
            Self::DefaultLayoutAggregate => "default_layout_aggregate",
            Self::StableLayoutAggregate => "stable_layout_aggregate",
            Self::CLayoutAggregate => "c_layout_aggregate",
            Self::TransparentLayoutAggregate => "transparent_layout_aggregate",
        }
    }
}

/// Locale-neutral callable ABI modes used by target diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableAbi {
    /// The target's C ABI.
    C,
    /// The target's system ABI.
    System,
}

impl DiagnosticCallableAbi {
    /// Returns the stable machine key for this ABI mode.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::C => "c",
            Self::System => "system",
        }
    }
}

/// Locale-neutral alignment validation surfaces.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticAlignmentKind {
    /// Ordinary value storage.
    Storage,
    /// Dynamic allocation.
    Allocation,
    /// A foreign callable ABI boundary.
    CallableAbi,
}

impl DiagnosticAlignmentKind {
    /// Returns the stable machine key for this alignment surface.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Storage => "storage",
            Self::Allocation => "allocation",
            Self::CallableAbi => "callable_abi",
        }
    }
}

/// Locale-neutral semantic operation categories used by selection diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSelectionKind {
    /// A callable or overload arm.
    Callable,
    /// A receiver-associated member.
    Member,
    /// A unary or binary operator implementation.
    Operator,
    /// An element or slice indexing contract.
    Index,
    /// A struct, variant, or type-form construction operation.
    Construction,
    /// An explicit conversion operation.
    Conversion,
    /// A trait implementation witness.
    Implementation,
    /// An iterable and iterator protocol pair for one source expression.
    IterationSource,
}

impl DiagnosticSelectionKind {
    /// Returns the stable machine key for this semantic operation category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Callable => "callable",
            Self::Member => "member",
            Self::Operator => "operator",
            Self::Index => "index",
            Self::Construction => "construction",
            Self::Conversion => "conversion",
            Self::Implementation => "implementation",
            Self::IterationSource => "iteration_source",
        }
    }
}

/// Locale-neutral semantic type categories used by structured diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticType {
    /// The canonical recovery type.
    Error,
    /// The Boolean scalar type.
    Boolean,
    /// The character scalar type.
    Character,
    /// The 8-bit signed integer type.
    I8,
    /// The 16-bit signed integer type.
    I16,
    /// The 32-bit signed integer type.
    I32,
    /// The 64-bit signed integer type.
    I64,
    /// The 128-bit signed integer type.
    I128,
    /// The 8-bit unsigned integer type.
    U8,
    /// The 16-bit unsigned integer type.
    U16,
    /// The 32-bit unsigned integer type.
    U32,
    /// The 64-bit unsigned integer type.
    U64,
    /// The 128-bit unsigned integer type.
    U128,
    /// The machine-sized signed integer type.
    Isize,
    /// The machine-sized unsigned integer type.
    Usize,
    /// The unit type.
    Unit,
    /// The uninhabited type.
    Never,
    /// The string type.
    String,
    /// The 16-bit real type.
    R16,
    /// The 32-bit real type.
    R32,
    /// The 64-bit real type.
    R64,
    /// The 128-bit real type.
    R128,
    /// The 32-bit complex type.
    C32,
    /// The 64-bit complex type.
    C64,
    /// The 128-bit complex type.
    C128,
    /// The 256-bit complex type.
    C256,
    /// Another named type.
    Named,
    /// A generic type parameter.
    TypeParameter,
    /// The contextual `Self` type.
    ContextualSelf,
    /// A type-valued member projection.
    TypeValuedMember,
    /// A tuple with the supplied element count.
    Tuple(u64),
    /// A fixed-size array.
    Array,
    /// A dynamically sized slice.
    Slice,
    /// A lazy homogeneous generator.
    Generator,
    /// A nullable type.
    Nullable,
    /// A borrowed type.
    Borrow,
    /// A dynamically dispatched trait view.
    TraitView,
    /// An owned indirection type.
    OwnedIndirection,
    /// A callable type.
    Callable,
}

/// Locale-neutral deterministic artifact digest used by diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticArtifactDigest {
    algorithm: DiagnosticArtifactDigestAlgorithm,
    bytes: [u8; 32],
}

impl DiagnosticArtifactDigest {
    /// Creates a digest from its typed algorithm and exact bytes.
    pub const fn new(algorithm: DiagnosticArtifactDigestAlgorithm, bytes: [u8; 32]) -> Self {
        Self { algorithm, bytes }
    }

    /// Returns the digest algorithm.
    pub const fn algorithm(&self) -> DiagnosticArtifactDigestAlgorithm {
        self.algorithm
    }

    /// Returns the exact digest bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Locale-neutral deterministic artifact digest algorithm.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArtifactDigestAlgorithm {
    /// BLAKE3 with its standard 256-bit output.
    Blake3,
    /// SHA-256.
    Sha256,
}

impl DiagnosticArtifactDigestAlgorithm {
    /// Returns the stable machine key for this digest algorithm.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blake3 => "blake3",
            Self::Sha256 => "sha256",
        }
    }
}

/// Locale-neutral compiler artifact category used by diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArtifactKind {
    /// Human-readable target assembly.
    Assembly,
    /// Human-readable backend low-level IR.
    BackendIr,
    /// Backend binary IR or bitcode.
    BackendBitcode,
    /// Relocatable native object.
    RelocatableObject,
    /// Backend-owned directly executable module.
    ExecutableModule,
    /// Separately stored debug data.
    DebugCompanion,
    /// Compiled package interface.
    PackageInterface,
    /// Compiler-owned dependency metadata.
    DependencyMetadata,
    /// Final executable product.
    Executable,
    /// Final static library product.
    StaticLibrary,
    /// Final shared library product.
    SharedLibrary,
    /// Target-required linked companion.
    LinkedCompanion,
}

impl DiagnosticArtifactKind {
    /// Returns the stable machine key for this artifact category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Assembly => "assembly",
            Self::BackendIr => "backend_ir",
            Self::BackendBitcode => "backend_bitcode",
            Self::RelocatableObject => "relocatable_object",
            Self::ExecutableModule => "executable_module",
            Self::DebugCompanion => "debug_companion",
            Self::PackageInterface => "package_interface",
            Self::DependencyMetadata => "dependency_metadata",
            Self::Executable => "executable",
            Self::StaticLibrary => "static_library",
            Self::SharedLibrary => "shared_library",
            Self::LinkedCompanion => "linked_companion",
        }
    }
}

/// Locale-neutral external output destination used by diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticOutputSink {
    /// Filesystem artifact path.
    Filesystem(PathBuf),
    /// Host-owned in-memory collector identity.
    Memory(String),
    /// Host-owned writable stream identity.
    Stream(String),
}

/// Locale-neutral semantic category expected from name binding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNameKind {
    /// Any ordinary declaration or local name.
    Symbol,
    /// A logical module.
    Module,
    /// A type-valued declaration or parameter.
    Type,
    /// A trait declaration.
    Trait,
    /// A runtime or compile-time value.
    Value,
    /// A declaration usable as a named pattern.
    Pattern,
    /// An explicit callable overload family.
    CallableOverload,
    /// A declaration associated with a module, type, trait, or implementation.
    Member,
    /// A compiler-known capability allowed in a trusted callable contract.
    TrustedCapability,
}

impl DiagnosticNameKind {
    /// Returns the stable machine key for this category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Symbol => "symbol",
            Self::Module => "module",
            Self::Type => "type",
            Self::Trait => "trait",
            Self::Value => "value",
            Self::Pattern => "pattern",
            Self::CallableOverload => "callable_overload",
            Self::Member => "member",
            Self::TrustedCapability => "trusted_capability",
        }
    }
}

/// Locale-neutral effective visibility used by declaration diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticVisibility {
    /// Public declaration visibility.
    Public,
    /// Internal declaration visibility.
    Internal,
}

impl DiagnosticVisibility {
    /// Returns the stable machine key for this visibility.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
        }
    }
}

/// Locale-neutral module trust state used by declaration diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticModuleTrust {
    /// Module permits trusted declarations.
    Trusted,
    /// Module does not permit trusted declarations.
    Ordinary,
}

impl DiagnosticModuleTrust {
    /// Returns the stable machine key for this trust state.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trusted => "trusted",
            Self::Ordinary => "ordinary",
        }
    }
}

/// Stable subset of host I/O error categories used in diagnostics.
///
/// This avoids storing localized or platform-specific error text in compiler
/// diagnostics while still preserving the relevant failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticIoErrorKind {
    /// A path or resource already exists.
    AlreadyExists,
    /// A path names a directory where a file was expected.
    IsDirectory,
    /// Input data was not accepted by the host API.
    InvalidData,
    /// An input argument was not accepted by the host API.
    InvalidInput,
    /// An operation was interrupted.
    Interrupted,
    /// A path component was not a directory.
    NotDirectory,
    /// A path or resource was not found.
    NotFound,
    /// Any I/O error category not yet modeled explicitly.
    Other,
    /// The host denied access to the resource.
    PermissionDenied,
    /// The operation timed out.
    TimedOut,
    /// The input ended unexpectedly.
    UnexpectedEof,
    /// The operation would have blocked.
    WouldBlock,
}

impl DiagnosticIoErrorKind {
    /// Returns the stable machine key for this I/O error category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AlreadyExists => "already_exists",
            Self::IsDirectory => "is_directory",
            Self::InvalidData => "invalid_data",
            Self::InvalidInput => "invalid_input",
            Self::Interrupted => "interrupted",
            Self::NotDirectory => "not_directory",
            Self::NotFound => "not_found",
            Self::Other => "other",
            Self::PermissionDenied => "permission_denied",
            Self::TimedOut => "timed_out",
            Self::UnexpectedEof => "unexpected_eof",
            Self::WouldBlock => "would_block",
        }
    }
}

impl From<std::io::ErrorKind> for DiagnosticIoErrorKind {
    fn from(kind: std::io::ErrorKind) -> Self {
        match kind {
            std::io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            std::io::ErrorKind::IsADirectory => Self::IsDirectory,
            std::io::ErrorKind::InvalidData => Self::InvalidData,
            std::io::ErrorKind::InvalidInput => Self::InvalidInput,
            std::io::ErrorKind::Interrupted => Self::Interrupted,
            std::io::ErrorKind::NotADirectory => Self::NotDirectory,
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::TimedOut => Self::TimedOut,
            std::io::ErrorKind::UnexpectedEof => Self::UnexpectedEof,
            std::io::ErrorKind::WouldBlock => Self::WouldBlock,
            _ => Self::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use bray_syntax::SyntaxKind;

    use super::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticArtifactDigest,
        DiagnosticArtifactDigestAlgorithm, DiagnosticIoErrorKind, DiagnosticNameKind,
    };

    #[test]
    fn diagnostic_args_pair_stable_names_with_typed_values() {
        let arg = DiagnosticArg::new(
            DiagnosticArgName::Character,
            DiagnosticArgValue::Character('\u{0}'),
        );

        assert_eq!(arg.name(), DiagnosticArgName::Character);
        assert_eq!(arg.name().as_str(), "character");
        assert_eq!(arg.value(), &DiagnosticArgValue::Character('\u{0}'));
    }

    #[test]
    fn syntax_args_keep_syntax_kind_values_typed() {
        let arg = DiagnosticArg::expected_syntax_kind(SyntaxKind::FuncKeyword);

        assert_eq!(arg.name(), DiagnosticArgName::ExpectedSyntaxKind);
        assert_eq!(arg.name().as_str(), "expected_syntax_kind");

        assert_eq!(
            arg.value(),
            &DiagnosticArgValue::SyntaxKind(SyntaxKind::FuncKeyword)
        );
    }

    #[test]
    fn artifact_mismatch_args_keep_expected_and_actual_facts_typed() {
        let digest =
            DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Blake3, [0_u8; 32]);

        let expected_digest = DiagnosticArg::expected_artifact_digest(digest.clone());
        let actual_length = DiagnosticArg::actual_byte_count(4);

        assert_eq!(
            expected_digest.name(),
            DiagnosticArgName::ExpectedArtifactDigest
        );

        assert_eq!(
            expected_digest.value(),
            &DiagnosticArgValue::ArtifactDigest(digest)
        );

        assert_eq!(actual_length.name(), DiagnosticArgName::ActualByteCount);
        assert_eq!(actual_length.value(), &DiagnosticArgValue::ByteCount(4));
    }

    #[test]
    fn declaration_args_keep_names_and_module_surface_values_typed() {
        let name = DiagnosticArg::declaration_name("Point");
        let named_member = DiagnosticArg::trait_member_name("get");
        let lifecycle_member = DiagnosticArg::trait_member_kind(SyntaxKind::EnterKeyword);
        let visibility = DiagnosticArg::expected_visibility(super::DiagnosticVisibility::Public);
        let trust = DiagnosticArg::actual_module_trust(super::DiagnosticModuleTrust::Ordinary);

        assert_eq!(name.name(), DiagnosticArgName::DeclarationName);
        assert_eq!(name.name().as_str(), "declaration_name");

        assert_eq!(
            name.value(),
            &DiagnosticArgValue::DeclarationName(String::from("Point"))
        );

        assert_eq!(named_member.name(), DiagnosticArgName::TraitMemberName);
        assert_eq!(named_member.name().as_str(), "trait_member_name");

        assert_eq!(
            named_member.value(),
            &DiagnosticArgValue::DeclarationName(String::from("get"))
        );

        assert_eq!(
            lifecycle_member.value(),
            &DiagnosticArgValue::SyntaxKind(SyntaxKind::EnterKeyword)
        );

        assert_eq!(
            visibility.value(),
            &DiagnosticArgValue::Visibility(super::DiagnosticVisibility::Public)
        );

        assert_eq!(
            trust.value(),
            &DiagnosticArgValue::ModuleTrust(super::DiagnosticModuleTrust::Ordinary)
        );

        assert_eq!(super::DiagnosticVisibility::Internal.as_str(), "internal");
        assert_eq!(super::DiagnosticModuleTrust::Trusted.as_str(), "trusted");
    }

    #[test]
    fn binding_args_keep_reference_names_and_expected_categories_typed() {
        let name = DiagnosticArg::referenced_name("Point");
        let kind = DiagnosticArg::expected_name_kind(DiagnosticNameKind::Type);

        assert_eq!(name.name(), DiagnosticArgName::ReferencedName);
        assert_eq!(name.name().as_str(), "referenced_name");

        assert_eq!(
            name.value(),
            &DiagnosticArgValue::ReferencedName(String::from("Point"))
        );

        assert_eq!(kind.name(), DiagnosticArgName::ExpectedNameKind);
        assert_eq!(kind.name().as_str(), "expected_name_kind");

        assert_eq!(
            kind.value(),
            &DiagnosticArgValue::NameKind(DiagnosticNameKind::Type)
        );

        assert_eq!(
            DiagnosticNameKind::CallableOverload.as_str(),
            "callable_overload"
        );
    }

    #[test]
    fn diagnostic_io_error_kinds_keep_stable_categories() {
        assert_eq!(
            DiagnosticIoErrorKind::from(ErrorKind::NotFound),
            DiagnosticIoErrorKind::NotFound
        );

        assert_eq!(
            DiagnosticIoErrorKind::from(ErrorKind::ConnectionReset),
            DiagnosticIoErrorKind::Other
        );

        assert_eq!(DiagnosticIoErrorKind::NotFound.as_str(), "not_found");
    }
}
