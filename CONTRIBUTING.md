# Contributing to Bray

Contributions to Bray's compiler, standard library, tooling, tests, and documentation are welcome. Bug reports and small
documentation improvements are useful ways to get started.

## Find work and ask for help

Search the [issue tracker](https://github.com/lejmer/bray/issues) and existing pull requests before starting work. Read the discussion and linked changes to avoid
duplicating an implementation already under review. Comment on an existing issue to share your intended approach and
coordinate with other contributors.

If you are unsure where a change belongs or which tests to run, ask on the relevant issue or pull request.

## Propose a language feature or substantial change

Use a Bray Enhancement Proposal (BEP) to propose changes to language syntax or semantics, substantial changes to
official library or tooling interfaces and behavior, and changes to project governance, policies, or procedures. Read the
[BEP index](https://github.com/lejmer/bray/blob/develop/beps/README.md) and existing proposals before drafting. [BEP 1](https://github.com/lejmer/bray/blob/develop/beps/bep-0001.md) defines the scope, review process, and decision rules.

Follow the [submission guide](https://github.com/lejmer/bray/blob/develop/beps/submit-a-bep.md) and use the [BEP template](https://github.com/lejmer/bray/blob/develop/beps/template.md). You may seek early feedback in the repository's
[GitHub Discussions](https://github.com/lejmer/bray/discussions) before writing a draft. Each admitted BEP has one canonical discussion in the **BEPs** category. Submit
and revise the proposal document through pull requests, and keep design discussion and decisions in that thread. Merging
a proposal document does not itself approve the design. Follow the BEP decision process before implementing the proposed
feature.

Bug fixes that restore specified behavior, internal refactoring, performance improvements that preserve behavior, and
editorial documentation changes use ordinary pull requests. Ask a maintainer if you are unsure whether a BEP is needed.

## Use AI tools responsibly

AI tools and coding agents are welcome under the [AI contribution policy](.github/AI_POLICY.md). Review and understand all work you submit,
verify its behavior, and take responsibility for review responses and follow-up changes. Whenever AI generates or edits
code, disclose the AI model used in your pull request description.

## Report a bug

Open an [issue](https://github.com/lejmer/bray/issues) with:

- A small, self-contained example that reproduces the problem.
- The command you ran, the expected behavior, and the actual output.
- The Bray commit or version, operating system, and any relevant toolchain details.

For a compiler crash, include the diagnostic output and backtrace when available. Reduce the example where possible while
keeping the failure reproducible.

## Set up a development checkout

Install Rust and Cargo. The minimum supported Rust version is declared as `rust-version` in
[`Cargo.toml`](Cargo.toml).

Clone the repository and create a feature branch from `develop`:

```sh
git clone https://github.com/lejmer/bray.git
cd bray
git switch develop
git switch -c feature/my-change
```

Run repository commands from the checkout root. Provision the pinned LLVM toolchain before building compiler tools:

```sh
cargo llvm fetch
cargo build
```

The [LLVM guide](docs/contributing/llvm.md) covers host support, provisioning prerequisites, and external installations. The default build selects
the developer tools. Use `cargo build --workspace` to build every workspace package. See [repository tasks](docs/contributing/xtask.md) for tool
commands, incremental builds, and required runtime and standard-library artifacts, and [artifact storage](docs/contributing/build-artifacts.md) for
build-cache guidance.

## Understand the area you are changing

Read the repository's [agent and development instructions](AGENTS.md) and the relevant contributor guides:

- [Coding conventions](docs/contributing/coding-conventions.md) and [crate boundaries](docs/contributing/crates.md).
- [Bray source style](docs/contributing/bray-style-guide.md).
- [Testing](docs/contributing/testing.md) and [repository tasks](docs/contributing/xtask.md).
- [Structured compiler diagnostics](docs/contributing/diagnostics.md).
- [Compiler-known catalog](docs/contributing/compiler-known-catalog.md).
- [Standard-library development](docs/contributing/standard-library.md),
  [runtime artifacts](docs/contributing/runtime-artifacts.md), and [OS bindings](docs/contributing/os-bindings.md).
- [Performance workflow](docs/contributing/performance.md) and [compiler profiling](docs/contributing/profiling.md).

The [language specification](docs/language/index.md) defines the intended language behavior. Keep implementation changes,
examples, and documentation consistent with that contract.

## Validate your change

For Rust changes, apply formatting and structural style checks:

```sh
cargo xtask style
```

For Bray source changes, run `cargo xtask format`. The style task also includes Bray formatting. Add regression coverage
for behavior changes and run tests for the affected crates:

```sh
cargo test -p <crate>
```

For compiler work, use `cargo xtask test compiler` for the fast behavior suite and select additional tiers appropriate
to the change. Native code generation, linking, or execution changes need native coverage. The [testing guide](docs/contributing/testing.md) explains
the tiers, focused runs, and specialized suites. `cargo test --workspace` runs tests across all workspace packages.

Documentation-only changes do not require code checks. Read the rendered Markdown and verify links and examples.

## Submit a pull request

Keep each PR focused on one coherent change and target `develop`. Apply the appropriate repository labels to every PR.
Use a [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) title in the form `type: description` or `type(scope): description`. The scope in
parentheses is optional. For example, use `feat: add some feature` or `fix(parser): reject an invalid token`.

Write the title and change summary in the imperative, such as "Add feature", "Fix bug", or "Refactor module". Use brief,
simple language. Avoid forms such as "Added feature" or "Adds feature".

Structure the PR description with these sections:

- `Problem`: Briefly explain why the PR exists. Make the problem easy to read and understand.
- `Change Summary`: List the changes as bullets. Include relevant validation and any checks you could not complete.
- `Related Issues`: If any GitHub or Linear issues are mentioned, list them as links. Use only the issue ID as the link
  text, such as `#123` or `BRA-678`. Omit this section when there are no related issues.
- `LLM Contributions`: List the AI models used, as required by the [AI contribution policy](.github/AI_POLICY.md). Exact model versions are optional.
  If no AI models were used, write "None".

Link any applicable BEP and its design discussion in the change summary. For implementation work, explain how the
change satisfies the approved proposal. Include reproduction steps, diagnostic examples, or performance measurements
when they help reviewers assess the change.

A simple example PR (the issue links and model names are examples):

```markdown
## Problem

Short description of why this PR exists.

## Change Summary

- Add feature
- Fix bug
- Refactor module

## Related Issues

- [#123](https://github.com/lejmer/bray/issues/123)

## LLM Contributions

- Opus 5.5
- GPT 6.1 Sol
```

Review may involve several rounds of feedback. Be prepared to explain the implementation, respond to questions,
and update tests and documentation as the change evolves.
