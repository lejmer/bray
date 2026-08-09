use std::collections::BTreeMap;
use std::hash::Hash;

use bray_base::StableDigestHasher;
use bray_source::SourceId;
use bray_symbols::ImportedInterfaceId;

use super::CompilationFactKey;

const COMPILER_SEMANTIC_REVISION: u32 = 1;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CompilationInputKey {
    PackageIdentity,
    PackageSourceAuthority,
    SourceSet,
    Source(SourceId),
    SourceDiagnostics,
    ProductKind,
    SelectedTarget,
    NativeLinkInputs,
    SemanticRecursionLimit,
    SemanticPairwiseLimit,
    DependencySet,
    DependencyInterface(ImportedInterfaceId),
    DependencyImplementation(ImportedInterfaceId),
    PlatformServices,
    PackageInterfaceExport,
    CodegenConfiguration,
    StandardLibrary,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct FactFingerprint([u8; 32]);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CompilationInputs(BTreeMap<CompilationInputKey, FactFingerprint>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FactDependencyRecord {
    fingerprint: FactFingerprint,
    facts: BTreeMap<CompilationFactKey, FactFingerprint>,
    inputs: BTreeMap<CompilationInputKey, FactFingerprint>,
}

impl CompilationInputs {
    pub(crate) fn insert<T>(&mut self, key: CompilationInputKey, value: &T)
    where
        T: Hash + ?Sized,
    {
        self.0.insert(key.clone(), fingerprint(&key, value));
    }

    pub(crate) fn get(&self, key: &CompilationInputKey) -> Option<FactFingerprint> {
        self.0.get(key).copied()
    }
}

impl FactDependencyRecord {
    pub(crate) fn new(
        key: &CompilationFactKey,
        facts: BTreeMap<CompilationFactKey, FactFingerprint>,
        inputs: BTreeMap<CompilationInputKey, FactFingerprint>,
    ) -> Self {
        let mut hasher = StableDigestHasher::new();

        COMPILER_SEMANTIC_REVISION.hash(&mut hasher);
        key.hash(&mut hasher);
        facts.hash(&mut hasher);
        inputs.hash(&mut hasher);

        Self {
            fingerprint: FactFingerprint(hasher.finalize()),
            facts,
            inputs,
        }
    }

    pub(crate) const fn fingerprint(&self) -> FactFingerprint {
        self.fingerprint
    }

    pub(crate) fn facts(&self) -> &BTreeMap<CompilationFactKey, FactFingerprint> {
        &self.facts
    }

    pub(crate) fn inputs(&self) -> &BTreeMap<CompilationInputKey, FactFingerprint> {
        &self.inputs
    }
}

pub(crate) fn fingerprint<T>(key: &CompilationInputKey, value: &T) -> FactFingerprint
where
    T: Hash + ?Sized,
{
    let mut hasher = StableDigestHasher::new();

    COMPILER_SEMANTIC_REVISION.hash(&mut hasher);
    key.hash(&mut hasher);
    value.hash(&mut hasher);

    FactFingerprint(hasher.finalize())
}
