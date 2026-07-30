use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_base::shared_slice;

use super::{CodegenInstance, CodegenInstanceKey};

/// Immutable closed concrete-instance graph reachable from requested roots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodegenReachability {
    roots: Arc<[CodegenInstanceKey]>,
    instances: Arc<[CodegenInstance]>,
}

impl CodegenReachability {
    /// Returns requested roots in canonical order.
    pub fn roots(&self) -> &[CodegenInstanceKey] {
        &self.roots
    }

    /// Returns every reachable instance in canonical identity order.
    pub fn instances(&self) -> &[CodegenInstance] {
        &self.instances
    }

    /// Resolves one reachable concrete instance.
    pub fn instance(&self, key: &CodegenInstanceKey) -> Option<&CodegenInstance> {
        self.instances
            .binary_search_by(|instance| instance.key().cmp(key))
            .ok()
            .and_then(|index| self.instances.get(index))
    }
}

/// Demand-driven builder for a closed concrete-instance reachability graph.
///
/// Callers may resolve each returned frontier concurrently. Completion order does not affect the
/// published graph.
pub struct CodegenReachabilityBuilder {
    roots: BTreeSet<CodegenInstanceKey>,
    pending: BTreeSet<CodegenInstanceKey>,
    demanded: BTreeSet<CodegenInstanceKey>,
    instances: BTreeMap<CodegenInstanceKey, CodegenInstance>,
    external: BTreeSet<CodegenInstanceKey>,
}

impl CodegenReachabilityBuilder {
    /// Creates a graph builder from one or more requested roots.
    pub fn try_new(
        roots: impl IntoIterator<Item = CodegenInstanceKey>,
    ) -> Result<Self, CodegenReachabilityBuildError> {
        let roots: BTreeSet<_> = roots.into_iter().collect();

        if roots.is_empty() {
            return Err(CodegenReachabilityBuildError::EmptyRoots);
        }

        Ok(Self {
            pending: roots.clone(),
            roots,
            demanded: BTreeSet::new(),
            instances: BTreeMap::new(),
            external: BTreeSet::new(),
        })
    }

    /// Claims every newly discovered unresolved instance in canonical order.
    pub fn take_frontier(&mut self) -> Arc<[CodegenInstanceKey]> {
        let frontier: Vec<_> = std::mem::take(&mut self.pending).into_iter().collect();

        self.demanded.extend(frontier.iter().cloned());

        frontier.into()
    }

    /// Publishes one demanded instance and discovers its unresolved dependencies.
    pub fn push_instance(
        &mut self,
        instance: CodegenInstance,
    ) -> Result<(), CodegenReachabilityBuildError> {
        let key = instance.key().clone();

        if self.instances.contains_key(&key) {
            return Err(CodegenReachabilityBuildError::DuplicateInstance);
        }

        if !self.demanded.remove(&key) {
            return Err(CodegenReachabilityBuildError::InstanceWasNotDemanded);
        }

        for dependency in instance.dependencies() {
            let dependency = dependency.instance();

            if dependency != &key
                && !self.instances.contains_key(dependency)
                && !self.demanded.contains(dependency)
            {
                self.pending.insert(dependency.clone());
            }
        }

        self.instances.insert(key, instance);

        Ok(())
    }

    /// Marks one demanded bodyless definition as an external leaf.
    pub fn push_external(
        &mut self,
        key: CodegenInstanceKey,
    ) -> Result<(), CodegenReachabilityBuildError> {
        if self.instances.contains_key(&key) || !self.external.insert(key.clone()) {
            return Err(CodegenReachabilityBuildError::DuplicateInstance);
        }

        if !self.demanded.remove(&key) {
            self.external.remove(&key);

            return Err(CodegenReachabilityBuildError::InstanceWasNotDemanded);
        }

        Ok(())
    }

    /// Completes the graph after every demanded instance has been supplied.
    pub fn finish(self) -> Result<CodegenReachability, CodegenReachabilityBuildError> {
        if !self.pending.is_empty() || !self.demanded.is_empty() {
            return Err(CodegenReachabilityBuildError::Incomplete);
        }

        Ok(CodegenReachability {
            roots: shared_slice(self.roots),
            instances: shared_slice(self.instances.into_values()),
        })
    }
}

/// A contract violation that prevents publication of a closed reachability graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodegenReachabilityBuildError {
    /// No product or inspection roots were requested.
    EmptyRoots,
    /// The same concrete instance was supplied more than once.
    DuplicateInstance,
    /// A completion was supplied without first claiming its frontier entry.
    InstanceWasNotDemanded,
    /// At least one discovered instance still requires resolution.
    Incomplete,
}

#[cfg(test)]
mod tests {
    use bray_testing::{test_mir_unit, test_mir_unit_with_declaration};

    use super::{CodegenReachabilityBuildError, CodegenReachabilityBuilder};
    use crate::{CodegenInstance, CodegenInstanceDependency, CodegenInstanceKey};

