# Specification conventions

These conventions apply to every chapter and its child sections. They cover the grammar references, compiler-known
declarations, and standard-library contracts in this specification.

## Requirements and explanations

**Normative** material defines Bray's source forms, validity rules, execution behavior, declaration contracts,
implementation obligations, and permitted choices. Definitions, grammar productions, and rules in prose, lists, or
tables are normative unless marked informative. A declarative sentence can state a requirement without the word `must`.

**Informative** material explains rules without adding requirements or exceptions. Examples and passages introduced as
`Example`, `Examples`, or `For example` are informative. Passages labeled `Note` or `Rationale` are informative too.
A section titled `Summary` or `Availability summary` is informative and restates the detailed rules. Navigation and
references locate material without specifying behavior.

At the start of a sentence, `for example` marks the rest of that sentence as informative. Within a sentence, the phrase
marks only the example clause. Preceding clauses and subsequent sentences keep their status. A `Note:` or `Rationale:`
label at the start of a paragraph applies to that paragraph. A label on its own line applies to the following block,
such as a paragraph, list, or code block.

Longer informative passages use a blockquote or a dedicated subsection. Their scope ends with the blockquote or at the
next heading of the same or higher level. Rules outside those boundaries remain normative.

An explanation, example, note, rationale, or summary cannot add an obligation, weaken a requirement, or grant an
implementation choice. Disagreements with a rule follow the [implementation-authority policy](#implementation-authority).

## Requirement wording

The subject and context of a sentence determine whether the rule concerns source code, execution, or an implementation.
The following wording applies throughout this reference:

| Wording | Meaning |
| --- | --- |
| `must`, `is required to`, `shall` | A requirement on the stated subject. |
| `must not`, `is not permitted`, `shall not` | A prohibition in the stated context. |
| `may`, `is permitted to` | Permission within the limits stated by the rule. |
| `can` | An available source form, capability, possible outcome, or implementation option, according to the subject and context. |
| `cannot` | A restriction or absence of a capability, according to the subject and context. |
| `should`, `is recommended` | Advice, not an additional validity or conformance requirement. |
| `is`, `has`, `produces`, `evaluates` | A definition or specified property or behavior, when used in a rule. |

A possible effect need not occur on every execution. Permission to use a source form leaves its other validity
conditions in force. Ambiguous uses of `may` or `can` follow the implementation-authority policy. A clarification states
whether the rule permits a choice, describes a possibility, or requires support.

## Definitions and examples

A definition introduces a term and its meaning. The term is often bold at its introduction, but definitions are
normative with or without that emphasis. A defined term retains its meaning within the stated context.
The [Concepts](concepts.md) chapter introduces shared terminology and links to the detailed rules.

Code fragments that illustrate source forms, accepted or rejected programs, or execution outcomes are informative
examples. The surrounding explanation states the context and consequence. A fragment can omit imports, support
declarations, or surrounding code. Such fragments are incomplete programs. Schematic ellipses and placeholder names
denote omitted context. The grammar defines literal syntax.

Omitting a validity condition from an example does not waive the condition. An accepted or rejected example illustrates
the rule. Rejection is distinct from a runtime panic, cancellation, or returned error.

### Required declarations

A code block is normative when its introduction identifies required declarations or declarations provided by a
compiler-known scope or standard-library module. The block specifies the declarations and their contracts.
Bodyless declarations can describe compiler-provided operations. The rules for those declarations determine their
availability and whether packages can write the bodyless form as source.

The introducing text and declaration rules distinguish examples from required declarations, regardless of code-block
formatting. Grammar productions specify source structure. The [syntax grammar](syntax-grammar.md#ebnf-notation) defines
grammar notation, and semantic rules constrain which parsed forms are valid.

## Validity, execution, and implementation

A source-validity rule determines which program forms the compiler accepts in a stated context. Phrases such as
`is rejected`, `is invalid`, and `is a compile-time error` identify an invalid form. These phrases alone do not specify
diagnostic wording, diagnostic order, or error recovery. Separate rules state any diagnostic or checking-phase
requirements.

An execution rule describes evaluation, results, effects, control flow, and obligations such as ownership and cleanup.
A rule that specifies a panic, cancellation, or typed failure describes that outcome. The rule does not imply
compile-time rejection. Checking steps are distinct from runtime operations, as described by
[expression evaluation order](expressions/expression-evaluation-order.md).

Conformance requirements can apply to the compiler, compiled interfaces, linker, runtime, product host, or standard
library. Requirements to preserve identity, contracts, proof information, or observable behavior need not describe a
runtime step. A reference to compiler work does not make a rule implementation advice.

An implementation strategy describes how the compiler, runtime, or standard library can meet a requirement. A permitted
choice of lowering strategy, representation, or optimization must preserve the required behavior and contracts.
A strategy is required only when the rule that specifies it says so.

## Target and implementation choices

Variation is permitted only when the relevant rule states a choice and its limits. Target properties supply values for
the selected target under [Target profiles and properties](targets-layout-abi-and-raw-memory/target-profiles-and-properties.md).
Rules for [layout](targets-layout-abi-and-raw-memory/layout-contracts.md) and
[callable ABI](targets-layout-abi-and-raw-memory/callable-abi.md) state the relevant representation freedoms and explicit
contracts.

The meaning of `compiler-defined`, `target-defined`, and `implementation-defined` depends on the relevant rule.
Compiler-provided behavior retains its specified meaning. For example, the
[compiler-provided declaration rules](compiler-known-and-standard-library/compiler-provided-declarations.md) preserve
observable contracts while permitting different lowering strategies. These conventions do not create additional
categories of unspecified or undefined behavior.

## Implementation authority

When this specification leaves a rule ambiguous, incomplete, or internally conflicting, the current implementation in
code is authoritative. Clarifications describe the relevant compiler, runtime, and standard-library code paths.
Inferred design intent, analogy with another language, and a preferred interpretation of unclear prose do not override
the implementation.

The implementing code at the documented revision establishes the behavior. Tests and observed executions can
corroborate the interpretation, but a single result or an older test expectation does not replace source inspection.
Review descriptions for substantive ambiguity resolutions record the implementation revision, responsible files or
symbols, and corroborating evidence.

The policy resolves unclear documentation. Editorial clarifications preserve implemented behavior and unambiguous
requirements. If behavior is unimplemented or code paths conflict without a determinate answer, the unresolved question
requires a tracked design decision. A difference from one interpretation of ambiguous prose does not establish an
implementation defect.

## Navigation

- [Language index](index.md)
- [Concepts](concepts.md)
