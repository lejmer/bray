use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;

use crate::{
    AnyBoundNodeId, BoundBlockId, BoundDependencySubject, BoundExpressionId, BoundUnitId,
    BoundUnitKind,
};

/// A subject's final required operation on every continuation from that operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LastUse {
    subject: BoundDependencySubject,
    operation: AnyBoundNodeId,
}

impl LastUse {
    /// Creates one last-use decision.
    pub const fn new(subject: BoundDependencySubject, operation: AnyBoundNodeId) -> Self {
        Self { subject, operation }
    }

    /// Returns the subject whose lifetime can end after the operation.
    pub const fn subject(self) -> BoundDependencySubject {
        self.subject
    }

    /// Returns the operation that performs the final required use.
    pub const fn operation(self) -> AnyBoundNodeId {
        self.operation
    }
}

/// A subject required by control flow after one lexical scope has exited.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LiveAcrossScope {
    scope: BoundBlockId,
    subject: BoundDependencySubject,
}

impl LiveAcrossScope {
    /// Creates one scope-boundary liveness decision.
    pub const fn new(scope: BoundBlockId, subject: BoundDependencySubject) -> Self {
        Self { scope, subject }
    }

    /// Returns the lexical scope being exited.
    pub const fn scope(self) -> BoundBlockId {
        self.scope
    }

    /// Returns the subject required after the scope exit.
    pub const fn subject(self) -> BoundDependencySubject {
        self.subject
    }
}

/// A subject retained while one direct await can suspend the current run.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LiveAcrossSuspension {
    await_expression: BoundExpressionId,
    subject: BoundDependencySubject,
}

impl LiveAcrossSuspension {
    /// Creates one suspension-boundary liveness decision.
    pub const fn new(await_expression: BoundExpressionId, subject: BoundDependencySubject) -> Self {
        Self {
            await_expression,
            subject,
        }
    }

    /// Returns the direct-await expression that can suspend.
    pub const fn await_expression(self) -> BoundExpressionId {
        self.await_expression
    }

    /// Returns the subject retained across suspension.
    pub const fn subject(self) -> BoundDependencySubject {
        self.subject
    }
}

/// A contract violation while constructing durable liveness analysis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LivenessBuildError {
    /// A decision references a subject or operation owned by another unit.
    ForeignUnit,
    /// Implementation witnesses are not flow-sensitive liveness subjects.
    UnsupportedSubject,
}

/// Durable lifetime decisions for one exact bound semantic unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Liveness {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    last_uses: Arc<[LastUse]>,
    live_across_scopes: Arc<[LiveAcrossScope]>,
    live_across_suspensions: Arc<[LiveAcrossSuspension]>,
    is_recovered: bool,
}

