use bray_diagnostics::DiagnosticBag;

use crate::profile::ProfileSession;

/// Stable source-and-stage position of one local diagnostic collection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct DiagnosticPublicationOrder(usize);

impl DiagnosticPublicationOrder {
    pub(crate) const fn new(ordinal: usize) -> Self {
        Self(ordinal)
    }
}

/// One stage-local diagnostic collection tagged for deterministic publication.
pub(crate) struct OrderedDiagnosticCollection {
    order: DiagnosticPublicationOrder,
    diagnostics: DiagnosticBag,
}

impl OrderedDiagnosticCollection {
    pub(crate) fn new(order: DiagnosticPublicationOrder, diagnostics: &DiagnosticBag) -> Self {
        Self {
            order,
            // Bags retain immutable collection nodes without copying diagnostic records.
            diagnostics: diagnostics.clone(),
        }
    }
}

/// Publishes local collections once in their stable source-and-stage order.
pub(crate) fn publish_diagnostics(
    profile: Option<&ProfileSession>,
    mut collections: Vec<OrderedDiagnosticCollection>,
) -> DiagnosticBag {
    collections.sort_unstable_by_key(|collection| collection.order);

    crate::profile::merge_diagnostics(
        profile,
        collections.iter().map(|collection| &collection.diagnostics),
    )
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind,
    };

    use super::{DiagnosticPublicationOrder, OrderedDiagnosticCollection, publish_diagnostics};

    #[test]
    fn publication_order_is_independent_of_worker_completion_order() {
        let first = diagnostic_bag(0);
        let second = diagnostic_bag(1);

        let diagnostics = publish_diagnostics(
            None,
            vec![
                OrderedDiagnosticCollection::new(DiagnosticPublicationOrder::new(1), &second),
                OrderedDiagnosticCollection::new(DiagnosticPublicationOrder::new(0), &first),
            ],
        );

        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.id())
                .collect::<Vec<_>>(),
            [DiagnosticId::new(0), DiagnosticId::new(1)]
        );
    }

    fn diagnostic_bag(id: u32) -> DiagnosticBag {
        DiagnosticBag::single(
            Diagnostic::new(
                DiagnosticId::new(id),
                DiagnosticKind::DeclarationDuplicateName,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::actual_count(u64::from(id))),
        )
    }
}
