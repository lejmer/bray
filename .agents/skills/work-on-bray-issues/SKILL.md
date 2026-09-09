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
2. Before implementation, divide substantial work into dependency-ordered outcomes that a human can review independently. Keep each outcome coherent and testable, and record the complete contract across the stack. Reassess the plan when a discovered gap introduces substantial new machinery, before implementing that expansion. Do not wait for a multi-thousand-line diff to consider a split. Keep the stack active and move an issue to `In Progress` only when its implementation starts.
3. Use one isolated worktree for the stack and one branch per pull request. Add a worktree only for concurrent independent work or at the user's request. Stack branches only for real dependencies.
4. Fix bounded defects within the active outcome. Put substantial prerequisites or follow-on behavior in a separately reviewable issue and pull request when a coherent contract boundary permits it. Splitting must preserve the full intended result and leave intermediate changes correct. Follow explicit user instructions to finish an already expanded pull request in full. Search for an active Linear owner first. Closed, completed, cancelled, and duplicate issues are historical relations only. Never reopen, extend, or assign new work to them. If no active owner exists, create one issue for the coherent outcome, not separate issues for symptoms, call sites, or cleanup. Then continue the ordered stack.
5. Run focused checks while editing and repository-required verification plus the `AGENTS.md` audit before review. For cross-cutting changes, first make a lightweight review matrix from the issue contract and repo-wide searches for every affected behavior category. Cover each relevant variant, conversion boundary, output surface, sibling implementation, and public API concern. Classify or resolve every search result before requesting review so review cycles are not used to discover predictable adjacent cases. Commit and push a coherent change, update its draft pull request, and checkpoint the issue, branch, commit, pull request, and results.
6. Use a fresh review agent for each cycle against the contract, full diff, tests, architecture, repository rules, and review matrix. Every cycle publishes a summary. Publish genuine actionable findings as inline comments. If there are none, say so in the summary. Never invent or stretch a finding to produce a comment.
7. Fix valid findings, reply with the change or a concrete disagreement, and resolve addressed threads. When a finding reveals a defect class, audit all structurally equivalent paths and update the review matrix before requesting re-review. Use focused checks while iterating, then run the required broad validation once the systemic audit is complete. Do not repeat unchanged broad checks merely to start another cycle. Request fresh re-review after material fixes, and repeat until there are no findings or all disagreements are recorded. After the agent review cycle is clean and a human reviewer takes over, treat the human review as the active review cycle. Fix and respond to human findings without launching another review agent unless the human reviewer asks for one.
8. Mark the pull request ready and move the issue to `In Review` only after implementation, verification, and review. Never mark it `Done`. Linear does that when the user merges the pull request.

## Preserve progress

During long tasks, commit and push at coherent, tested milestones instead of waiting for review readiness. Before a restart or a substantial change of direction, push the latest useful checkpoint. Mark incomplete checkpoints clearly as work in progress, record verification and remaining gaps, and keep the pull request draft. A checkpoint push does not mean implementation or review is complete. Preserve scratch evidence locally and keep generated artifacts, temporary files, and editor backups out of source commits.

## Bound build storage

Before the first expensive Rust build, resolve and size Cargo's target directory. Reuse repository-approved shared Cargo, dependency, and LLVM storage. Otherwise use one task-owned target for the whole stack, never one per issue, branch, review, or reproduction. Never clean shared storage.

Keep checks lightweight and focused on the Cargo target, usually `target/debug`. Do not inventory artifacts or investigate normal growth. Reuse outputs. If disk space cannot safely support another broad build, report the target path and size instead of running it.

At handoff, stop task-owned processes and report task-owned target or toolchain paths and sizes. Reclaim space only with a bounded build-tool command such as `cargo clean`, verified against task-owned output and an explicit target when supported. Never use `rm`, `Remove-Item`, `rmdir`, `del`, filesystem scripts, or equivalent raw deletion commands.

## Continue the stack

After an issue reaches review, checkpoint it and start the next. Do not stop at a commit, push, draft pull request, review pass, or Linear state change.

Treat user questions, corrections, and status requests as steering: answer in commentary, apply them, and resume. They replace only the affected part unless the user replaces the stack. End only when the full stack is complete, the user explicitly says stop, end, pause, or replace, or a real blocker needs new authority, external change, or material choice.

Keep a compact checkpoint: objective, remaining order, completed issue and pull-request IDs, worktree and base, check and review outcomes, blockers, and next action. After compaction, do not repeat completed work. Revalidate only mutable external state.