impl Liveness {
    /// Validates and creates durable liveness decisions.
    pub fn try_new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        last_uses: impl IntoIterator<Item = LastUse>,
        live_across_scopes: impl IntoIterator<Item = LiveAcrossScope>,
        live_across_suspensions: impl IntoIterator<Item = LiveAcrossSuspension>,
        is_recovered: bool,
    ) -> Result<Self, LivenessBuildError> {
        let last_uses = sorted_unique_shared_slice(last_uses);
        let live_across_scopes = sorted_unique_shared_slice(live_across_scopes);
        let live_across_suspensions = sorted_unique_shared_slice(live_across_suspensions);

        if last_uses
            .iter()
            .any(|entry| entry.operation().unit() != unit || !entry.subject().is_valid_for(unit))
            || live_across_scopes
                .iter()
                .any(|entry| entry.scope().unit() != unit || !entry.subject().is_valid_for(unit))
            || live_across_suspensions.iter().any(|entry| {
                entry.await_expression().unit() != unit || !entry.subject().is_valid_for(unit)
            })
        {
            return Err(LivenessBuildError::ForeignUnit);
        }

        if last_uses
            .iter()
            .map(|entry| entry.subject())
            .chain(live_across_scopes.iter().map(|entry| entry.subject()))
            .chain(live_across_suspensions.iter().map(|entry| entry.subject()))
            .any(|subject| matches!(subject, BoundDependencySubject::ImplementationWitness(_)))
        {
            return Err(LivenessBuildError::UnsupportedSubject);
        }

        Ok(Self {
            unit,
            kind,
            last_uses,
            live_across_scopes,
            live_across_suspensions,
            is_recovered,
        })
    }

    /// Returns the exact bound unit described by these liveness.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns normalized last-use decisions.
    pub fn last_uses(&self) -> &[LastUse] {
        &self.last_uses
    }

    /// Returns normalized subjects required beyond lexical scope exits.
    pub fn live_across_scopes(&self) -> &[LiveAcrossScope] {
        &self.live_across_scopes
    }

    /// Returns normalized subjects retained across direct-await suspension points.
    pub fn live_across_suspensions(&self) -> &[LiveAcrossSuspension] {
        &self.live_across_suspensions
    }

    /// Returns whether recovery prevented complete lifetime decisions.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    /// Returns whether the operation is a last use of the subject.
    pub fn is_last_use(&self, operation: AnyBoundNodeId, subject: BoundDependencySubject) -> bool {
        self.last_uses
            .binary_search(&LastUse::new(subject, operation))
            .is_ok()
    }

    /// Returns whether the subject is required after the lexical scope exits.
    pub fn is_live_across_scope(
        &self,
        scope: BoundBlockId,
        subject: BoundDependencySubject,
    ) -> bool {
        self.live_across_scopes
            .binary_search(&LiveAcrossScope::new(scope, subject))
            .is_ok()
    }

    /// Returns whether the subject is retained across one direct await.
    pub fn is_live_across_suspension(
        &self,
        await_expression: BoundExpressionId,
        subject: BoundDependencySubject,
    ) -> bool {
        self.live_across_suspensions
            .binary_search(&LiveAcrossSuspension::new(await_expression, subject))
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::{LastUse, LiveAcrossScope, LiveAcrossSuspension, Liveness, LivenessBuildError};
    use crate::test_support::semantic_values;
    use crate::{
        BorrowCapabilityId, BoundBlockId, BoundDependencySubject, BoundExpressionId, BoundUnitId,
        BoundUnitKind, LifecycleObligationId, ScopedCapabilityId, StorageAccessId,
        StorageIdentityId,
    };

    #[test]
    fn liveness_normalizes_last_uses_and_scope_boundaries() {
        let unit = BoundUnitId::new(3);
        let expression = BoundExpressionId::from_slot(unit, 1);
        let scope = BoundBlockId::from_slot(unit, 2);

        let subject = BoundDependencySubject::Storage(StorageIdentityId::from_slot(unit, 0));

        let last_use = LastUse::new(subject, expression.into());
        let live_across_scope = LiveAcrossScope::new(scope, subject);
        let live_across_suspension = LiveAcrossSuspension::new(expression, subject);

        let Ok(liveness) = Liveness::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [last_use, last_use],
            [live_across_scope, live_across_scope],
            [live_across_suspension, live_across_suspension],
            false,
        ) else {
            panic!("unit-local liveness decisions must be valid");
        };

        assert_eq!(liveness.last_uses(), &[last_use]);
        assert_eq!(liveness.live_across_scopes(), &[live_across_scope]);

        assert_eq!(
            liveness.live_across_suspensions(),
            &[live_across_suspension]
        );

        assert!(liveness.is_last_use(expression.into(), subject));
        assert!(liveness.is_live_across_scope(scope, subject));
        assert!(liveness.is_live_across_suspension(expression, subject));
        assert!(!liveness.is_recovered());
    }

    #[test]
    fn liveness_rejects_foreign_and_non_liveness_subjects() {
        let unit = BoundUnitId::new(4);
        let expression = BoundExpressionId::from_slot(unit, 0);

        let foreign =
            BoundDependencySubject::Storage(StorageIdentityId::from_slot(BoundUnitId::new(5), 0));

        assert_eq!(
            Liveness::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [LastUse::new(foreign, expression.into())],
                [],
                [],
                false,
            ),
            Err(LivenessBuildError::ForeignUnit)
        );

        let witness = BoundDependencySubject::ImplementationWitness(
            bray_symbols::testing::implementation_instance(&semantic_values(), 17),
        );

        assert_eq!(
            Liveness::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [LastUse::new(witness, expression.into())],
                [],
                [],
                false,
            ),
            Err(LivenessBuildError::UnsupportedSubject)
        );
    }

    #[test]
    fn liveness_accepts_every_flow_sensitive_subject_category() {
        let unit = BoundUnitId::new(6);
        let expression = BoundExpressionId::from_slot(unit, 0);

        let subjects = [
            BoundDependencySubject::Storage(StorageIdentityId::from_slot(unit, 0)),
            BoundDependencySubject::StorageAccess(StorageAccessId::from_slot(unit, 0)),
            BoundDependencySubject::BorrowCapability(BorrowCapabilityId::from_slot(unit, 0)),
            BoundDependencySubject::ScopedCapability(ScopedCapabilityId::from_slot(unit, 0)),
            BoundDependencySubject::LifecycleObligation(LifecycleObligationId::from_slot(unit, 0)),
        ];

        let last_uses = subjects
            .into_iter()
            .map(|subject| LastUse::new(subject, expression.into()));

        let Ok(liveness) =
            Liveness::try_new(unit, BoundUnitKind::CallableBody, last_uses, [], [], false)
        else {
            panic!("every flow-sensitive subject must support liveness decisions");
        };

        assert_eq!(liveness.last_uses().len(), subjects.len());
    }

    #[test]
    fn liveness_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<Liveness>();
    }
}
