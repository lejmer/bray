---
name: babysit-pr
description: Get a PR ready to merge. Use when asked to babysit, watch, or follow a PR.
---

# Babysit PR

The PR you are doing this for is:

- If you are in a branch with a remote PR on it already, that one
- If the user includes a specific PR in their request, babysit that one

The end state here is:

- The PR doesn't have merge conflicts with the target branch
- The CI checks (if any) are green
- Any AI code review bots (including subagents running in Codex/ChatGPT) are all done, green, and their reviews have been addressed (by either implementing them or by ignoring them because they are not worth addressing, but always with all inline comments answered and resolved)

Steps to take:

1. Watch the PR for CI (if any) and code review. Check the AI reviewers and their findings.
2. When issues come in, fix them locally, then commit and push them up.
3. Wait for checks to run again (if any), and if there are more issues, repeat step 2, otherwise move on to the next step.
4. Give the user a concise summary of the changes you made to fix the PR and a concise list of things the PR actually does.

NOTE: If the user asks for an extra review from you, use a subagent to do that review and treat it like one of the AI code reviewers. Take it's findings and follow the steps above.
