use std::collections::HashMap;

use bray_platform::MonotonicInstant;
use bray_runtime_model::ProtectedFrameStateId;

use crate::{ExecutionLane, TaskId, TaskWakeCause};

use super::contract::SchedulerError;

#[derive(Clone, Copy, Debug)]
pub(super) struct QueuedTask {
    pub(super) task: TaskId,
    pub(super) state: ProtectedFrameStateId,
    pub(super) cause: TaskWakeCause,
    pub(super) queued_at: Option<MonotonicInstant>,
}

/// Each admitted task owns one ready slot, independent of its selected lane.
#[derive(Debug, Default)]
pub(super) struct ReadySlots {
    slots: Vec<ReadySlot>,
    free: Option<ReadySlotId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ReadySlotId(usize);

#[derive(Debug)]
enum ReadySlot {
    Vacant(Option<ReadySlotId>),
    Reserved,
    Queued(ReadyEntry),
}

#[derive(Debug)]
struct ReadyEntry {
    task: QueuedTask,
    lane: ExecutionLane,
    previous: Option<ReadySlotId>,
    next: Option<ReadySlotId>,
}

/// A lane retains only links and the number of admitted tasks that can reach it.
#[derive(Debug, Default)]
pub(super) struct ReadyQueue {
    reservations: usize,
    first: Option<ReadySlotId>,
    last: Option<ReadySlotId>,
}

impl ReadySlots {
    pub(super) fn reserve_capacity(&mut self, additional: usize) -> Result<(), SchedulerError> {
        crate::allocation::reserve_vec_entries(&mut self.slots, additional)?;

        Ok(())
    }

    pub(super) fn reserve(&mut self) -> Result<ReadySlotId, SchedulerError> {
        if let Some(slot) = self.free {
            let ReadySlot::Vacant(next) = self.slots[slot.0] else {
                unreachable!("free ready slot must be vacant");
            };

            self.free = next;
            self.slots[slot.0] = ReadySlot::Reserved;

            return Ok(slot);
        }

        crate::allocation::reserve_vec_entries(&mut self.slots, 1)?;

        let slot = ReadySlotId(self.slots.len());
        self.slots.push(ReadySlot::Reserved);

        Ok(slot)
    }

    pub(super) fn release(
        &mut self,
        slot: ReadySlotId,
        queues: &mut HashMap<ExecutionLane, ReadyQueue>,
    ) {
        if let ReadySlot::Queued(entry) = &self.slots[slot.0] {
            let Some(queue) = queues.get_mut(&entry.lane) else {
                unreachable!("queued slot must retain its lane");
            };

            self.remove(slot, queue);
        }

        assert!(
            matches!(self.slots[slot.0], ReadySlot::Reserved),
            "ready slot must be released exactly once"
        );

        self.slots[slot.0] = ReadySlot::Vacant(self.free);
        self.free = Some(slot);
    }

    pub(super) fn push(
        &mut self,
        slot: ReadySlotId,
        queue: &mut ReadyQueue,
        lane: ExecutionLane,
        task: QueuedTask,
    ) -> Result<(), SchedulerError> {
        if queue.reservations == 0 || !matches!(self.slots[slot.0], ReadySlot::Reserved) {
            return Err(SchedulerError::ReadyQueueCapacityReached);
        }

        if let Some(last) = queue.last {
            self.queued_mut(last).next = Some(slot);
        } else {
            queue.first = Some(slot);
        }

        self.slots[slot.0] = ReadySlot::Queued(ReadyEntry {
            task,
            lane,
            previous: queue.last,
            next: None,
        });

        queue.last = Some(slot);

        Ok(())
    }

    pub(super) fn pop(&mut self, queue: &mut ReadyQueue) -> Option<QueuedTask> {
        let first = queue.first?;

        Some(self.remove(first, queue))
    }

    pub(super) fn queued(&self, slot: ReadySlotId) -> Option<&QueuedTask> {
        match &self.slots[slot.0] {
            ReadySlot::Queued(entry) => Some(&entry.task),
            ReadySlot::Reserved | ReadySlot::Vacant(_) => None,
        }
    }

    fn remove(&mut self, slot: ReadySlotId, queue: &mut ReadyQueue) -> QueuedTask {
        let ReadySlot::Queued(entry) =
            std::mem::replace(&mut self.slots[slot.0], ReadySlot::Reserved)
        else {
            unreachable!("linked ready slot must be queued");
        };

        if let Some(previous) = entry.previous {
            self.queued_mut(previous).next = entry.next;
        } else {
            queue.first = entry.next;
        }

        if let Some(next) = entry.next {
            self.queued_mut(next).previous = entry.previous;
        } else {
            queue.last = entry.previous;
        }

        entry.task
    }

