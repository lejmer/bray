use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_symbols::{
    NativeLinkKind, NativeLinkRequirement, NativeSymbolBinding, NativeSymbolContract,
    NativeSymbolIdentity, NativeSymbolPresence,
};
use bray_target::NativeTarget;

use crate::{
    NativeArtifactIndex, NativeCoRetentionGroup, NativeComdatSelection, NativeContentDigest,
    NativeDefinition, NativeDefinitionSelection, NativeIndexError, NativeRoot, NativeUnit,
    NativeUnitKind, NativeUnitSummary,
};

fn digest(bytes: &[u8]) -> NativeContentDigest {
    NativeContentDigest::new(
        bray_base::sha256_reader(bytes)
            .unwrap_or_else(|error| panic!("test content must hash: {error}")),
    )
}

fn name(value: &str) -> NonEmptySharedStr {
    NonEmptySharedStr::try_new(value).unwrap_or_else(|| panic!("test symbol must be nonempty"))
}

fn symbol(
    identity: NativeSymbolIdentity,
    version: Option<&str>,
    binding: NativeSymbolBinding,
    presence: NativeSymbolPresence,
) -> NativeSymbolContract {
    NativeSymbolContract::new(identity, version.map(name), binding, presence)
}

fn exact(
    digest: NativeContentDigest,
    definitions: Vec<NativeDefinition>,
    references: Vec<NativeSymbolContract>,
    roots: Vec<NativeRoot>,
) -> NativeUnit {
    NativeUnit::new(
        digest,
        NativeUnitKind::Object,
        NativeUnitSummary::Exact {
            definitions: definitions.into(),
            references: references.into(),
            roots: roots.into(),
        },
        [NativeLinkRequirement::new(
            name("pthread"),
            NativeLinkKind::System,
        )],
        [Arc::from("-pthread")],
    )
}

fn index() -> (NativeArtifactIndex, Vec<(NativeContentDigest, Vec<u8>)>) {
    let first = b"first object".to_vec();
    let second = b"second object".to_vec();
    let archive = b"opaque native library".to_vec();
    let first_id = digest(&first);
    let second_id = digest(&second);
    let archive_id = digest(&archive);

    let definition = symbol(
        NativeSymbolIdentity::Name(name("entry")),
        Some("VERSION_2"),
        NativeSymbolBinding::Strong,
        NativeSymbolPresence::Required,
    );

    let first_unit = exact(
        first_id,
        vec![
            NativeDefinition::new(
                definition.clone(),
                NativeDefinitionSelection::Comdat {
                    group: NativeSymbolIdentity::Name(name("group")),
                    rule: NativeComdatSelection::ExactMatch,
                    associative_with: Some(NativeSymbolIdentity::Ordinal(17)),
                },
            ),
            NativeDefinition::new(
                symbol(
                    NativeSymbolIdentity::Ordinal(17),
                    None,
                    NativeSymbolBinding::Weak,
                    NativeSymbolPresence::Required,
                ),
                NativeDefinitionSelection::Fallback,
            ),
        ],
        vec![symbol(
            NativeSymbolIdentity::Ordinal(23),
            None,
            NativeSymbolBinding::Weak,
            NativeSymbolPresence::Optional,
        )],
        vec![NativeRoot::Initialization, NativeRoot::Finalization],
    );

    let second_unit = exact(
        second_id,
        vec![NativeDefinition::new(
            definition,
            NativeDefinitionSelection::Ordinary,
        )],
        Vec::new(),
        Vec::new(),
    );

    let archive_unit = NativeUnit::new(
        archive_id,
        NativeUnitKind::OpaqueArchive,
        NativeUnitSummary::Opaque,
        [],
        [],
    );

    let index = NativeArtifactIndex::try_new(
        NativeTarget::X86_64LinuxGnu,
        digest(b"producer policy"),
        [second_unit, archive_unit, first_unit],
        [
            NativeCoRetentionGroup::try_new([first_id, second_id])
                .unwrap_or_else(|| panic!("test group must be valid")),
            NativeCoRetentionGroup::try_new([second_id, archive_id])
                .unwrap_or_else(|| panic!("test group must be valid")),
            NativeCoRetentionGroup::try_new([archive_id, first_id])
                .unwrap_or_else(|| panic!("test group must be valid")),
        ],
    )
    .unwrap_or_else(|error| panic!("test index must validate: {error:?}"));

    (
        index,
        vec![
            (first_id, first),
            (second_id, second),
            (archive_id, archive),
        ],
    )
}

