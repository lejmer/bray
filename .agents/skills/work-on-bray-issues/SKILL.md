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
4. Before editing, state the acceptance criteria and trace the behavior through its owning modules, producers, and consumers. Choose the smallest coherent design using the architecture guidance below, including what existing machinery it replaces. Keep necessary adjacent fixes in the active pull request and apply the follow-up issue policy below.
5. Implement a coherent batch before validating it. For cross-cutting changes, keep a short checklist of affected contracts and meaningful boundaries, informed by targeted searches. Use it to catch predictable omissions, not to audit or classify the entire repository. Self-review against that checklist, the architecture guidance, and applicable `AGENTS.md` rules, then perform end-of-implementation validation as described below. Commit and push the coherent change, then create or update its draft pull request. Checkpoint the issue, branch, commit, pull request, and validation results.
6. Dispatch a fresh review agent only after the draft pull request exists and contains the commit under review. The review happens on that pull request so the reviewer has a destination for inline comments and its summary. A pre-PR review does not satisfy this workflow. The review agent itself publishes the review summary and every genuine actionable finding directly on the pull request. Do not ask it to return findings for the parent agent to publish. Review against the contract, full diff, tests, architecture, repository rules, affected-contract checklist, and maintainability guidance below. If there are no findings, the reviewer says so in the pull-request summary. Never invent or stretch a finding to produce a comment. A cycle is complete only after the reviewer returns the GitHub review URL or ID and the URL or ID of every inline comment. Treat a chat-only review result as incomplete, including after compaction, and verify the returned artifacts on the pull request before acting on the review.
7. Triage findings against the agreed contract and concrete architectural or correctness impact. Batch valid fixes, reply with the change or a concrete disagreement, and resolve addressed threads. If a finding exposes a defect class, inspect equivalent paths in the affected mechanism and fix the class together. Validate the completed batch using the policy below. Request re-review after material correctness or design changes. Cosmetic edits, documentation wording, and unchanged code do not restart the review cycle. End agent review when actionable findings are resolved or disagreements recorded. Once agent review is clean, a user correction in task chat or a human PR comment becomes the active human review, including documentation-only feedback. Fix and respond without launching another review agent unless the human reviewer asks for one.
8. Mark the pull request ready and move the issue to `In Review` only after implementation, verification, and review. Never mark it `Done`. Linear does that when the user merges the pull request.

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

Keep a compact validation record with the command, relevant code state, result, and any coverage limitation. Reuse valid results across commits, review, handoff, and compaction. Fix regressions introduced by the change. Report blocked or unavailable checks honestly instead of repeatedly retrying an unchanged environment or implying unrun checks passed.

## Design and review maintainability

Assess architecture before implementation and again on the completed diff. Identify the owning layer, dependency direction, established contracts, and known near-term consumers. Compare approaches by the total concepts and maintenance obligations they introduce: how many places must understand the policy, where future changes must propagate, and what can be deleted. Prefer one clear representation and owner over local patches that spread special cases or parallel mechanisms. Remove superseded machinery within the chosen scope. Avoid compatibility scaffolding in this greenfield project and speculative frameworks for unknown consumers.

Review maintainability and cost as seriously as correctness. Trace changed behavior through its producers and consumers, then ask whether the change leaves redundant concepts, representations, conversions, traversals, policies, branches, allocations, or APIs. Examine module cohesion in both directions, including oversized files and one-function files created only to satisfy a size limit. Watch for duplicated behavior, hidden work in hot paths, unnecessary clones, and fallback or validation that repeats established phase contracts. These are prompts, not automatic findings. Check proposed replacements against compiler speed, generated code speed and size, and the issue scope. Passing tests establishes behavioral evidence, not architectural quality. Report only problems supported by the code and a concrete improvement. Do not turn design review into an unrelated redesign or a source of speculative follow-up issues.

## Attribute PR comments

Start every agent-authored PR comment with `Posted by <model name> on behalf of <GitHub user name>:` followed by a blank line. Apply this to review summaries, inline findings, replies, and comments posted for subagents. Use the authoring agent's model name from session information and the verified authenticated GitHub login. When publishing a subagent's review, retain that subagent's model attribution. Do not guess a model name or add agent attribution to human-authored comments.

## Bound build storage

Before the first expensive Rust build, resolve and size Cargo's target directory. Reuse repository-approved shared Cargo, dependency, and LLVM storage. Otherwise use one task-owned target for the whole stack, never one per issue, branch, review, or reproduction. Never clean shared storage.

Keep checks lightweight and focused on the Cargo target, usually `target/debug`. Do not inventory artifacts or investigate normal growth. Reuse outputs. If disk space cannot safely support another broad build, report the target path and size instead of running it.

At handoff, stop task-owned processes and report task-owned target or toolchain paths and sizes. Reclaim space only with a bounded build-tool command such as `cargo clean`, verified against task-owned output and an explicit target when supported. Never use `rm`, `Remove-Item`, `rmdir`, `del`, filesystem scripts, or equivalent raw deletion commands.

## Continue the stack

After an issue reaches review, checkpoint it and start the next. Do not stop at a commit, push, draft pull request, review pass, or Linear state change.

Treat user questions, corrections, and status requests as steering: answer in commentary, apply them, and resume. They replace only the affected part unless the user replaces the stack. End only when the full stack is complete, the user explicitly says stop, end, pause, or replace, or a real blocker needs new authority, external change, or material choice.

Keep a compact checkpoint: objective, remaining order, completed issue and pull-request IDs, worktree and base, check and review outcomes, blockers, and next action. After compaction, do not repeat completed work. Revalidate only mutable external state.
