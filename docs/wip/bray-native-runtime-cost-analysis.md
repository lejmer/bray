# Appendix C. Runtime cost analysis

This appendix supports the [WIP native runtime design](bray-native-runtime-design.md#appendix-c-cost-analysis). It evaluates architectural costs in execution, binary size, memory and compilation. The analysis is based on language contracts and the [implementation inventory](bray-native-runtime-consumer-inventory.md). It contains no new benchmark results or measured performance claim.

The design removes duplicate task state, runtime descriptor reconstruction and scheduler mediation for direct await. Separately allocated backing and transition locks remain choices governed by lifetime and sharing requirements. The largest unresolved storage costs are mandatory cleanup backing and whole-product retention.

## Component costs

| Component | Potential cost | Design response |
| --- | --- | --- |
| Shutdown and provider retention | Blocked shutdown preserves ownership without mandatory waiting. A small retained report can nevertheless keep a large product's caches, handles and other resources alive. This primarily affects retained memory and resource lifetime. | Keep the agreed uniform dependency rule. Make the retention cost visible to consumers. Optional independently owned diagnostics allow hosts to release original reports. Their conversion cost occurs only when requested. |
| Shared cleanup service | A stable service binding avoids rebinding transferred owners. A central lock or registry call for every owner operation could become a contention bottleneck. | Share the service lifetime without prescribing a central operation for every local admission or move. Already secured local backing can satisfy admission. Internal borrows need no extra lease when the enclosing owner covers their lifetime. |
| Bootstrap and optional services | Explicit startup storage supports a small synchronous host. A universal host descriptor or registration table could still retain unused scheduler, loader or diagnostic code. | Keep service references specific to reachable behavior. Check both linked code and metadata. Lazy initialization alone does not reduce binary size. Packaging dynamic-loading support with the synchronous host must not force consumers to link it. |
| Static cleanup order | Dependency ordering adds compiler analysis and metadata. Repeating structural sorting at runtime would add startup or teardown cost without improving the guarantee. | Emit the known within-domain order once. Keep runtime graph work for relationships that actually depend on loaded products and attachments. Structural tie keys do not imply storing full compiler identities in every live runtime node. |
| Secured cleanup backing | Each live owner can require storage for a cleanup error or activation that never occurs. Large error types and overlapping lifetimes can enlarge frames, reduce cache locality and increase allocation pressure. This is the largest storage risk. | Compute concrete local needs. Reuse backing only where lifetimes permit. Remove requirements for actions impossible for every value of a concrete type. Avoid separate allocation per owner and duplicate child charges. Retained incidents still prevent premature reuse. |
| Tasks and waits | One task record and direct await within one run remove duplication. Wake delivery still needs coordination and retained target storage. Extra allocation per waiter, universal locking or atomic retention for every internal access would add unnecessary work. | Use admitted waiter storage and coalesced wakes. Synchronize actual shared state. Thread confinement must include wake access before removing synchronization. Direct await needs no additional task record or queue round trip. |
| Accessibility and authority | Safe internal access requires preserved contracts. It does not inherently require a runtime registry lookup or repeated dynamic validation for every ordinary borrow. Compiler checking can itself become expensive if equivalent contract facts are repeatedly reconstructed. | Use valid ownership and witness conditions where available. Check external identities and runtime transitions where necessary. Preserve contracts through internal access without treating every field read as an untrusted foreign call. Share compiler summaries with correct invalidation. |
| Execution limits | Counting running work allows suspended parents and eligible children to make progress. It also permits many queued tasks, each retaining storage. | Keep storage admission separate from execution limits. The execution quota does not bound memory. Choose queue representations for locality and actual sharing. Public channel and budget ordering does not prescribe one globally locked FIFO scheduler. |
| Resident fallback cleanup | Pending products retain storage, services and any required execution lanes until dependencies resolve. This can extend infrastructure lifetime during abnormal shutdown. | Admit fallback requirements during formation and use the existing terminal host state. Do not create per-owner helper threads, polling services or a general deferred-work framework. A binding without continued execution cannot admit the fallback. |

## Representation and compiler costs

Immutable descriptors should replace runtime descriptor reconstruction. They can still enlarge binaries if every concrete instantiation emits duplicate adapters, debug strings and dependency structures. Keep concrete operations where layout or behavior differs. Share equivalent emitted operations where ABI, ownership and identity permit it. Avoid adding general reflection data to support disposal.

Use direct typed calls for known representations and erased operations where heterogeneous ownership requires them. Specializing everything can increase code size and compilation time. Erasing everything can add allocation, indirection and lost optimization. The design must preserve this choice without requiring either policy everywhere.

The compiler already needs lifecycle selection and dependency reasoning. Build runtime descriptions from those results. Avoid a second analysis in the runtime and repeated whole-graph analysis for each generated wrapper. Incremental reuse must invalidate summaries when their contracts, layout or dependencies change. This is a compiler design requirement, not evidence that current caching already achieves it.

Error formatting, message preservation and suppressed incident disposal must preserve the language contracts. Keep work specific to failures off successful execution paths. Mandatory reporting still needs valid backing. Optional diagnostic history must not become mandatory reporting infrastructure.

## Implementation choices and guarantees

Enclosing and coallocated storage remain available for erased state. Thread-confined dispatch may omit a transition lock when confinement includes wake access. Shared service lifetime does not prescribe per-operation synchronization. A valid enclosing owner can cover provider lifetime without an extra lease per descriptor or internal borrow.

Representation changes preserve ownership, cleanup capacity, callback lifetime and accessibility contracts. Whole-product retention and secured cleanup backing remain selected requirements. If their concrete costs prevent the destination requirements, the architecture and any requiring language rule must be reconsidered explicitly.

## Performance gates and implementation order

The design's [performance requirements](bray-native-runtime-design.md#performance-requirements-and-evidence) apply to the completed implementation. An intermediate issue need not reach that destination independently. A slower prerequisite may enable a later change that removes the overhead. Record the temporary cost, its enabling dependency and the stage where a meaningful comparison becomes possible.

Use existing migration consumers for measurements where useful. Gate avoidable regressions when comparable paths exist. Complete architecture and dependent functionality before tuning costs that cannot yet be meaningfully evaluated. Keep architectural reasoning about those costs throughout the work.

Useful early comparisons include a small synchronous program's fixed host cost, frame size for large cleanup errors, task and wait allocation counts, retained memory across plugin reloads, and compiler time spent producing lifecycle and runtime descriptors. Later comparisons can cover complete scheduler throughput, contention and incremental build behavior. These are candidate measurements for existing steps, not separate prototype requirements.

## References

- [Language foundations](bray-native-runtime-design.md#appendix-a-language-foundations) and [implementation inventory](bray-native-runtime-consumer-inventory.md).
- [Async representation and cleanup capacity](../language/async-and-concurrency/async-representation-and-storage.md).
- [Runtime selection](../language/async-and-concurrency/entrypoints-and-runtime.md).
- [Static storage and dependency cleanup](../language/declarations/static-storage-declarations.md).
- [Trusted witnesses](../language/contracts-and-trust/trusted-witness-values.md) and [caller obligations](../language/contracts-and-trust/trusted-caller-obligations.md).
