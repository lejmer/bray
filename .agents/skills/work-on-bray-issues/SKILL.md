---
name: work-on-bray-issues
description: >
  Use for Bray Linear issue implementation, issue stacks, stacked or draft pull requests, pull-request review cycles,
  review comments, unattended multi-issue work, and questions, corrections, or status requests that steer an active
  stack. Do not use for read-only issue triage or planning that will not implement or review repository changes.
---

# Work on Bray issues

Complete the requested issue or ordered stack through implementation, verification, review, pull requests, and Linear handoff. The whole stack is the terminal objective. A pull-request boundary is only a checkpoint.

Optimize for completing the agreed outcome with a coherent design and proportionate verification. An issue is ready for handoff when its acceptance criteria are met, the architecture and maintenance cost are justified, required validation is complete, and actionable review findings are resolved or disagreements recorded. Optional improvements and hypothetical future work do not move that finish line.

## Run the stack

1. Read applicable `AGENTS.md` files, complete Linear issues and relations, and linked design contracts. Inspect the worktree, branches, pull requests, and existing changes. Preserve work owned by others.
2. Order the stack by dependency and keep it active. Move an issue to `In Progress` only when its implementation starts.
3. Use one isolated worktree for the stack and one branch per pull request. Add a worktree only for concurrent independent work or at the user's request. Stack branches only for real dependencies.
4. Before semantic edits, satisfy the contract gate below. Choose the smallest coherent design using the architecture guidance, including what existing machinery it replaces. Keep necessary adjacent fixes in the active pull request and apply the follow-up issue policy below.
5. Implement a coherent batch before validating it. Self-review against the recorded contracts, the architecture guidance, and applicable `AGENTS.md` rules, then perform end-of-implementation validation as described below. Commit and push the coherent change, then create or update its draft pull request. Update the task record.
6. Dispatch a fresh review subagent for the initial review only after the draft pull request exists and contains the commit under review. Retain that subagent for re-reviews. Record its agent ID and latest reviewed commit in the task record. The review happens on that pull request so the reviewer has a destination for inline comments and its summary. A pre-PR review does not satisfy this workflow. The review subagent itself publishes the review summary and every genuine actionable finding directly on the pull request. Do not ask it to return findings for the parent agent to publish. The initial review covers the contract, full diff, tests, architecture, repository rules, affected-contract checklist, and maintainability guidance below. If there are no findings, the reviewer says so in the pull-request summary. Never invent or stretch a finding to produce a comment. A cycle is complete only after the reviewer returns the GitHub review URL or ID and the URL or ID of every inline comment. Treat a chat-only review result as incomplete, including after compaction, and verify the returned artifacts on the pull request before acting on the review.
7. Triage findings against the agreed contract and concrete architectural or correctness impact. Batch valid fixes, reply with the change or a concrete disagreement, and resolve addressed threads. If a finding exposes a defect class, inspect equivalent paths in the affected mechanism and fix the class together. Validate the completed batch using the policy below. After material correctness or design changes, send the fixes and addressed findings to the same review subagent for re-review. Limit re-review to changes since its last reviewed commit and surrounding code whose behavior may be affected. Reuse earlier conclusions for unchanged, unaffected code. Cosmetic edits, documentation wording, and unchanged code do not restart the review cycle. End agent review when actionable findings are resolved or disagreements recorded. Once agent review is clean, a user correction in task chat or a human PR comment becomes the active human review, including documentation-only feedback. Fix and respond without launching another review agent unless the human reviewer asks for one.
8. Mark the pull request ready and move the issue to `In Review` only after implementation, verification, and review. Never mark it `Done`. Linear does that when the user merges the pull request.

## Close the contract before changing it

For a cross-cutting semantic repair, keep a compact current contract in the task record before editing production code:

- State the behavioral rule and what the repair must never permit. Name the authoritative producer and actual consumers found by code searches, including equivalent paths outside the failing example.
- Trace what each affected transition receives, produces, preserves and invalidates. Record unresolved paths explicitly. A list of modules or "calls covered" is insufficient.
- Name positive and negative regression oracles and the fixed comparison evidence. Derive cases from the rule, including ways the repair could preserve too much or lose necessary information.
- State the design hypothesis, machinery to remove, growth limit and evidence that would require reconsidering the design.