    fn queued_mut(&mut self, slot: ReadySlotId) -> &mut ReadyEntry {
        let ReadySlot::Queued(entry) = &mut self.slots[slot.0] else {
            unreachable!("linked ready slot must be queued");
        };

        entry
    }
}

impl ReadyQueue {
    pub(super) fn reserve(&mut self) -> Result<(), SchedulerError> {
        self.reservations = self
            .reservations
            .checked_add(1)
            .ok_or(SchedulerError::ReadyQueueCapacityReached)?;

        Ok(())
    }

    pub(super) fn release(&mut self) -> bool {
        self.reservations -= 1;

        self.is_unreserved()
    }

    pub(super) fn is_unreserved(&self) -> bool {
        self.reservations == 0
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.first.is_none()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bray_runtime_model::ProtectedFrameStateId;

    use super::{QueuedTask, ReadyQueue, ReadySlots};
    use crate::test_support::{TestFrame, with_allocation_failure};
    use crate::{
        ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, SchedulerError, TaskControlBlock,
        TaskWakeCause,
    };

    #[test]
    fn one_admitted_slot_follows_each_task_across_lanes_without_allocation() {
        let tasks: Vec<_> = (0..8)
            .map(|value| TaskControlBlock::start(TestFrame::completing(value)).unwrap())
            .collect();

        let mut ready = ReadySlots::default();
        let slots: Vec<_> = tasks.iter().map(|_| ready.reserve().unwrap()).collect();

        let lanes = [ExecutionWorkload::Cooperative, ExecutionWorkload::Blocking]
            .map(|workload| ExecutionLane::new(ExecutionLanePlacement::Migratable, workload));

        let mut queues: HashMap<_, _> = lanes
            .into_iter()
            .map(|lane| {
                let mut queue = ReadyQueue::default();

                for _ in &tasks {
                    queue.reserve().unwrap();
                }

                (lane, queue)
            })
            .collect();

        with_allocation_failure(|| {
            for lane in lanes.into_iter().cycle().take(6) {
                let queue = queues.get_mut(&lane).unwrap();

                for (task, slot) in tasks.iter().zip(&slots) {
                    ready
                        .push(
                            *slot,
                            queue,
                            lane,
                            QueuedTask {
                                task: task.id(),
                                state: ProtectedFrameStateId::new(0),
                                cause: TaskWakeCause::Explicit,
                                queued_at: None,
                            },
                        )
                        .unwrap();
                }

                for task in &tasks {
                    assert_eq!(ready.pop(queue).unwrap().task, task.id());
                }

                assert!(ready.pop(queue).is_none());
            }

            for slot in &slots {
                ready.release(*slot, &mut queues);
            }

            for expected in slots.iter().rev() {
                assert_eq!(ready.reserve().unwrap(), *expected);
            }
        });

        assert_eq!(ready.slots.len(), tasks.len());
    }

    #[test]
    fn failed_admission_and_queued_removal_preserve_other_tasks_and_reusable_capacity() {
        let tasks: Vec<_> = (0..4)
            .map(|value| TaskControlBlock::start(TestFrame::completing(value)).unwrap())
            .collect();

        let lane = ExecutionLane::new(
            ExecutionLanePlacement::Migratable,
            ExecutionWorkload::Cooperative,
        );

        let mut queues = HashMap::from([(lane, ReadyQueue::default())]);
        let mut ready = ReadySlots::default();

        assert!(matches!(
            with_allocation_failure(|| ready.reserve()),
            Err(SchedulerError::AdmissionAllocation(_))
        ));

        assert!(ready.slots.is_empty());

        let slots: Vec<_> = tasks.iter().map(|_| ready.reserve().unwrap()).collect();
        let queue = queues.get_mut(&lane).unwrap();

        for (task, slot) in tasks.iter().zip(&slots) {
            queue.reserve().unwrap();

            ready
                .push(
                    *slot,
                    queue,
                    lane,
                    QueuedTask {
                        task: task.id(),
                        state: ProtectedFrameStateId::new(0),
                        cause: TaskWakeCause::Explicit,
                        queued_at: None,
                    },
                )
                .unwrap();
        }

        with_allocation_failure(|| {
            for index in [1, 3, 0] {
                ready.release(slots[index], &mut queues);
                assert!(!queues.get_mut(&lane).unwrap().release());
            }

            let queue = queues.get_mut(&lane).unwrap();
            assert_eq!(ready.pop(queue).unwrap().task, tasks[2].id());
            assert!(ready.pop(queue).is_none());
            assert!(queue.release());

            ready.release(slots[2], &mut queues);

            for _ in &tasks {
                ready.reserve().unwrap();
            }
        });

        assert_eq!(ready.slots.len(), tasks.len());
    }
}
