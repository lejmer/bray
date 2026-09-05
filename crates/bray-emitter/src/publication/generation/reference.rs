use std::path::Path;

use bray_base::decode_lowercase_hex;
use serde::{Deserialize, Serialize};

use super::locator::GenerationLocator;
use super::reader::PublishedGenerationReadError;
use crate::ProductGenerationIdentity;

pub(super) const REFERENCE_REVISION: u32 = 1;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GenerationReference {
    pub(super) revision: u32,
    pub(super) current: GenerationReferenceEntry,
    pub(super) previous: Option<GenerationReferenceEntry>,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GenerationReferenceEntry {
    pub(super) locator: String,
    pub(super) manifest_digest: String,
}

impl GenerationReferenceEntry {
    pub(super) fn new(locator: GenerationLocator, identity: ProductGenerationIdentity) -> Self {
        Self {
            locator: locator.to_hex(),
            manifest_digest: identity.to_hex(),
        }
    }

    pub(super) fn locator(&self) -> Result<GenerationLocator, PublishedGenerationReadError> {
        GenerationLocator::try_from_hex(&self.locator)
            .ok_or(PublishedGenerationReadError::InvalidGenerationReference)
    }

    pub(super) fn identity(
        &self,
    ) -> Result<ProductGenerationIdentity, PublishedGenerationReadError> {
        decode_lowercase_hex::<32>(&self.manifest_digest)
            .map(ProductGenerationIdentity::new)
            .ok_or(PublishedGenerationReadError::InvalidGenerationReference)
    }
}

impl GenerationReference {
    pub(super) fn publish(current: GenerationReferenceEntry, prior: Option<Self>) -> Self {
        let previous = prior.and_then(|prior| {
            if current == prior.current {
                prior.previous
            } else {
                Some(prior.current)
            }
        });

        Self {
            revision: REFERENCE_REVISION,
            current,
            previous,
        }
    }

    pub(super) fn read(path: &Path) -> Result<Option<Self>, PublishedGenerationReadError> {
        let bytes = match crate::storage::read_owned_file(path) {
            Ok(bytes) => bytes,
            Err(error)
                if matches!(
                    error.kind(),
                    crate::StorageErrorKind::Io {
                        cause: std::io::ErrorKind::NotFound,
                        ..
                    }
                ) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(PublishedGenerationReadError::Storage(Box::new(error))),
        };

        let revision = decode_revision(&bytes).map_err(|error| {
            PublishedGenerationReadError::Storage(Box::new(crate::StorageError::json(path, error)))
        })?;

        if revision != REFERENCE_REVISION {
            return Err(PublishedGenerationReadError::UnsupportedReferenceRevision(
                revision,
            ));
        }

        let reference: Self = serde_json::from_slice(&bytes).map_err(|error| {
            PublishedGenerationReadError::Storage(Box::new(crate::StorageError::json(path, error)))
        })?;

        for entry in std::iter::once(&reference.current).chain(reference.previous.iter()) {
            entry.locator()?;
            entry.identity()?;
        }

        if reference
            .previous
            .as_ref()
            .is_some_and(|previous| previous == &reference.current)
        {
            return Err(PublishedGenerationReadError::InvalidGenerationReference);
        }

        Ok(Some(reference))
    }
}

#[derive(Deserialize)]
struct RevisionProbe {
    revision: u32,
}

pub(super) fn decode_revision(bytes: &[u8]) -> Result<u32, serde_json::Error> {
    serde_json::from_slice::<RevisionProbe>(bytes).map(|probe| probe.revision)
}