An unresolved contract blocks speculative production edits and migration expansion. Read-only tracing and focused experiments that resolve the specific uncertainty remain allowed. Before validation, compare the whole batch to this contract. A newly found omission reopens the affected contract and equivalent paths before another patch. Passing the original reproducer does not close it.

After compaction or restart, read the current task record and this gate before editing or launching validation. Resume from the first unresolved contract, not merely the last command. Keep one current record, replacing stale entries rather than appending a history.

## Keep scope finite

Keep work needed for the accepted outcome and coherent architecture together, even across modules. Do not preserve a poor design just to keep a diff small, or use every nearby imperfection to expand the task. Split only when the work is a genuinely independent subsystem or a multi-thousand-line addition that cannot reasonably remain reviewable in the active pull request.

The normal outcome is zero new issues. Create a follow-up only for a verified problem with material impact on a supported use case or committed requirement that needs independent work and cannot reasonably be completed in the active scope. Optional cleanup, speculative future requirements, additional coverage without a demonstrated gap, and review preferences do not qualify. Small necessary fixes belong in the active change. Minor residual limitations can be mentioned briefly in the handoff without creating a ticket. A pre-existing failure does not automatically require a new issue: report what it prevents verifying, reuse existing tracking when available, and block only if required evidence for this change is missing.

Before creating a qualifying issue, search for an active owner. Closed, completed, cancelled, and duplicate issues are historical relations only. Never reopen, extend, or assign new work to them. If no active owner exists, create one issue for the coherent outcome, with concrete impact, evidence, and acceptance criteria, not separate issues for symptoms, layers, call sites, tests, or cleanup. Set a label and record real blocking dependencies in Linear's relations. Do not turn follow-ups into an unrequested recursive stack. Resume and finish the requested work.

## Batch validation

Choose validation from the changed behavior and its risks. Before each build or test run, identify the uncertainty it will resolve and why inspection or existing evidence is insufficient. A command must either verify a coherent completed batch, investigate a concrete blocker, or satisfy a required final check. Habit, another edit, a new commit, a branch switch, or a review checkpoint alone is not a reason to run it.

- During implementation, work through related edits using inspection and reasoning. Do not compile, run `cargo check`, or run tests after each file, function, or small patch. Run an early focused check only when its result will materially decide the design or unblock further implementation, such as a targeted reproducer or an uncertain API contract. This is an exception, not a recurring edit/check loop. Do not run formatting, style, or lint commands during implementation.
- Write or extend tests when they protect a meaningful behavioral contract or demonstrated regression. Prefer existing fixtures and representative boundary cases. Do not mirror implementation details, duplicate equivalent cases across layers, or build a test framework for a small change. Documentation-only changes need no compilation, tests, linting, or style commands.
- At finalization, after the implementation and self-review are complete, run repository-required formatting and style commands, inspect their changes, then run the smallest sufficient set of behavioral checks. After Rust changes, use the mandatory mutating `cargo xtask style`. Run `cargo xtask format` when applicable to Bray changes. Reuse builds and avoid redundant `check`, `build`, and `test` commands when one provides the needed evidence. Run broader suites only when repository rules or cross-cutting risk require them, once for the completed implementation.
- After failures or review findings, diagnose and fix the related problems as a batch before rerunning the affected checks. Repeat formatting or style only at finalization of a batch that changed relevant source. Repeat tests or compilation only where subsequent changes invalidate the evidence. Formatting-only or prose-only edits do not justify restarting behavioral validation. Broaden checks only for a new change, failure, unresolved risk, or explicit repository requirement. Stop when sufficient evidence and required checks are complete.

Record validation briefly in the task record, with the command, relevant code state, result, and material coverage limits. Reuse valid results across commits, review, handoff, and compaction. Fix regressions introduced by the change. Report blocked or unavailable checks honestly instead of repeatedly retrying an unchanged environment or implying unrun checks passed.

## Design and review maintainability

