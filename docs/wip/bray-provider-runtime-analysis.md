# Independently loaded provider ownership

The provider implementation follows the [provider lifetime contract](bray-native-runtime-design.md#provider-lifetime)
and the existing host terminal state machine. Its Bray implementation lives in `runtime/bootstrap/src/host/product`.
The native ABI records live in `bray-runtime-abi`. Compiler mappings connect reusable code to its final product.

## Formation and residency

A native loader keeps its image owner alive while invoking the image's provider formation operation. The binding names
the actual resident domain, required services, residency callbacks and already formed dependency handles. A resident
domain exports its host storage, synchronization, report storage and callbacks together. Its storage and callback code
must outlive every bound image. The domain initializes its native TLS keys before publishing the view. An already bound
image exports the original resident view, including its callback operations.

Formation freezes the binding and secures metadata, static cleanup backing and one stable root cell before publishing
entry. Concurrent compatible attempts return the same epoch. Incompatible bindings, invalid dependencies, capacity
failure and reused activation slots leave entry unpublished. Static admission retains its existing exactly-once rollback
protocol. An accepted binding remains owned through failed formation and terminal retirement.

Dependency handles are sorted once using heapsort. Only open, already published epochs can become dependencies of a new
epoch. Every edge therefore points to an older admission. Rebinding cannot introduce a cycle. Dependency owners feed the
existing cleanup eligibility mechanism rather than a second graph. Their immutable handles remain available after the
actual roots have been released.

Two mapped copies have distinct epochs and static storage. Archives and imported MIR reference the final consumer's
descriptor. A final executable has resident code and elides provider root acquisition. A shared image publishes an
activation slot and typed formation/root operations. The synchronous case requires no scheduler binding.

## Ownership and retirement

Reports and typed incidents carry provider roots that retain code and statics together. Copying a native reference view
does not acquire ownership. A noncopy owner acquires explicitly and releases once. Native callers retain an outside
guard through callbacks and all body-local destruction. Bray consumers detach the record's root, dispose its payload
and backing, then release the detached root from resident code.

A native header may own a provider root even when its primary is absent. Suppression releases that independent header
owner while preserving its incident chain. Activation cleanup also disposes otherwise empty headers that own a root.

Entry closure rejects new external roots. Existing owners can finish, clone their disposal authority and produce a
replacement error while retaining the original provider. Internal teardown incidents do not retain the product they
are currently cleaning. Consumer statics finish before their dependency roots release.

Retirement observes native residency outside the host lock. A stable-cell pin and revision detect acquisition, release
and reentrant control during that observation, including an acquisition that completes before observation returns.
Final retirement requires closed entry, no provider roots, no callbacks, no caller-owned shutdown obligation and no
attachments. It invalidates both the external registry identity and image activation slot before transferring the last
native owner to a caller outside the image. Stale handles cannot touch image metadata. Epochs are never reused.
Common host shutdown rejects independent bindings until this retirement protocol has completed, including failed
formation with an outstanding observer callback. Product reservation checks admission under the host lock after any
off-lock preparation, so completed host shutdown cannot be followed by publication into retired storage.

Product TLS membership remains separate inside the common native attachment. Explicitly detaching one product does not
allow a raw accessor to reenroll through another product's attachment. Explicit reentry opens a fresh membership.
Retirement makes inactive membership rows reusable so load history does not accumulate there.

Native C/Rust hosts that link reusable runtime code publish their own resident final descriptor. The explicitly bundled
Rust host supplies this descriptor through its bundled-bootstrap feature. Produced Bray products use unbundled runtime
components and their compiler-generated descriptors.

## Work and storage costs

Known roots access their stable cell directly. Retain uses a compare/exchange and a revision increment. Non-final release
pins the callback, updates the revision, decrements the root and unpins the callback. Neither path allocates, scans the
registry or takes the host lock. Final release performs the existing host progress and retirement work.

External handle validation scans the existing product registry. Formation sorts D dependencies in O(D log D), then
validates and acquires them through that registry. With P registered products this lookup work is O(D P). Sorting uses
the admitted dependency buffer and does not allocate a second representation. Retirement also visits live native
attachment memberships to reclaim inactive rows. These admission and terminal costs are separate from steady-state
root operations.

The native reference/owner is 16 bytes and its immutable callback table is 24 bytes. A report grows from 136 to 152 bytes,
an incident from 152 to 168 bytes, and a product descriptor from 64 to 80 bytes. An independent image adds an atomic
activation slot and a 16-byte operation table. Resident products do not allocate a root cell. A provider-dependent value
can deliberately retain the entire image and all its product statics until disposal.

### Linux x86-64 measurement

The native lifetime fixture loads a real shared image, produces a panic with a typed cleanup payload on an OS thread,
joins that thread, copies independent history, closes entry, and disposes the primary and incident separately. The
chain-only header acquires its own root and is suppressed with an empty report before incident disposal. The fixture repeats
eight times, alternating successful and panicking payload destruction. It also performs 1,024 root retain/release pairs
after closure with allocations denied. Native archive and generic imported-MIR producers use the same resident host.

| Measure | Observed |
| --- | --- |
| Cold formation, after resident domain initialization | 12 allocations, 6,480 requested bytes |
| Warm formation for the six-static fixture | 2 allocations, 920 requested bytes |
| Resident storage after each completed cycle | 13 allocations, 10,320 requested bytes |
| Normal disposal and root cloning after admission | 0 allocation attempts |
| Required scheduler binding for the synchronous image | None |
| Development image file | 1,189,264 bytes |
| Release image file | 1,153,296 bytes |
| Development image text / data / BSS | 1,081,991 / 2,248 / 96 bytes |
| Release image text / data / BSS | 1,048,374 / 2,088 / 2,344 bytes |

Allocation observations cover Bray's aligned heap allocations. They exclude loader/libc allocations and probe metadata.
The 13 retained allocations are resident host/report storage, not eight retained providers. Each image's native owner
reaches its final unload on every cycle. Denying allocations during a panicking destructor may reject optional message
formatting, while mandatory payload, backing and provider disposal still completes.

The resident native link maps select no members from the Rust host, callback or scheduler archives for these synchronous
provider consumers. Compiler and test coordination remain Rust. The provider fixture's emitted native archive and
imported-MIR variants retain the same final consumer and require no execution binding.

These are local implementation measurements, not a cross-language benchmark. The downstream comparative corpus remains
the owner of whole-program acceptance. In particular, retained image/statics size is application-dependent and should
not be inferred from the small fixture's requested heap bytes.