#[test]
fn round_trip_duplicate_providers_cycles_roots_and_opaque_units() {
    let (index, payloads) = index();

    let bytes = index
        .encode()
        .unwrap_or_else(|error| panic!("index must encode: {error:?}"));

    let directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("test payload directory must exist: {error}"));

    for (digest, bytes) in &payloads {
        let kind = index
            .units()
            .iter()
            .find(|unit| unit.digest() == *digest)
            .unwrap_or_else(|| panic!("test unit must exist"))
            .kind();

        std::fs::write(
            directory
                .path()
                .join(kind.file_name(*digest, index.target())),
            bytes,
        )
        .unwrap_or_else(|error| panic!("test payload must be written: {error}"));
    }

    let imported = NativeArtifactIndex::import(
        &bytes,
        digest(&bytes),
        index.target(),
        index.producer(),
        directory.path(),
    )
    .unwrap_or_else(|error| panic!("test index must import: {error:?}"));

    assert_eq!(imported.index(), &index);
    assert_eq!(imported.index().units().len(), 3);
    assert_eq!(imported.index().co_retention_groups().len(), 3);
    assert!(imported.payload(payloads[0].0).is_some());
    assert!(imported.payload(digest(b"absent unit")).is_none());

    let reversed = NativeArtifactIndex::try_new(
        index.target(),
        index.producer(),
        index.units().iter().cloned().rev(),
        index.co_retention_groups().iter().cloned().rev(),
    )
    .unwrap_or_else(|error| panic!("reordered units must validate: {error:?}"));

    assert_eq!(reversed.encode().ok().as_deref(), Some(bytes.as_slice()));

    let reordered = index.units().iter().map(|unit| {
        let NativeUnitSummary::Exact {
            definitions,
            references,
            roots,
        } = unit.summary()
        else {
            return unit.clone();
        };

        NativeUnit::new(
            unit.digest(),
            unit.kind(),
            NativeUnitSummary::Exact {
                definitions: definitions.iter().rev().cloned().collect(),
                references: references.iter().rev().cloned().collect(),
                roots: roots.iter().rev().copied().collect(),
            },
            unit.native_links().iter().cloned().rev(),
            unit.link_options().iter().cloned(),
        )
    });

    let reordered = NativeArtifactIndex::try_new(
        index.target(),
        index.producer(),
        reordered,
        index.co_retention_groups().iter().cloned(),
    )
    .unwrap_or_else(|error| panic!("reordered symbols must validate: {error:?}"));

    assert_eq!(reordered.encode().ok().as_deref(), Some(bytes.as_slice()));
}

#[test]
fn import_rejects_truncation_target_policy_and_payload_mismatch() {
    let (index, _) = index();

    let bytes = index
        .encode()
        .unwrap_or_else(|error| panic!("index must encode: {error:?}"));

    let directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("test payload directory must exist: {error}"));

    assert!(matches!(
        NativeArtifactIndex::import(
            &bytes[..bytes.len() / 2],
            digest(&bytes[..bytes.len() / 2]),
            index.target(),
            index.producer(),
            directory.path(),
        ),
        Err(NativeIndexError::Malformed)
    ));

    assert!(matches!(
        NativeArtifactIndex::import(
            &bytes,
            digest(b"wrong"),
            index.target(),
            index.producer(),
            directory.path(),
        ),
        Err(NativeIndexError::IndexDigestMismatch { .. })
    ));

    assert!(matches!(
        NativeArtifactIndex::import(
            &bytes,
            digest(&bytes),
            NativeTarget::X86_64WindowsMsvc,
            index.producer(),
            directory.path(),
        ),
        Err(NativeIndexError::WrongTarget { .. })
    ));

    assert!(matches!(
        NativeArtifactIndex::import(
            &bytes,
            digest(&bytes),
            index.target(),
            digest(b"different policy"),
            directory.path(),
        ),
        Err(NativeIndexError::WrongProducer { .. })
    ));

    assert!(matches!(
        NativeArtifactIndex::import(
            &bytes,
            digest(&bytes),
            index.target(),
            index.producer(),
            directory.path(),
        ),
        Err(NativeIndexError::Read { path, error })
            if path.parent() == Some(directory.path())
                && error.kind() == std::io::ErrorKind::NotFound
    ));

    for unit in index.units() {
        std::fs::write(
            directory
                .path()
                .join(unit.kind().file_name(unit.digest(), index.target())),
            b"corrupt",
        )
        .unwrap_or_else(|error| panic!("test payload must be written: {error}"));
    }

    assert!(matches!(
        NativeArtifactIndex::import(
            &bytes,
            digest(&bytes),
            index.target(),
            index.producer(),
            directory.path(),
        ),
        Err(NativeIndexError::PayloadDigestMismatch { .. })
    ));
}

