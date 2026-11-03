# AI contribution policy

You may use AI tools and coding agents to contribute to Bray. You remain responsible for the work you submit, including
code, tests, documentation, issues, proposals, and review replies. A human contributor must review the work before
submission and take responsibility for follow-up changes.

## Review and understand the work

Read every change before you submit it. Check that the change addresses the stated problem and follows Bray's design
and contribution rules. You must understand the implementation well enough to explain its behavior and tradeoffs to a
reviewer. If you cannot explain part of a change, investigate it or remove it before submission.

Agents may research, implement, test, and revise changes under your supervision. You decide what to submit and remain
responsible for answering review questions. AI assistance does not replace the BEP process or authorize a design that
the project has not approved.

## Disclose AI code changes

Whenever AI generates or edits code in your contribution, disclose that use in the pull request description. Name each
AI model used. Include the model version(s) when available, although including the exact version(s) is optional. You do not
need to identify which edits each model made.

This requirement applies to all AI code changes, including small edits and code you revise by hand after generation.
Update the disclosure if you use another model or submit further AI code changes during review.

## Verification

Check generated code against the language specification and the relevant implementation contracts. Run the checks
appropriate to the change, as described in the [contribution guide](../CONTRIBUTING.md). Inspect test results and generated snapshots before
accepting them. Never weaken tests to make a generated implementation pass.

## Communicate with reviewers

You may use AI to draft or edit descriptions and review replies. Read the text before posting and make sure it reflects
your understanding and intent. Answer the reviewer's actual question and verify any technical claims in the response.
Do not submit generated explanations that you cannot defend or leave an agent to conduct review without your oversight.

## Respect source and data restrictions

Check that you have the right to submit generated material under the repository's license. Preserve required attribution
and license notices. Do not send credentials, private code, or other confidential data to an AI service without
authorization from the owner of that data.

## Meet the contribution standard

AI-assisted contributions must meet the same correctness, design, testing, and documentation requirements as other
contributions. This includes code formatting. Maintainers may request revisions or close contributions that lack human
review, contain unsupported claims, or require reviewers to reconstruct work that the contributor does not understand.