Assess architecture before implementation and again on the completed diff. Identify the owning layer, dependency direction, established contracts, and known near-term consumers. Compare approaches by the total concepts and maintenance obligations they introduce: how many places must understand the policy, where future changes must propagate, and what can be deleted. Prefer one clear representation and owner over local patches that spread special cases or parallel mechanisms. Remove superseded machinery within the chosen scope. Avoid compatibility scaffolding in this greenfield project and speculative frameworks for unknown consumers.

Review maintainability and cost as seriously as correctness. Trace changed behavior through its producers and consumers, then ask whether the change leaves redundant concepts, representations, conversions, traversals, policies, branches, allocations, or APIs. Examine module cohesion in both directions, including oversized files and one-function files created only to satisfy a size limit. Watch for duplicated behavior, hidden work in hot paths, unnecessary clones, and fallback or validation that repeats established phase contracts. These are prompts, not automatic findings. Check proposed replacements against compiler speed, generated code speed and size, and the issue scope. Passing tests establishes behavioral evidence, not architectural quality. Report only problems supported by the code and a concrete improvement. Do not turn design review into an unrelated redesign or a source of speculative follow-up issues.

## Write pull requests

Use lower-case Conventional Commits titles in the form `type: description`, without a parenthesized scope. Keep the title brief and specific.

Use these description sections in order:

- `## Problem`: Explain why the PR exists in a short paragraph. Use simple language that is easy to understand without prior context.
- `## Change Summary`: List the main changes as broad, clear bullets. Use imperative verbs such as "Add", "Fix", and "Refactor", rather than past-tense or third-person forms. Include details that have significant impact without listing every implementation step.
- `## Related Issues`: Include this section when GitHub or Linear issues are mentioned. Link each issue with only its ID as the displayed text, such as `ABC-123` or `#123`. Omit this section when there are no related issues, and do not treat the PR itself or formatting examples as related issues.
- `## LLM Contributions`: List the actual contributing model names. Use verified session information and never copy example names or guess a model identity.

Do not include verification or test steps unless the user explicitly asks for them. Keep descriptions and comments understandable on their own and useful long term, following the writing rules in `AGENTS.md`. Keep temporary checkpoints and environment details in the task record rather than publishing them in issues, PRs, or documentation.

## Attribute PR comments

Start every agent-authored PR comment with `Posted by <model name> on behalf of <GitHub user name>:` followed by a blank line. Apply this to review summaries, inline findings, replies, and comments posted for subagents. Use the authoring agent's model name from session information and the verified authenticated GitHub login. When publishing a subagent's review, retain that subagent's model attribution. Do not guess a model name or add agent attribution to human-authored comments.

## Bound build storage

Before the first expensive Rust build, resolve and size Cargo's target directory. Reuse repository-approved shared Cargo, dependency, and LLVM storage. Otherwise use one task-owned target for the whole stack, never one per issue, branch, review, or reproduction. Never clean shared storage.

Keep checks lightweight and focused on the Cargo target, usually `target/debug`. Do not inventory artifacts or investigate normal growth. Reuse outputs. If disk space cannot safely support another broad build, report the target path and size instead of running it.

At handoff, stop task-owned processes and report task-owned target or toolchain paths and sizes. Reclaim space only with a bounded build-tool command such as `cargo clean`, verified against task-owned output and an explicit target when supported. Never use `rm`, `Remove-Item`, `rmdir`, `del`, filesystem scripts, or equivalent raw deletion commands.

## Continue the stack

After an issue reaches review, update the task record and start the next. Do not stop at a commit, push, draft pull request, review pass, or Linear state change.

Treat user questions, corrections, and status requests as steering: answer in commentary, apply them, and resume. They replace only the affected part unless the user replaces the stack. End only when the full stack is complete, the user explicitly says stop, end, pause, or replace, or a real blocker needs new authority, external change, or material choice.

Keep one brief task record for the whole stack, shared by checkpoints, checklists, and validation notes. Include the `AGENTS.md` design commitments, remaining work, issue and repository references, current validation and review evidence, blockers, and next action. Update it in place and delete superseded task-owned records and stale entries. Retain only decisions and evidence still needed to finish or resume. Do not keep parallel bookkeeping or an implementation history. After compaction, read this record and do not repeat completed work. Revalidate only mutable external state.