#[test]
fn import_rejects_malformed_ids_and_cross_unit_edges() {
    let (index, _) = index();

    let bytes = index
        .encode()
        .unwrap_or_else(|error| panic!("index must encode: {error:?}"));

    let mut wire: serde_json::Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("index must be JSON: {error}"));

    wire["units"][0]["digest"] = serde_json::Value::String("not-a-digest".to_owned());

    let malformed =
        serde_json::to_vec(&wire).unwrap_or_else(|error| panic!("test JSON must encode: {error}"));

    assert!(matches!(
        NativeArtifactIndex::import(
            &malformed,
            digest(&malformed),
            index.target(),
            index.producer(),
            tempfile::tempdir()
                .unwrap_or_else(|error| panic!("test directory must exist: {error}"))
                .path(),
        ),
        Err(NativeIndexError::Wire(
            crate::wire::WireError::InvalidDigest
        ))
    ));

    wire["units"][0]["digest"] = serde_json::Value::String(index.units()[0].digest().to_string());
    wire["object_format"] = serde_json::Value::String("coff".to_owned());

    let wrong_format =
        serde_json::to_vec(&wire).unwrap_or_else(|error| panic!("test JSON must encode: {error}"));

    assert!(matches!(
        NativeArtifactIndex::import(
            &wrong_format,
            digest(&wrong_format),
            index.target(),
            index.producer(),
            tempfile::tempdir()
                .unwrap_or_else(|error| panic!("test directory must exist: {error}"))
                .path(),
        ),
        Err(NativeIndexError::Wire(
            crate::wire::WireError::InvalidTarget
        ))
    ));

    let absent = digest(b"absent unit");
    let unit = exact(digest(b"unit"), Vec::new(), Vec::new(), Vec::new());

    let group = NativeCoRetentionGroup::try_new([unit.digest(), absent])
        .unwrap_or_else(|| panic!("test group must be valid"));

    assert!(matches!(
        NativeArtifactIndex::try_new(index.target(), index.producer(), [unit], [group]),
        Err(NativeIndexError::MissingCoRetentionMember(member)) if member == absent
    ));

    let opaque_object = NativeUnit::new(
        digest(b"object with unknown retention"),
        NativeUnitKind::Object,
        NativeUnitSummary::Opaque,
        [],
        [],
    );

    assert!(
        NativeArtifactIndex::try_new(index.target(), index.producer(), [opaque_object], []).is_ok()
    );

    let orphan = exact(
        digest(b"orphan association"),
        vec![NativeDefinition::new(
            symbol(
                NativeSymbolIdentity::Name(name("child")),
                None,
                NativeSymbolBinding::Strong,
                NativeSymbolPresence::Required,
            ),
            NativeDefinitionSelection::Comdat {
                group: NativeSymbolIdentity::Name(name("group")),
                rule: NativeComdatSelection::Any,
                associative_with: Some(NativeSymbolIdentity::Name(name("missing_parent"))),
            },
        )],
        Vec::new(),
        Vec::new(),
    );

    assert!(matches!(
        NativeArtifactIndex::try_new(index.target(), index.producer(), [orphan], []),
        Err(NativeIndexError::InvalidAssociation(_))
    ));
}

#[test]
fn payload_names_follow_target_object_and_archive_formats() {
    let id = digest(b"unit");

    assert_eq!(
        NativeUnitKind::Object.file_name(id, NativeTarget::X86_64WindowsMsvc),
        format!("{id}.obj")
    );

    assert_eq!(
        NativeUnitKind::Object.file_name(id, NativeTarget::X86_64LinuxGnu),
        format!("{id}.o")
    );

    assert_eq!(
        NativeUnitKind::OpaqueArchive.file_name(id, NativeTarget::X86_64WindowsMsvc),
        format!("{id}.lib")
    );

    assert_eq!(
        NativeUnitKind::OpaqueArchive.file_name(id, NativeTarget::X86_64LinuxGnu),
        format!("lib{id}.a")
    );
}

