use std::collections::{BTreeMap, BTreeSet};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::EmissionPlan;

pub(super) const INDEX_REVISION: u32 = 1;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StorageIndex {
    pub(super) revision: u32,
    pub(super) entries: BTreeMap<String, StorageRecord>,
    pub(super) cursor: Option<String>,
    pub(super) next_operation: u64,
    pub(super) content: BTreeMap<String, BTreeSet<String>>,
    pub(super) public_paths: BTreeMap<String, PublicPathClaim>,
}

impl Default for StorageIndex {
    fn default() -> Self {
        Self {
            revision: INDEX_REVISION,
            entries: BTreeMap::new(),
            cursor: None,
            next_operation: 0,
            content: BTreeMap::new(),
            public_paths: BTreeMap::new(),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicPathClaim {
    pub(super) owner: String,
    pub(super) path: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EntryKind {
    Product,
    Cache,
    Operation,
}

impl EntryKind {
    pub(super) const fn directory(self) -> &'static str {
        match self {
            Self::Product => "products",
            Self::Cache => "cache",
            Self::Operation => "operations",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StorageProduct {
    pub(crate) package: String,
    pub(crate) name: String,
}

impl StorageProduct {
    pub(crate) fn new(product: &bray_symbols::ProductIdentity) -> Self {
        Self {
            package: product.package().as_str().to_owned(),
            name: product.name().to_owned(),
        }
    }

    pub(crate) fn identity(&self) -> Option<bray_symbols::ProductIdentity> {
        bray_symbols::ProductIdentity::try_new(
            bray_symbols::PackageIdentity::try_new(self.package.as_str())?,
            self.name.as_str(),
        )
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StorageContext {
    pub(crate) target: Option<String>,
    pub(crate) profile: Option<String>,
    pub(crate) toolchain: Option<String>,
}

impl StorageContext {
    pub(crate) fn validate(
        &self,
        path: &std::path::Path,
        product: Option<&StorageProduct>,
    ) -> Result<(), super::error::StorageError> {
        if product.is_some_and(|product| product.identity().is_none())
            || self
                .target
                .as_deref()
                .is_some_and(|target| bray_target::TargetIdentity::try_new(target).is_none())
        {
            return Err(super::error::StorageError::new(
                path,
                super::error::StorageErrorKind::Metadata {
                    cause: bray_diagnostics::DiagnosticDocumentParseKind::Schema,
                    line: None,
                    column: None,
                },
            ));
        }

        Ok(())
    }

    pub(crate) fn for_plan(plan: &EmissionPlan) -> Self {
        Self {
            target: Some(plan.request().target().as_str().to_owned()),
            profile: plan.request().storage_profile().map(str::to_owned),
            toolchain: plan
                .backend()
                .map(|backend| backend.toolchain_revision().to_owned()),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StorageOwner {
    pub(super) product: Option<StorageProduct>,
    pub(super) context: StorageContext,
}

impl StorageOwner {
    pub(super) fn product(plan: &EmissionPlan) -> Self {
        Self {
            product: Some(StorageProduct::new(plan.request().product())),
            context: StorageContext::for_plan(plan),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StorageRecord {
    pub(super) kind: EntryKind,
    pub(super) owner: StorageOwner,
    pub(super) last_used: SystemTime,
    pub(super) bytes: u64,
    pub(super) state: EntryState,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EntryState {
    Live,
    Retiring,
    RemovingFiles,
}
