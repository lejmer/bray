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
        }
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

    /// Consumes the request into its parts.
    pub fn into_parts(self) -> (PackageIdentity, CompilationOptions, Vec<SourceInput>) {
        (self.package_identity, self.options, self.sources)
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::PackageIdentity;

    use crate::worker::WorkerBudget;

    use super::{CompilationOptions, CompilationRequest};

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
            CompilationRequest::with_options(package_identity.clone(), vec![source], options);

        assert_eq!(request.package_identity(), &package_identity);
        assert_eq!(request.options(), options);
        assert_eq!(request.sources().len(), 1);
    }
}