#[test]
fn construction_rejects_duplicate_units_and_unmodelled_link_options() {
    let (index, _) = index();

    let duplicate = index.units()[0].clone();

    assert!(matches!(
        NativeArtifactIndex::try_new(
            index.target(),
            index.producer(),
            [duplicate.clone(), duplicate],
            [],
        ),
        Err(NativeIndexError::DuplicateUnit(_))
    ));

    let invalid_option = NativeUnit::new(
        digest(b"invalid option"),
        NativeUnitKind::Bitcode,
        NativeUnitSummary::Opaque,
        [],
        [Arc::from("-bad\noption")],
    );

    assert!(matches!(
        NativeArtifactIndex::try_new(index.target(), index.producer(), [invalid_option], []),
        Err(NativeIndexError::InvalidLinkOption(_))
    ));

    assert!(NativeCoRetentionGroup::try_new([index.units()[0].digest()]).is_none());

    let group = index.co_retention_groups()[0].clone();

    assert!(matches!(
        NativeArtifactIndex::try_new(
            index.target(),
            index.producer(),
            index.units().iter().cloned(),
            [group.clone(), group],
        ),
        Err(NativeIndexError::DuplicateCoRetentionGroup)
    ));
}

#[test]
fn import_rejects_unsupported_schema_without_loading_payloads() {
    let (index, _) = index();

    let bytes = index
        .encode()
        .unwrap_or_else(|error| panic!("index must encode: {error:?}"));

    let mut wire: serde_json::Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("index must be JSON: {error}"));

    wire["revision"] = serde_json::Value::from(2);

    let bytes =
        serde_json::to_vec(&wire).unwrap_or_else(|error| panic!("test JSON must encode: {error}"));

    assert!(matches!(
        NativeArtifactIndex::import(
            &bytes,
            digest(&bytes),
            index.target(),
            index.producer(),
            tempfile::tempdir()
                .unwrap_or_else(|error| panic!("test directory must exist: {error}"))
                .path(),
        ),
        Err(NativeIndexError::Wire(
            crate::wire::WireError::UnsupportedSchema
        ))
    ));
}

#[test]
fn comdat_selection_rules_round_trip() {
    let directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("test payload directory must exist: {error}"));

    for rule in [
        NativeComdatSelection::Any,
        NativeComdatSelection::SameSize,
        NativeComdatSelection::ExactMatch,
        NativeComdatSelection::Largest,
        NativeComdatSelection::NoDuplicates,
    ] {
        let payload = format!("object with {rule:?}");
        let id = digest(payload.as_bytes());

        let unit = exact(
            id,
            vec![NativeDefinition::new(
                symbol(
                    NativeSymbolIdentity::Name(name("entry")),
                    None,
                    NativeSymbolBinding::Strong,
                    NativeSymbolPresence::Required,
                ),
                NativeDefinitionSelection::Comdat {
                    group: NativeSymbolIdentity::Name(name("group")),
                    rule,
                    associative_with: None,
                },
            )],
            Vec::new(),
            Vec::new(),
        );

        let index = NativeArtifactIndex::try_new(
            NativeTarget::X86_64LinuxGnu,
            digest(b"producer"),
            [unit],
            [],
        )
        .unwrap_or_else(|error| panic!("COMDAT index must validate: {error:?}"));

        std::fs::write(
            directory
                .path()
                .join(NativeUnitKind::Object.file_name(id, index.target())),
            payload,
        )
        .unwrap_or_else(|error| panic!("test payload must be written: {error}"));

        let bytes = index
            .encode()
            .unwrap_or_else(|error| panic!("index must encode: {error:?}"));

        let imported = NativeArtifactIndex::import(
            &bytes,
            digest(&bytes),
            index.target(),
            index.producer(),
            directory.path(),
        )
        .unwrap_or_else(|error| panic!("COMDAT index must import: {error:?}"));

        assert_eq!(imported.index(), &index);
    }
}
