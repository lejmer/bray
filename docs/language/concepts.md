# Concepts

This chapter defines terms used throughout the Bray language specification. These terms describe the basics for the language semantics.

---

## Program Structure

A **source unit** is one UTF-8 text input accepted by the lexer.

A **token** is a lexical item produced from source text.

**Trivia** is whitespace or comments attached to tokens. Trivia separates tokens and preserves source text but is not itself a
semantic language construct.

A **syntax tree** is the parsed representation of a source unit.

A **module** is a named source-level unit of declaration ownership and path resolution.

A **package** is a collection of source units and package metadata compiled as one product or library surface.

A **declaration** introduces a named program entity or language-defined member. Declaration rules are defined in [Declarations](declarations.md).

A **scope** is a region where names can be introduced and resolved.

A **binding** associates a name use with the declaration, local binding, member, or other entity it denotes.

---

## Names And Paths

A **name** is an identifier spelling used to introduce or refer to a program entity.

A **path** is a qualified reference made from names and path separators.

A **member** is a declaration or component reached through a type, value, trait application, module, or implementation relationship.

A **receiver** is the value or storage access that a method call operates on.

An **implementation** binds behavior for a subject type and a trait application.

A **trait application** is a trait declaration together with any generic arguments required by that trait.

---

## Values And Storage

A **value** is a typed runtime entity.

**Storage** is a location that can hold a value.

An **access path** is a source-level route to storage or a subpart of storage.

An **owner** is the binding or storage location responsible for a value's ownership obligations.

An **owned value** is a value whose ownership obligations are held by the current owner.

A **borrow** is temporary access to storage without taking ownership of that storage.

A **shared borrow** permits read access according to the borrow's contract.

A **mutable borrow** permits mutation according to the borrow's contract and Bray aliasing rules.

A **move** transfers ownership obligations from one owner to another.

A **copy** duplicates a value according to the value's copy contract without transferring ownership from the source.

A **partial move** moves a subpart of a value while leaving the remaining parts subject to Bray's partial-move rules.

Ownership, borrowing, access-path, movement, and dependency-contract rules are defined in [Ownership and borrowing](ownership-and-borrowing.md).

An **initialized** storage location contains a valid value of its type.

An **uninitialized** storage location does not contain a valid value and cannot be read as that type.

---

## Types

A **type** describes the set of values and operations accepted for a value or storage location.

A **type expression** is source syntax that denotes a type.

A **type form** is a type constructor shape such as borrow, nullable, array, tuple, box, or trait-view form.

A **sized type** has a compile-time known finite size for direct storage.

An **unsized type** does not have a compile-time known finite size for direct storage and must appear behind a type form that
defines storage or access.

A **nullable type** is a type form that can represent absence with `none`.

A **trait view** is a type form that exposes behavior through a trait application without making traits ordinary stored types.

A **compiler-known type** is a type whose identity and semantics are defined by the language and always known to the compiler.

A **standard-library type** is supplied by a standard-library package and must be made visible through normal source structure.

---

## Evaluation

An **expression** computes a value, performs an effect, controls evaluation, or combines those behaviors according to its expression
kind.

A **statement-like expression** is an expression commonly used for its effect or control behavior even though Bray treats it as an
expression form.

A **block** is a scoped expression region containing declarations and expressions.

The **result** of an expression is the value, `unit`, `never`, panic, cancellation, or other completion behavior defined by that
expression.

**Evaluation order** is the order in which subexpressions and runtime operations occur.

A **normal completion path** is an execution path that reaches the expression's ordinary result without panic, cancellation,
return, yield, break, continue, or another non-local completion.

A **non-local completion** exits the current expression region through a control operation such as `return`, `yield`, `break`,
`continue`, panic, or cancellation.

---

## Contracts And Conditions

A **condition** is a proposition guaranteed by the language rules at a particular program point.

A **predicate** is a named or inline boolean rule valid in predicate context.

A **contract** is a caller-visible semantic obligation or guarantee attached to a declaration, type, trait, implementation, or
lifecycle operation.

A **precondition** must hold before an operation is used.

A **postcondition** must hold after an operation completes normally.

A **dependency contract** records requirements that must remain true for a value, borrow, callable value, trait view, task, or other
entity to stay valid.

A **trusted obligation** is an obligation the ordinary checker cannot prove and that must be satisfied through Bray's trust rules.

A **capability** is named authority to perform an operation that ordinary safe Bray does not allow without that authority.

Contract, predicate, flow-sensitive contract reasoning, trusted-capability, trusted-obligation, witness-value, and trust-boundary
rules are defined in [Contracts and trust](contracts-and-trust.md).

---

## Lifecycle

**Construction** creates a value and establishes its initial invariants.

**Destruction** ends a value's ownership and releases its ordinary owned resources.

**Finalization** is a required lifecycle obligation that must complete before ownership ends when a type defines such an obligation.

**Enter** begins a scoped lifecycle region and can produce scoped capability.

**Exit** ends a scoped lifecycle region and consumes the scoped capability produced by the matching enter operation.

A **lifecycle obligation** is an obligation to finalize, exit, destroy, cancel, join, release, or otherwise complete a value's
required lifecycle before the owning scope ends.

Construction, finalization, destruction, scoped-use, and lifecycle-obligation rules are defined in [Lifecycle](lifecycle.md).

---

## Concurrency

A **run** is a dynamic execution domain that ultimately completes normally, panics, or is cancelled. Ordinary calls and direct
awaits remain in the current run; tasks, native threads, and typed child processes create child runs.

A **task** is an asynchronous run managed by Bray's async rules.

A **thread** is an operating-system execution context when the target and selected standard library support thread execution.

A **process** is an isolated operating-system execution and resource domain when the target and selected standard library support
process creation.

A **run boundary** separates independently executing work and converts a crossing panic or cancellation into the outcome defined by
the owning observation contract.

A **task obligation** is the compiler-known requirement to join, cancel, transfer, or automatically resolve `Task<T>` before its
owner ends. Ordinary standard-library `Thread<T>` and `Process<T>` owners carry analogous library-defined lifecycle obligations.

The **root run** is owned by the executable or test product host. A synchronous entrypoint executes as that run; an async entrypoint
is driven as a host-owned root task without a source-visible `Task<T>`.

**Cancellation** requests that a run stop according to its cancellation contract.

---
