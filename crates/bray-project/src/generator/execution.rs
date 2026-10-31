use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bray_base::{Cancellation, StableDigestHasher};
use bray_diagnostics::DiagnosticProjectGeneratorFailure as Failure;

use super::model::{
    GeneratedSource, GeneratorCacheKey, GeneratorExecutable, GeneratorRequest, GeneratorTool,
};
use crate::ProjectPath;

/// One attempted file write returned by the sandbox. The executor must return every write.
#[derive(Debug)]
pub struct GeneratorOutput {
    /// Untrusted generator-relative file identity, validated before publication.
    pub path: String,
    /// Complete file contents. Links, directories, and special files cannot be published.
    pub bytes: Vec<u8>,
}

/// Host boundary for executing exact tool bytes with a closed file and environment inventory.
///
/// Implementations must enforce the request's permissions before executing. Materialize input
/// snapshots read-only, isolate output writes, clear inherited environment, and deny all ambient
/// filesystem access and network access unless explicitly granted. Return every attempted output write
/// and reject links and special files. If these guarantees cannot be enforced, return `SandboxUnavailable` without executing.
/// No implementation may silently fall back to an ordinary host process.
pub trait SandboxedGeneratorExecutor: Send + Sync {
    /// Executes one request without reading manifests, selecting inputs, or acquiring tools.
    fn execute(
        &self,
        request: &GeneratorRequest<'_>,
        cancellation: &dyn Cancellation,
    ) -> Result<Vec<GeneratorOutput>, Failure>;
}

/// Reusable successful generator results. Failed or cancelled graph loads do not update the cache.
#[derive(Default)]
pub struct GeneratorCache {
    pub(crate) entries: BTreeMap<GeneratorCacheKey, Arc<[GeneratedSource]>>,
}

/// Selects cache reuse or explicit reproducibility checking.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GeneratorCacheMode {
    /// Reuse successful outputs for equal inputs and execution policy.
    #[default]
    Reuse,
    /// Execute again and assert that equal inputs produce identical sources.
    Verify,
}

/// Explicit execution resources supplied by the host. None are discovered from the process environment.
pub struct GeneratorHost<'a> {
    /// Exact host artifacts indexed by the declared product and execution target.
    pub executables: &'a BTreeMap<GeneratorTool, GeneratorExecutable>,
    /// Content identity of the executor implementation and enforced sandbox policy.
    /// Change this when any observable sandbox behavior changes.
    pub policy_identity: [u8; 32],
    /// Executor that enforces the declared file, environment, and network restrictions.
    pub executor: &'a dyn SandboxedGeneratorExecutor,
    /// Successful invocation results shared across graph loads.
    pub cache: &'a mut GeneratorCache,
    /// Whether to reuse cached outputs or reproduce and compare them.
    pub cache_mode: GeneratorCacheMode,
    /// Cancellation observed before execution and before publishing any source or cache entry.
    pub cancellation: &'a dyn Cancellation,
}

pub(crate) fn cache_key(request: &GeneratorRequest<'_>) -> GeneratorCacheKey {
    let mut hasher = StableDigestHasher::new();

    "bray-generated-source".hash(&mut hasher);
    hasher.write_u32(1);
    request.generator.hash(&mut hasher);
    request.executable.content_identity().hash(&mut hasher);
    request.executable.toolchain_identity().hash(&mut hasher);
    request.target.hash(&mut hasher);
    request.inputs.hash(&mut hasher);
    request.policy_identity.hash(&mut hasher);

    GeneratorCacheKey(hasher.finalize())
}

pub(crate) fn validate_outputs(
    request: &GeneratorRequest<'_>,
    key: GeneratorCacheKey,
    outputs: Vec<GeneratorOutput>,
) -> Result<Arc<[GeneratedSource]>, Failure> {
    let mut sources = BTreeMap::new();

    for output in outputs {
        let Some(path) = ProjectPath::try_relative(output.path.as_str()) else {
            return Err(Failure::InvalidOutput { path: output.path });
        };

        if request.generator.outputs.binary_search(&path).is_err() {
            return Err(Failure::UndeclaredOutput { path: output.path });
        }

        let text = String::from_utf8(output.bytes).map_err(|_| Failure::NonUtf8Output {
            path: output.path.clone(),
        })?;

        let source = GeneratedSource {
            // Snapshots share the declaration's small Arc-backed identity and target values.
            generator: request.generator.identity.clone(),
            target: request.target.identity().clone(),
            output: path.clone(),
            text: text.into(),
            cache_key: key,
        };

        if sources.insert(path, source).is_some() {
            return Err(Failure::OutputCollision { path: output.path });
        }
    }

    for path in request.generator.outputs.iter() {
        if !sources.contains_key(path) {
            return Err(Failure::MissingOutput {
                path: path.as_str().into(),
            });
        }
    }

    Ok(sources.into_values().collect())
}
