use std::sync::Arc;

use bray_bound_tree::{
    BoundCallStorage, BoundDependencyRequirementKind, BoundDependencySubject,
    CallReportOutput, ReportDependencies,
    StorageReferenceStep, StorageTarget,
};
use bray_symbols::{
    CapturedValueOutcome, DependencyGuard, DependencyRequirement, DependencyRequirementKind, DependencySubject,
    DependencySubjectRoot,
};

use super::ExecutionFlowDomain;
use bray_bound_tree::StorageDeferredProducer;
use crate::execution_guarantees::{ExecutionDeferredInvocation, FrameReportOwner};
use crate::{CheckerRequestContext, ExecutionCallEvidence, ExecutionCondition, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn frame_storage_outcomes(
        &self,
        source: &StorageDeferredProducer,
        candidate: &ExecutionDeferredInvocation,
    ) -> Option<(&BoundCallStorage, &[CapturedValueOutcome])> {
        if let Some(source) = source.local() {
            let contract = self.dependencies.calls().get(&source)?;

            return Some((contract.storage(), contract.captured_values()));
        }

        let body = self.dependencies.frame_callees().get(candidate.callee.as_deref()?)?;

        Some((&body.storage, body.summary.captured_values()))
    }

    pub(super) fn bind_carried_report_support(&self, candidate: &mut ExecutionDeferredInvocation) {
        let Some(callee) = candidate.callee.as_deref() else { return; };

        let Some(body) = self.dependencies.frame_callees().get(callee) else { return; };

        let entry = Arc::make_mut(&mut candidate.entry);
        let summary = &body.summary;
        let has_receiver = callee.receiver.is_some();

        entry.failure_rows = self.select_frame_failures(entry, summary.failure_outcomes());

        let bind = |requirements: &[DependencyRequirement]| {
            self.bind_frame_support(entry, &body.storage, has_receiver, requirements)
        };

        candidate.completed = Some(Arc::new(bind(summary.requirements()).requirements));

        for (index, row) in summary.captured_values().iter().enumerate() {
            if let Some(report) = &row.report {
                candidate.reports.entry(FrameReportOwner::Boundary(CallReportOutput::DeferredCompleted))
                    .or_default().insert(Some(index), Arc::new(bind(report)));
            }

            for payload in row.report_payloads.iter() {
                candidate.reports.entry(FrameReportOwner::Payload(payload.destination.clone()))
                    .or_default().insert(Some(index), Arc::new(bind(&payload.requirements)));
            }
        }

        for (index, row) in summary.failure_outcomes().into_iter().flatten().enumerate() {
            if let Some(report) = &row.report {
                candidate.reports.entry(FrameReportOwner::Boundary(CallReportOutput::DeferredAbnormal(row.kind)))
                    .or_default().insert(Some(index), Arc::new(bind(report)));
            }
        }
    }

    pub(in crate::analysis) fn visit_frame_execution_requirements(
        &self,
        source: &StorageDeferredProducer,
        candidate: &ExecutionDeferredInvocation,
        proof: &crate::execution_guarantees::ExecutionProof<'_>,
        frame: BoundDependencySubject,
        mut visit: impl FnMut(&BoundCallStorage, &DependencySubject, Option<BoundDependencySubject>, BoundDependencyRequirementKind),
    ) {
        let prepared = if let Some(site) = source.local() {
            self.dependencies.calls().get(&site).and_then(|call| Some((call.storage(), call.deferred_template()?)))
        } else {
            candidate.callee.as_deref().and_then(|callee| {
                let body = self.dependencies.frame_callees().get(callee)?;

                Some((&body.storage, body.execution.as_deref()?))
            })
        };

        let Some((storage, execution)) = prepared else { return; };

        let values = self.request.semantic_values();
        let captured_proof = self.proofs.ordinary_proof(&candidate.entry.assumptions);

        let guard_possible = |subject: &DependencySubject, reference: bool, condition: &ExecutionCondition, required| {
            proof.can_assume(condition, required)
                && (reference || storage.subject(subject).is_some_and(|(_, path)| !path.steps().is_empty())
                    || captured_proof.can_assume(condition, required))
        };

        let selected = |subject: &DependencySubject| {
            let ordinal = subject.subject_root().input_ordinal(candidate.has_receiver())?;
            let argument = ExecutionPlace::argument(ordinal).value_in(&candidate.entry.arguments)?;

            frame_support_value(storage, subject, &argument)
        };

        let mut pending = execution.requirements().iter().collect::<Vec<_>>();

        while let Some(requirement) = pending.pop() {
            match requirement {
                DependencyRequirement::Direct { subject, kind } => {
                    let root = subject.subject_root();

                    let bound = BoundDependencySubject::from_static_root(values, root).or_else(|| {
                        let value = selected(subject)?;

                        self.frame_requirement_subject(&value, *kind).or_else(|| {
                            (!value.is_reference() && matches!(kind, DependencyRequirementKind::StorageAlive
                                | DependencyRequirementKind::StorageInitialized | DependencyRequirementKind::ValueDependencies))
                                .then_some(frame)
                        })
                    });

                    visit(storage, subject, bound, (*kind).into());
                }
                DependencyRequirement::Guarded(guarded) => {
                    let excluded = match guarded.guard() {
                        DependencyGuard::NullablePresent(subject) => selected(subject).is_some_and(|value| {
                            storage.subject(subject).and_then(|(ty, _)| {
                                crate::execution_guarantees::pattern_value(values, ty, bray_bound_tree::PatternPredicate::NullableAbsent)
                            }).is_some_and(|(operator, absent)| {
                                let reference = value.is_reference();
                                let condition = ExecutionCondition::pattern(value, operator, absent);

                                !guard_possible(subject, reference, &condition, false)
                            })
                        }),
                        DependencyGuard::ActiveUnionVariant { subject, variant } => selected(subject)
                            .is_some_and(|value| {
                                let reference = value.is_reference();
                                let condition = value.matches_variant(*variant);

                                !guard_possible(subject, reference, &condition, true)
                            }),
                    };

                    if !excluded { pending.extend(guarded.requirements()); }
                }
                _ => {}
            }
        }
    }

    fn frame_requirement_subject(&self, value: &ExecutionCondition, kind: DependencyRequirementKind) -> Option<BoundDependencySubject> {
        let storage = value.inline_captured_storage()?;
        let loan = storage.binding()?.borrow_capability().map(BoundDependencySubject::BorrowCapability);

        if matches!(kind, DependencyRequirementKind::BorrowCapabilityActive(_) | DependencyRequirementKind::ExclusiveMutationAuthority)
            || kind == DependencyRequirementKind::StorageAlive && loan.is_some()
        {
            return loan;
        }

        if !value.is_reference() {
            return None;
        }

        storage.known_address()?.dependency_subject(self.request.semantic_values(), self.storage)
    }

    fn bind_frame_support(
        &self,
        entry: &ExecutionCallEvidence,
        storage: &BoundCallStorage,
        has_receiver: bool,
        requirements: &[DependencyRequirement],
    ) -> ReportDependencies {
        let mut support = ReportDependencies::default();

        let mapped = crate::dependency::map_requirements::<std::convert::Infallible>(requirements, &mut |subject, kind, _| {
            let root = subject.subject_root();

            let Some(ordinal) = root.input_ordinal(has_receiver) else {
                support.subjects.extend(BoundDependencySubject::from_static_root(self.request.semantic_values(), root));

                return Ok(vec![DependencyRequirement::direct(subject.clone(), kind)]);
            };

            let argument = ExecutionPlace::argument(ordinal).value_in(&entry.arguments);

            let Some(argument) = argument else {
                return Ok(vec![evaluation_requirement(kind)]);
            };

            let selected = frame_support_value(storage, subject, &argument);
            let selected = selected.as_ref().unwrap_or(&argument);
            let captured = selected.inline_captured_storage();
            let mut portable = Vec::new();
            let mut known = false;

            if let Some(capture) = captured {
                for dependencies in capture.dependency_support() {
                    support.subjects.extend(dependencies.subjects.iter().copied());
                    support.storage.extend(dependencies.storage.iter().copied());
                    portable.extend(dependencies.requirements.iter().cloned());
                    known = true;
                }

                if let Some(regions) = capture.address_regions() {
                    for target in &regions {
                        self.retain_frame_support(&mut support, target);
                    }
                }

                known |= capture.binding().is_some_and(|binding| binding.whole_complete());
            }

            if !known || kind != DependencyRequirementKind::ValueDependencies && !selected.is_reference() {
                portable.push(evaluation_requirement(kind));
            }

            Ok(portable)
        }).expect("captured frame requirements preserve checked variable bindings");

        support.requirements.extend(mapped);

        support
    }

    fn retain_frame_support(&self, support: &mut ReportDependencies, target: &StorageTarget) {
        let Some(path) = target.inline_state_path() else { return; };

        support.subjects.insert(BoundDependencySubject::Storage(path.root));

        if let Some(access) = path.access_id(self.storage) {
            support.storage.insert(access);
        }
    }
}

fn frame_support_value(
    storage: &BoundCallStorage,
    subject: &DependencySubject,
    argument: &ExecutionCondition,
) -> Option<ExecutionCondition> {
    let Some((_, path)) = storage.subject(subject) else {
        return subject.projections().is_empty().then(|| argument.clone());
    };

    let projections = path.projections();
    let mut current = argument.clone();
    let mut offset = 0;

    for step in path.steps() {
        let index = step.index();

        for &projection in &projections[offset..index] {
            current = current.project(projection);
        }

        match step {
            StorageReferenceStep::ReadBorrow { .. } => {
                current = current.current_referent()?.clone();
                offset = index;
            }
            StorageReferenceStep::ReadAddress { .. } | StorageReferenceStep::Backing { .. } => return None,
        }
    }

    for &projection in &projections[offset..] {
        current = current.project(projection);
    }

    Some(current)
}

fn evaluation_requirement(kind: DependencyRequirementKind) -> DependencyRequirement {
    DependencyRequirement::direct(DependencySubject::root(DependencySubjectRoot::EvaluationStorage), kind)
}
