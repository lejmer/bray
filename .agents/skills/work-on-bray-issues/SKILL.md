---
name: work-on-bray-issues
description: >
  Use for Bray Linear issue implementation, issue stacks, stacked or draft pull requests, pull-request review cycles,
  review comments, unattended multi-issue work, and questions, corrections, or status requests that steer an active
  stack. Do not use for read-only issue triage or planning that will not implement or review repository changes.
---

# Work on Bray issues

Complete the requested issue or ordered stack through implementation, verification, review, pull requests, and Linear handoff. The whole stack is the terminal objective. A pull-request boundary is only a checkpoint.

## Run the stack

1. Read applicable `AGENTS.md` files, complete Linear issues and relations, and linked design contracts. Inspect the worktree, branches, pull requests, and existing changes. Preserve work owned by others.
2. Order the stack by dependency and keep it active. Move an issue to `In Progress` only when its implementation starts.
3. Use one isolated worktree for the stack and one branch per pull request. Add a worktree only for concurrent independent work or at the user's request. Stack branches only for real dependencies.
4. Keep the issue contract and discovered gaps in the active pull request. Size, cross-module work, a changed diff, or added tests do not justify a split. Split only a multi-thousand-line addition or genuinely independent subsystem. Search for an active Linear owner first. Closed, completed, cancelled, and duplicate issues are historical relations only. Never reopen, extend, or assign new work to them. If no active owner exists, create one issue for the coherent independent outcome, not separate issues for symptoms, layers, call sites, tests, or cleanup. Then resume the active issue.
5. Run focused checks while editing and repository-required verification plus the `AGENTS.md` audit before review. For cross-cutting changes, first make a lightweight review matrix from the issue contract and every materially changed behavior category. Cover each relevant variant, conversion boundary, output surface, sibling implementation, and public API concern so review cycles are not used to discover predictable adjacent cases. Commit and push a coherent change, update its draft pull request, and checkpoint the issue, branch, commit, pull request, and results.
6. Use a fresh review agent for each cycle against the contract, full diff, tests, architecture, repository rules, and review matrix. Every cycle publishes a summary. Publish genuine actionable findings as inline comments. If there are none, say so in the summary. Never invent or stretch a finding to produce a comment.
7. Fix valid findings, reply with the change or a concrete disagreement, and resolve addressed threads. When a finding reveals a defect class, audit all structurally equivalent paths and update the review matrix before requesting re-review. Use focused checks while iterating, then run the required broad validation once the systemic audit is complete. Do not repeat unchanged broad checks merely to start another cycle. Request fresh re-review after material fixes, and repeat until there are no findings or all disagreements are recorded.
8. Mark the pull request ready and move the issue to `In Review` only after implementation, verification, and review. Never mark it `Done`. Linear does that when the user merges the pull request.

## Bound build storage

Before the first expensive Rust build, resolve and size Cargo's target directory. Reuse repository-approved shared Cargo, dependency, and LLVM storage. Otherwise use one task-owned target for the whole stack, never one per issue, branch, review, or reproduction. Never clean shared storage.

Keep checks lightweight and focused on the Cargo target, usually `target/debug`. Do not inventory artifacts or investigate normal growth. Reuse outputs. If disk space cannot safely support another broad build, report the target path and size instead of running it.

At handoff, stop task-owned processes and report task-owned target or toolchain paths and sizes. Reclaim space only with a bounded build-tool command such as `cargo clean`, verified against task-owned output and an explicit target when supported. Never use `rm`, `Remove-Item`, `rmdir`, `del`, filesystem scripts, or equivalent raw deletion commands.

## Continue the stack

After an issue reaches review, checkpoint it and start the next. Do not stop at a commit, push, draft pull request, review pass, or Linear state change.

Treat user questions, corrections, and status requests as steering: answer in commentary, apply them, and resume. They replace only the affected part unless the user replaces the stack. End only when the full stack is complete, the user explicitly says stop, end, pause, or replace, or a real blocker needs new authority, external change, or material choice.

Keep a compact checkpoint: objective, remaining order, completed issue and pull-request IDs, worktree and base, check and review outcomes, blockers, and next action. After compaction, do not repeat completed work. Revalidate only mutable external state.
