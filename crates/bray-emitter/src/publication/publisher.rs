use bray_base::Cancellation;
use bray_diagnostics::DiagnosticBag;

use crate::OutputSinkResolver;

/// Final host validation performed after artifact preparation and before publication.
pub trait PublicationValidator {
    /// Returns exact diagnostics when the prepared product must not be published.
    fn validate(&self) -> Result<(), DiagnosticBag>;
}

impl<F> PublicationValidator for F
where
    F: Fn() -> Result<(), DiagnosticBag>,
{
    fn validate(&self) -> Result<(), DiagnosticBag> {
        self()
    }
}

/// Publishes validated artifact contributions to the immutable plan's external sinks.
#[derive(Clone, Copy)]
pub struct ArtifactPublisher<'host> {
    pub(super) cancellation: &'host dyn Cancellation,
    pub(super) resolver: Option<&'host dyn OutputSinkResolver>,
    validation: Option<&'host dyn PublicationValidator>,
}

impl<'host> ArtifactPublisher<'host> {
    /// Creates a publisher for filesystem-only plans.
    pub const fn new(cancellation: &'host dyn Cancellation) -> Self {
        Self {
            cancellation,
            resolver: None,
            validation: None,
        }
    }

    /// Creates a publisher that can resolve memory collectors and writable streams.
    pub const fn with_sink_resolver(
        cancellation: &'host dyn Cancellation,
        resolver: &'host dyn OutputSinkResolver,
    ) -> Self {
        Self {
            cancellation,
            resolver: Some(resolver),
            validation: None,
        }
    }

    /// Requires one final host validation after artifact preparation and before publication.
    pub const fn with_publication_validation(
        mut self,
        validation: &'host dyn PublicationValidator,
    ) -> Self {
        self.validation = Some(validation);

        self
    }

    pub(super) fn validate_publication(&self) -> Result<(), DiagnosticBag> {
        match self.validation {
            Some(validation) => validation.validate(),
            None => Ok(()),
        }
    }
}
