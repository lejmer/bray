# Axiom 6: Resource lifetime is deterministic by default

Owned resources have deterministic lifetime. When a resource is owned by a value, the point at which that resource is released follows from the value's ownership story.

Resource release is part of destruction. When an owned value is destroyed, the resources it owns are released according to the value's destruction contract.

Resource safety and memory safety use the same ownership model. Memory, handles, locks, buffers, regions, foreign resources, device resources, and other owned resources are
governed by ownership, movement, borrowing, aliasing, initialization, and destruction rules.

A resource is released exactly once unless it enters an explicit ownership construct whose contract changes its release behavior.

Moving a resource-owning value transfers release responsibility to the new owner. The old owner no longer releases the moved resource.

Partially initialized resource-owning values release only the resources that were successfully initialized.

Resource lifetime is visible at API boundaries. Functions, methods, traits, and generic signatures declare whether resources are observed, mutably borrowed, consumed,
produced, or returned.

A resource may be deliberately detached from ordinary deterministic destruction only through an explicit ownership construct whose contract defines the resulting lifetime
and release responsibility.
