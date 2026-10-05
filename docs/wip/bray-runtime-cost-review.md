# Bray runtime architecture cost review

This reviews the [agreed design](bray-native-runtime-design.md) at `4e2fc10e` for execution speed, binary size, memory use and compilation cost. It uses the language and consumer research. It contains architectural reasoning, not benchmark results or a claim of parity with Rust or C++.

I would keep the agreed ownership architecture. Several decisions remove known sources of overhead, including duplicate task records, descriptor reconstruction and scheduler mediation for direct await. Two implementation prescriptions needed correction. Erasure does not always need separately allocated backing, and dispatch does not always need a lock. The proposal now makes both depend on actual lifetime and sharing requirements.

The largest unresolved costs are mandatory cleanup backing and whole-product retention. Neither has enough implementation evidence to establish its cost. They need attention when choosing representations, even if performance tuning waits until dependent work is complete.

## Review of agreed decisions

| Decisions | Cost assessment | Design consequence |
| --- | --- | --- |
| 1 and 3, shutdown and whole-product retention | Avoid mandatory blocking shutdown. A small retained report can nevertheless keep a large product's caches, handles and other resources alive. This primarily affects retained memory and resource lifetime. | Keep the agreed uniform dependency rule. Make the retention cost visible to consumers. Optional independently owned diagnostics allow hosts to release original reports. Their conversion cost occurs only when requested. |
| 2, shared cleanup service | A stable service binding avoids rebinding transferred owners. A central lock or registry call for every owner operation could become a contention bottleneck. | Share the service lifetime without prescribing a central operation for every local admission or move. Already secured local backing can satisfy admission. Internal borrows need no extra lease when the enclosing owner covers their lifetime. |
| 4 and 11, bootstrap and optional services | Explicit startup storage supports a small synchronous host. A universal host descriptor or registration table could still retain unused scheduler, loader or diagnostic code. | Keep service references specific to reachable behavior. Check both linked code and metadata. Lazy initialization alone does not reduce binary size. Packaging dynamic-loading support with the synchronous host must not force consumers to link it. |
| 5, static cleanup order | Dependency ordering adds compiler analysis and metadata. Repeating structural sorting at runtime would add startup or teardown cost without improving the guarantee. | Emit the known within-domain order once. Keep runtime graph work for relationships that actually depend on loaded products and attachments. Structural tie keys do not imply storing full compiler identities in every live runtime node. |
| 6, secured cleanup backing | Each live owner can require storage for a cleanup error or activation that never occurs. Large error types and overlapping lifetimes can enlarge frames, reduce cache locality and increase allocation pressure. This is the largest storage risk. | Compute concrete local needs. Reuse backing only where lifetimes permit. Remove requirements for actions impossible for every value of a concrete type. Avoid separate allocation per owner and duplicate child charges. Retained incidents still prevent premature reuse. |
| 7 and 10, tasks and waits | One task record and direct await within one run remove duplication. Wake delivery still needs coordination and retained target storage. Extra allocation per waiter, universal locking or atomic retention for every internal access would add unnecessary work. | Use admitted waiter storage and coalesced wakes. Synchronize actual shared state. Thread confinement must include wake access before removing synchronization. Direct await needs no additional task record or queue round trip. |
| 8, accessibility and authority | Safe internal access requires preserved contracts. It does not inherently require a runtime registry lookup or repeated dynamic validation for every ordinary borrow. Compiler checking can itself become expensive if equivalent contract facts are repeatedly reconstructed. | Use valid ownership and witness conditions where available. Check external identities and runtime transitions where necessary. Preserve contracts through internal access without treating every field read as an untrusted foreign call. Share compiler summaries with correct invalidation. |
| 9, execution limits | Counting running work allows suspended parents and eligible children to make progress. It also permits many queued tasks, each retaining storage. | Keep storage admission separate from execution limits. The execution quota does not bound memory. Choose queue representations for locality and actual sharing. Public channel and budget ordering does not prescribe one globally locked FIFO scheduler. |

## Costs outside the numbered decisions

Immutable descriptors should replace runtime descriptor reconstruction. They can still enlarge binaries if every concrete instantiation emits duplicate adapters, debug strings and dependency structures. Keep concrete operations where layout or behavior differs. Share equivalent emitted operations where ABI, ownership and identity permit it. Avoid adding general reflection data to support disposal.

Use direct typed calls for known representations and erased operations where heterogeneous ownership requires them. Specializing everything can increase code size and compilation time. Erasing everything can add allocation, indirection and lost optimization. The design must preserve this choice without requiring either policy everywhere.

The compiler already needs lifecycle selection and dependency reasoning. Build runtime descriptions from those results. Avoid a second analysis in the runtime and repeated whole-graph analysis for each generated wrapper. Incremental reuse must invalidate summaries when their contracts, layout or dependencies change. This is a compiler design requirement, not evidence that current caching already achieves it.

Error formatting, message preservation and suppressed incident disposal must preserve the language contracts. Keep work specific to failures off successful execution paths. Mandatory reporting still needs valid backing. Optional diagnostic history must not become mandatory reporting infrastructure.

## What this review changes

The proposal now permits enclosing or coallocated backing for erased state, permits dispatch without a transition lock when actual confinement proves it safe, and separates shared service lifetime from per-operation synchronization. It also makes provider retention depend on the lifetime that needs it, without requiring an extra lease for every descriptor or internal borrow.

These changes clarify implementation freedom. They do not weaken ownership, cleanup capacity, callback lifetime or accessibility rules. This review does not reverse the whole-product retention decision or the secured cleanup backing decision. If their concrete costs prevent the destination requirements, revisit those guarantees explicitly.

## Performance gates and implementation order

The destination must meet the execution, size and compiler requirements in decision 12. An intermediate issue need not independently reach that destination. A slower prerequisite can enable a later change that removes the overhead. Record what causes the temporary cost, what depends on the prerequisite and when a meaningful comparison becomes possible.

Use existing migration consumers for measurements where useful. Gate avoidable regressions when comparable paths exist. Complete architecture and dependent functionality before tuning costs that cannot yet be meaningfully evaluated. Keep architectural reasoning about those costs throughout the work.

Useful early comparisons include a small synchronous program's fixed host cost, frame size for large cleanup errors, task and wait allocation counts, retained memory across plugin reloads, and compiler time spent producing lifecycle and runtime descriptors. Later comparisons can cover complete scheduler throughput, contention and incremental build behavior. These are candidate measurements for existing steps, not separate prototype requirements.

## Sources

- [Language findings](bray-runtime-language-notes.md) and [consumer inventory](bray-runtime-consumer-notes.md).
- [Async representation, storage and cleanup capacity](../language/async-and-concurrency/async-representation-and-storage.md).
- [Runtime selection](../language/async-and-concurrency/entrypoints-and-runtime.md).
- [Static storage and dependency cleanup](../language/declarations/static-storage-declarations.md).
- [Trusted witness values](../language/contracts-and-trust/trusted-witness-values.md) and [caller obligations](../language/contracts-and-trust/trusted-caller-obligations.md).
