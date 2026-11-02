# Specification conventions

These conventions apply to every chapter and its child sections, including the grammar references, compiler-known
declarations, and standard-library contracts described by this specification.

## Requirements and explanations

**Normative** material specifies Bray: its source forms, validity rules, execution behavior, declaration contracts,
implementation obligations, and permitted choices. Definitions, grammar productions, and rules stated in prose, lists,
or tables are normative unless identified as informative. A declarative sentence can state a requirement without using
the word `must`.

**Informative** material explains those rules without adding requirements or exceptions. Examples and passages
introduced as `Example`, `Examples`, or `For example` are informative. Passages labeled `Note` or `Rationale` are also
informative. A section titled `Summary` or `Availability summary` is informative and restates the detailed rules it
summarizes. Navigation and references help locate material and do not independently specify behavior.

An explanation, example, note, rationale, or summary cannot introduce an additional obligation, weaken a requirement,
or grant an implementation choice. If such material appears to disagree with a rule, resolve the discrepancy under
[implementation authority](#implementation-authority).

## Requirement wording

The subject and context of a sentence determine whether it concerns source code, execution, or an implementation.
The following wording has the same meaning throughout this reference:

| Wording | Meaning |
| --- | --- |
| `must`, `is required to`, `shall` | A requirement on the stated subject. |
| `must not`, `is not permitted`, `shall not` | A prohibition in the stated context. |
| `may`, `is permitted to` | Permission within the limits stated by the rule. |
| `can` | An available source form, capability, possible outcome, or implementation option, as determined by its subject and context. |
| `cannot` | A restriction or absence of a capability, as determined by its subject and context. |
| `should`, `is recommended` | Advice, not an additional validity or conformance requirement. |
| `is`, `has`, `produces`, `evaluates` | A definition or specified property or behavior, when used in a rule. |

A sentence about a possible effect does not require that effect on every execution. A permission to use a source form
does not remove its other validity conditions. Where `may` or `can` leaves it unclear whether a sentence describes
permission, possibility, or required support, clarification must name that distinction explicitly and follow the
implementation-authority rule rather than infer a new choice.

## Definitions and examples

A definition introduces a term and its meaning. The term is often bold at its introduction, but highlighting is not
required for a definition to be normative. Uses of a defined term retain that meaning within its stated context.
The [Concepts](concepts.md) chapter introduces shared terminology and links to the detailed rules.

Code fragments illustrating source forms, accepted or rejected programs, or execution outcomes are informative usage
examples. Their surrounding explanation states the relevant context and consequence. A fragment can omit imports,
support declarations, or surrounding code. Schematic ellipses and placeholder names describe omitted context, not
additional Bray syntax. Literal syntax is defined by the grammar. Fragments with omitted context do not promise to be
complete compilable programs.

An example's omission of a validity condition does not waive it. An accepted or rejected example illustrates the
governing rule, and a rejection is distinct from a runtime panic, cancellation, or returned error.

### Required declaration surfaces

A code block introduced as a required declaration surface or as declarations provided by a compiler-known scope or
standard-library module is normative for the surface and contracts it specifies. This includes bodyless declarations
used to describe compiler-provided operations. Those declarations are not ordinary source forms that packages can
necessarily write. Their owning rules specify their availability and interpretation.

Code-block formatting alone does not distinguish a usage example from a required declaration surface. Read the
introducing text and the owning declaration rules. Grammar productions similarly specify source structure rather than
serve as usage examples. The [syntax grammar](syntax-grammar.md#ebnf-notation) defines grammar notation, while semantic
rules constrain which parsed forms are valid.

## Validity, execution, and implementation

A source-validity rule determines which program forms are accepted in a stated context. Phrases such as `is rejected`,
`is invalid`, and `is a compile-time error` identify an invalid form. They do not by themselves specify diagnostic
wording, ordering, or a recovery strategy. Any diagnostic or checking-phase requirements are stated separately.

An execution rule describes evaluation, results, effects, control flow, and obligations such as ownership and cleanup.
A rule specifying a panic, cancellation, or typed failure describes that outcome, not an implicit compile-time rejection.
Checking steps are distinct from runtime operations, as described by
[expression evaluation order](expressions/expression-evaluation-order.md).

Conformance requirements can apply to the compiler, compiled interfaces, linker, runtime, product host, or standard
library. Requirements to preserve identity, contracts, proof information, or observable behavior remain requirements
even when they do not describe a runtime step. A reference to compiler work is not automatically implementation advice.

Implementation strategies explain how a requirement can be met. Where a rule permits different lowering strategies,
representations, or optimizations, the choice must preserve the behavior and contracts required by that rule. A
description of an available strategy does not require it unless the owning rule explicitly does so.

## Target and implementation choices

Variation is permitted only where the owning rule states a choice and its limits. Target properties supply the selected
target's values within the rules in [Target profiles and properties](targets-layout-abi-and-raw-memory/target-profiles-and-properties.md).
Rules for [layout](targets-layout-abi-and-raw-memory/layout-contracts.md) and
[callable ABI](targets-layout-abi-and-raw-memory/callable-abi.md) state the relevant representation freedoms and explicit
contracts.

Read `compiler-defined`, `target-defined`, and `implementation-defined` with their owning rule. A reference to
compiler-provided behavior does not itself grant freedom to change its specified meaning. For example, the
[compiler-provided declaration rules](compiler-known-and-standard-library/compiler-provided-declarations.md) preserve
observable contracts while allowing different lowering strategies. These conventions do not create additional
categories of unspecified or undefined behavior.

## Implementation authority

Whenever this specification is ambiguous, incomplete, or contains conflicting formulations that leave a rule unclear,
the actual current implementation in code is authoritative. Clarify the specification to match the relevant compiler,
runtime, and standard-library code paths. Inferred design intent, analogy with another language, and a preferred
reading of unclear prose do not override that implementation.

Trace the implementing code at the revision being described. Tests and observed executions can corroborate that
reading, but one observed result or an older test expectation does not replace source inspection. When documenting a
substantive ambiguity resolution, record the implementation revision, responsible files or symbols, and corroborating
evidence in the change's review description.

This policy resolves unclear documentation. It does not authorize changing implemented behavior during an editorial
clarification or discarding an unambiguous requirement. If behavior is unimplemented, or conflicting code paths provide
no determinate answer, identify the exact unresolved question for a tracked design decision rather than inventing
semantics. An implementation is not defective merely because it differs from one interpretation of ambiguous prose.

## Navigation

- [Language index](index.md)
- [Concepts](concepts.md)
