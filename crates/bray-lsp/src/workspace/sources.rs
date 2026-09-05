use super::WorkspaceError;
use crate::model::{ContentChange, Position};
use bray_source::{
    LineIndex, LspPosition, SourceEdit, SourceId, SourceIdentity, SourceInput, SourceOrigin,
    SourceSnapshot, SourceVersion,
};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(super) struct WorkspaceSource {
    pub(super) identity: SourceIdentity,
    pub(super) path: Option<PathBuf>,
    pub(super) uri: String,
    pub(super) version: SourceVersion,
    pub(super) text: String,
    pub(super) is_open: bool,
}

pub(super) fn workspace_source(source: &SourceSnapshot) -> Result<WorkspaceSource, WorkspaceError> {
    let uri = source
        .origin()
        .document_uri()
        .map_err(|cause| WorkspaceError::InvalidSourceOrigin {
            origin: source.origin().clone(),
            cause,
        })?
        .ok_or(WorkspaceError::MissingDocumentUri)?;

    let path = source.origin().document_file_path().map_err(|cause| {
        WorkspaceError::InvalidSourceOrigin {
            origin: source.origin().clone(),
            cause,
        }
    })?;

    Ok(WorkspaceSource {
        identity: source.identity(),
        path,
        uri,
        version: source.version(),
        text: source.text().to_owned(),
        is_open: false,
    })
}

pub(super) fn source_index_for_uri(sources: &[WorkspaceSource], uri: &str) -> Option<usize> {
    let path = SourceOrigin::path_from_document_uri(uri).ok().flatten();

    sources.iter().position(|source| {
        source.uri == uri
            || path
                .as_ref()
                .zip(source.path.as_ref())
                .is_some_and(|(left, right)| absolute_path(left).ok() == absolute_path(right).ok())
    })
}

pub(super) fn append_open_source(
    sources: &mut Vec<WorkspaceSource>,
    uri: &str,
) -> Result<usize, WorkspaceError> {
    let next_identity = sources
        .iter()
        .map(|source| source.identity.raw())
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(WorkspaceError::SourceIdentityExhausted { current: u32::MAX })?;

    if next_identity >= 1 << 31 {
        return Err(WorkspaceError::SourceIdentityExhausted {
            current: next_identity,
        });
    }

    sources.push(WorkspaceSource {
        identity: SourceIdentity::new(next_identity),
        path: SourceOrigin::path_from_document_uri(uri).ok().flatten(),
        uri: uri.to_owned(),
        version: SourceVersion::new(0),
        text: String::new(),
        is_open: true,
    });

    sources
        .len()
        .checked_sub(1)
        .ok_or(WorkspaceError::SourceIdentityExhausted {
            current: next_identity,
        })
}

pub(super) fn apply_changes(
    source: &mut WorkspaceSource,
    version: SourceVersion,
    changes: &[ContentChange],
) -> Result<(), WorkspaceError> {
    for change in changes {
        let Some(range) = change.range else {
            source.text.clone_from(&change.text);

            continue;
        };

        let index = LineIndex::new(&source.text).map_err(WorkspaceError::SourceTooLarge)?;

        let range = index
            .text_range_for_lsp_range(
                LspPosition::new(range.start.line, range.start.character),
                LspPosition::new(range.end.line, range.end.character),
            )
            .ok_or(WorkspaceError::InvalidEdit)?;

        let snapshot = SourceSnapshot::new(
            SourceId::new(0),
            source.identity,
            SourceOrigin::lsp_document(&source.uri),
            source.version,
            source.text.as_str(),
        )
        .map_err(WorkspaceError::SourceTooLarge)?;

        let edit = SourceEdit::new(range, change.text.as_str());

        source.text = snapshot
            .apply_edit(SourceId::new(0), version, &edit)
            .map_err(WorkspaceError::SourceEdit)?
            .text()
            .to_owned();
    }

    source.version = version;

    Ok(())
}

pub(super) fn source_version(version: i64) -> Result<SourceVersion, WorkspaceError> {
    let version =
        u64::try_from(version).map_err(|_| WorkspaceError::InvalidVersion { actual: version })?;

    Ok(SourceVersion::new(version))
}

pub(super) fn absolute_path(path: &Path) -> Result<PathBuf, WorkspaceError> {
    std::path::absolute(path).map_err(|cause| WorkspaceError::InvalidDocumentPath {
        path: path.to_path_buf(),
        cause,
    })
}

impl WorkspaceSource {
    pub(super) fn input(&self) -> SourceInput {
        if self.is_open {
            return SourceInput::lsp_open_document(
                self.identity,
                self.uri.clone(),
                self.version,
                self.text.clone(),
            );
        }

        match &self.path {
            Some(path) => {
                SourceInput::file(self.identity, path.clone(), self.version, self.text.clone())
            }
            None => SourceInput::lsp_open_document(
                self.identity,
                self.uri.clone(),
                self.version,
                self.text.clone(),
            ),
        }
    }
}

pub(crate) fn offset_for_position(
    source: &SourceSnapshot,
    position: Position,
) -> Result<bray_source::TextSize, WorkspaceError> {
    LineIndex::new(source.text())
        .map_err(WorkspaceError::SourceTooLarge)?
        .offset_for_lsp_position(LspPosition::new(position.line, position.character))
        .ok_or(WorkspaceError::InvalidEdit)
}
