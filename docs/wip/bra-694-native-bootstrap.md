# Native bootstrap contracts

Issue: https://linear.app/bray-lang/issue/BRA-694.
Branch: `feature/bra-694-native-bootstrap-contracts`, based on develop c1d01eb34.
PR #710 and its seven prerequisites are merged. Native execution and teardown remain BRA-695 and BRA-696.

## Contract and baseline

Make actual Linux and Windows bootstrap products check and lower without widening authority.
Preserve disjoint owners and fields. Reject fabricated owners, replaced contents, prior epochs and real raw retirement.
Source/imported composition and negative rejection locations stay fixed. Internal invariant failures panic.

The inherited compiler baseline passed 1,718 comparison identities, 561 checker tests and 24 focused controls.
The integration tree passes mutating style and format. Actual native product closure remains unverified.

## Current investigation

Current source-body lowering reproduces `owner_entry_scope` and `dispatch_ready` on both platforms, 61 seconds total.
The former loses `reference_retained` at `origin_driver_startup`. The latter fails `view_owned` at the recovery `context`
call after `demand_lane` and `record_task_failure`. Exact first loss in the latter remains unresolved.

The actual Windows product, with current imported standard-library contracts, reports nine call failures and one ordinary
registration postcondition failure. Sixteen further diagnostics propagate those failures. Additional leaf sites are
`create_execution`, `root_scope`, `create_worker_context`, observer `create` and `unlink_registration`.
Group these by shared-marker retirement, allocation/nullable owner transport and mutable registration presence.
These are investigation categories, not established independent causes. Linux imported products remain unverified.

`origin_driver_startup` exports unbounded write/retirement effects. Its call invalidation has no retained parameter marker.
`demand_lane` exports ambient effects plus escaped domain/worker pointers. Its call preserves the Current acquisition
marker, as does `record_task_failure`. That narrows dispatch loss to subsequent framing, witness retirement or cleanup.
`reserve_start` does not publish registration presence across that call. These observations do not establish a repair yet.

Trace effect inference and export/import, argument staging, current storage framing, witness retirement, normal return,
conditional extraction, cleanup and postcondition checking. Preserve actual produced values through every consumer.
No production semantics changed. Focused checks request execution certification and lowering. Temporary checker tracing
must be removed before committing a repair. Style remains deferred to batch finalization.

An unresolved transition blocks production edits. Record its inputs, outputs, preservation and invalidation here
after tracing executable producers and consumers. Derive both positive and overpreservation controls before repair.
Prefer existing storage/effect representations. No historical authority reconstruction or consumer exceptions.
Remove superseded machinery rather than layering a second mechanism. Reconsider if a repair needs broad new APIs.

Yielded values remain pending until ordinary result completion. Failure cleanup admits the exact produced result once
before conditional snapshots. Do not move normal publication earlier. RawAllocation backing follows its pointer field.
Aggregate stored roots are replacement markers. Borrow retention never grants ownership or preserves overwritten data.

Next: finish the bounded transition trace and establish the owning defects and overpreservation controls.
Keep this single record current. Replace stale logs, preserve readable fixtures and run style at batch finalization.
