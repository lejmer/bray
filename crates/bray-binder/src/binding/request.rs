use bray_bound_tree::{BoundNodeOrigin, BoundSourceAnchor};
use bray_declarations::SyntaxAnchor;
use bray_syntax::SourceSyntaxNode;

use super::{BindingError, BindingResult};
use crate::{BinderFactContext, request::BinderRequestContext};

impl<C> BinderRequestContext<'_, C>
where
    C: BinderFactContext + ?Sized,
{
    pub(super) fn check_cancellation(&self) -> BindingResult<()> {
        if self.facts().is_cancelled() {
            return Err(BindingError::Cancelled);
        }

        Ok(())
    }

    pub(super) fn pattern_origin(&self, syntax: &impl SourceSyntaxNode) -> BoundNodeOrigin {
        self.source_origin(syntax)
    }

    pub(super) fn source_origin(&self, syntax: &impl SourceSyntaxNode) -> BoundNodeOrigin {
        let source = BoundSourceAnchor::new(
            SyntaxAnchor::from_node(syntax),
            self.unit().key().source().source_version(),
        );

        BoundNodeOrigin::source(source)
    }
}