    #[test]
    fn recursive_dependencies_close_without_repeated_demand() {
        let first_mir = test_mir_unit(4);
        let second_mir = test_mir_unit_with_declaration(8, 1);
        let first_key = CodegenInstanceKey::non_generic(&first_mir);
        let second_key = CodegenInstanceKey::non_generic(&second_mir);

        let Ok(first) =
            CodegenInstance::try_new(
                first_key.clone(),
                first_mir,
                [CodegenInstanceDependency::definition(second_key.clone())],
            )
        else {
            panic!("first test instance must validate");
        };

        let Ok(second) =
            CodegenInstance::try_new(
                second_key.clone(),
                second_mir,
                [CodegenInstanceDependency::definition(first_key.clone())],
            )
        else {
            panic!("second test instance must validate");
        };

        let Ok(mut builder) = CodegenReachabilityBuilder::try_new([first_key.clone()]) else {
            panic!("test root must validate");
        };

        assert_eq!(
            builder.take_frontier().as_ref(),
            std::slice::from_ref(&first_key)
        );

        assert_eq!(builder.push_instance(first), Ok(()));

        assert_eq!(
            builder.take_frontier().as_ref(),
            std::slice::from_ref(&second_key)
        );

        assert_eq!(builder.push_instance(second), Ok(()));
        assert!(builder.take_frontier().is_empty());

        let Ok(graph) = builder.finish() else {
            panic!("closed recursive graph must publish");
        };

        assert_eq!(graph.roots(), &[first_key]);
        assert_eq!(graph.instances().len(), 2);
        assert!(graph.instance(&second_key).is_some());
    }

    #[test]
    fn self_recursion_does_not_reopen_a_resolved_instance() {
        let mir = test_mir_unit(4);
        let key = CodegenInstanceKey::non_generic(&mir);

        let Ok(instance) = CodegenInstance::try_new(
            key.clone(),
            mir,
            [CodegenInstanceDependency::definition(key.clone())],
        ) else {
            panic!("recursive test instance must validate");
        };

        let Ok(mut builder) = CodegenReachabilityBuilder::try_new([key]) else {
            panic!("test root must validate");
        };

        let _ = builder.take_frontier();

        assert_eq!(builder.push_instance(instance), Ok(()));
        assert!(builder.take_frontier().is_empty());
        assert!(builder.finish().is_ok());
    }

    #[test]
    fn shared_dependencies_are_demanded_once() {
        let first_mir = test_mir_unit(4);
        let second_mir = test_mir_unit_with_declaration(8, 1);
        let shared = CodegenInstance::non_generic(test_mir_unit_with_declaration(12, 2));
        let shared_key = shared.key().clone();

        let Ok(first) = CodegenInstance::try_new(
            CodegenInstanceKey::non_generic(&first_mir),
            first_mir,
            [CodegenInstanceDependency::definition(shared_key.clone())],
        ) else {
            panic!("first test instance must validate");
        };

        let Ok(second) = CodegenInstance::try_new(
            CodegenInstanceKey::non_generic(&second_mir),
            second_mir,
            [CodegenInstanceDependency::definition(shared_key.clone())],
        ) else {
            panic!("second test instance must validate");
        };

        let Ok(mut builder) = CodegenReachabilityBuilder::try_new([
            first.key().clone(),
            second.key().clone(),
        ]) else {
            panic!("test roots must validate");
        };

        let _ = builder.take_frontier();

        assert_eq!(builder.push_instance(second), Ok(()));
        assert_eq!(builder.push_instance(first), Ok(()));
        assert_eq!(builder.take_frontier().as_ref(), &[shared_key]);
        assert_eq!(builder.push_instance(shared), Ok(()));
        assert!(builder.finish().is_ok());
    }

    #[test]
    fn completion_order_does_not_change_the_published_graph() {
        let first = CodegenInstance::non_generic(test_mir_unit(4));
        let second = CodegenInstance::non_generic(test_mir_unit_with_declaration(8, 1));
        let roots = [first.key().clone(), second.key().clone()];

        let forward = graph(roots.clone(), [first.clone(), second.clone()]);
        let reversed = graph(roots, [second, first]);

        assert_eq!(forward, reversed);
    }

    #[test]
    fn incomplete_or_unsolicited_results_are_rejected() {
        let instance = CodegenInstance::non_generic(test_mir_unit(4));

        let Ok(mut builder) = CodegenReachabilityBuilder::try_new([instance.key().clone()]) else {
            panic!("test root must validate");
        };

        assert_eq!(
            builder.push_instance(instance.clone()),
            Err(CodegenReachabilityBuildError::InstanceWasNotDemanded)
        );

        assert_eq!(
            builder.finish(),
            Err(CodegenReachabilityBuildError::Incomplete)
        );
    }

    fn graph(
        roots: impl IntoIterator<Item = CodegenInstanceKey>,
        instances: impl IntoIterator<Item = CodegenInstance>,
    ) -> super::CodegenReachability {
        let Ok(mut builder) = CodegenReachabilityBuilder::try_new(roots) else {
            panic!("test roots must validate");
        };

        let _ = builder.take_frontier();

        for instance in instances {
            if let Err(error) = builder.push_instance(instance) {
                panic!("test instance must publish: {error:?}");
            }
        }

        match builder.finish() {
            Ok(graph) => graph,
            Err(error) => panic!("test graph must close: {error:?}"),
        }
    }
}
