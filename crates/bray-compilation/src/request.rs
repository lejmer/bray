use std::sync::Arc;

use bray_package_interface::{InterfaceProductIdentity, InterfaceValidationPolicy};
use bray_source::SourceInput;
use bray_symbols::PackageIdentity;

use crate::TargetAvailabilityFacts;
use crate::worker::WorkerBudget;

/// Options for one compiler operation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct CompilationOptions {
    worker_budget: WorkerBudget,
    target_availability: TargetAvailabilityFacts,
}

impl CompilationOptions {
    /// Creates compilation options.
    pub const fn new(worker_budget: WorkerBudget) -> Self {
        Self {
            worker_budget,
            target_availability: TargetAvailabilityFacts::portable(),
        }
    }

    /// Returns the compiler-owned CPU worker budget.
    pub const fn worker_budget(self) -> WorkerBudget {
        self.worker_budget
    }

    /// Returns a copy configured with target availability facts.
    pub const fn with_target_availability(
        mut self,
        target_availability: TargetAvailabilityFacts,
    ) -> Self {
        self.target_availability = target_availability;
        self
    }

    /// Returns the immutable target availability facts.
    pub const fn target_availability(self) -> TargetAvailabilityFacts {
        self.target_availability
    }
}

/// Package identity, source inputs, and options used to load a [`Compilation`](crate::Compilation).
#[derive(Debug, Eq, PartialEq)]
pub struct CompilationRequest {
    package_identity: PackageIdentity,
    options: CompilationOptions,
    sources: Vec<SourceInput>,
    dependency_interfaces: Vec<DependencyInterfaceInput>,
}

/// One package-selected compiled dependency interface supplied to a compilation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyInterfaceInput {
    package: PackageIdentity,
    product: InterfaceProductIdentity,
    bytes: Arc<[u8]>,
    validation_policy: InterfaceValidationPolicy,
}

impl DependencyInterfaceInput {
    /// Creates one immutable untrusted dependency-interface input.
    pub fn new(
        package: PackageIdentity,
        product: InterfaceProductIdentity,
        bytes: impl Into<Arc<[u8]>>,
        validation_policy: InterfaceValidationPolicy,
    ) -> Self {
        Self {
            package,
            product,
            bytes: bytes.into(),
            validation_policy,
        }
    }

    /// Returns the package identity selected by package resolution.
    pub const fn package(&self) -> &PackageIdentity {
        &self.package
    }

    /// Returns the product identity selected by package resolution.
    pub const fn product(&self) -> &InterfaceProductIdentity {
        &self.product
    }

    /// Returns the immutable untrusted artifact bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn shared_bytes(&self) -> Arc<[u8]> {
        // Validation retains immutable request bytes, so sharing avoids copying dependency files.
        Arc::clone(&self.bytes)
    }

    /// Returns the compatibility and resource policy for this artifact.
    pub const fn validation_policy(&self) -> InterfaceValidationPolicy {
        self.validation_policy
    }
}

impl CompilationRequest {
    /// Creates a compilation request for one source package with default options.
    pub fn new(package_identity: PackageIdentity, sources: Vec<SourceInput>) -> Self {
        Self::with_options(package_identity, sources, CompilationOptions::default())
    }

    /// Creates a compilation request for one source package with explicit options.
    pub fn with_options(
        package_identity: PackageIdentity,
        sources: Vec<SourceInput>,
        options: CompilationOptions,
    ) -> Self {
        Self {
            package_identity,
            options,
            sources,
            dependency_interfaces: Vec::new(),
        }
    }

    /// Returns a copy owning the selected compiled dependency interfaces.
    pub fn with_dependency_interfaces(
        mut self,
        dependency_interfaces: impl IntoIterator<Item = DependencyInterfaceInput>,
    ) -> Self {
        self.dependency_interfaces = dependency_interfaces.into_iter().collect();

        self
    }

    /// Returns the source package identity selected for this compilation.
    pub const fn package_identity(&self) -> &PackageIdentity {
        &self.package_identity
    }

    /// Returns the compilation options.
    pub const fn options(&self) -> CompilationOptions {
        self.options
    }

    /// Returns the source inputs in request order.
    pub fn sources(&self) -> &[SourceInput] {
        &self.sources
    }

    /// Returns selected dependency interfaces in package-request order.
    pub fn dependency_interfaces(&self) -> &[DependencyInterfaceInput] {
        &self.dependency_interfaces
    }

    /// Consumes the request into its parts.
    pub fn into_parts(
        self,
    ) -> (
        PackageIdentity,
        CompilationOptions,
        Vec<SourceInput>,
        Vec<DependencyInterfaceInput>,
    ) {
        (
            self.package_identity,
            self.options,
            self.sources,
            self.dependency_interfaces,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_package_interface::{
        InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceValidationPolicy,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::PackageIdentity;

    use crate::worker::WorkerBudget;

    use super::{CompilationOptions, CompilationRequest, DependencyInterfaceInput};

    #[test]
    fn compilation_requests_hold_sources_and_options() {
        let options = CompilationOptions::new(WorkerBudget::serial());

        let source = SourceInput::virtual_text(
            SourceIdentity::new(1),
            "source-1",
            SourceVersion::new(1),
            "one",
        );

        let Some(package_identity) = PackageIdentity::try_new("test.package") else {
            panic!("test package identity must be valid");
        };

        let request =
            CompilationRequest::with_options(package_identity.clone(), vec![source], options)
                .with_dependency_interfaces([dependency_interface()]);

        assert_eq!(request.package_identity(), &package_identity);
        assert_eq!(request.options(), options);
        assert_eq!(request.sources().len(), 1);
        assert_eq!(request.dependency_interfaces().len(), 1);
    }

    fn dependency_interface() -> DependencyInterfaceInput {
        let Some(package) = PackageIdentity::try_new("test.dependency") else {
            panic!("test dependency package identity must be valid");
        };

        let Some(product) = InterfaceProductIdentity::try_new("main") else {
            panic!("test product identity must be valid");
        };

        DependencyInterfaceInput::new(
            package,
            product,
            Arc::<[u8]>::from([1, 2, 3]),
            InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0)),
        )
    }
}
