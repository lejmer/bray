# Bray Enhancement Proposals

Bray Enhancement Proposals (BEPs) describe substantial changes to Bray and preserve the reasoning behind them.
They cover the language, official libraries and tooling, and project processes.

Start with [BEP 1: The BEP Process](bep-0001.md) for governance, scope, proposal types, statuses, and review rules.
Use the [BEP template](template.md) to write a proposal.

Design discussions take place in the [official Bray repository's GitHub Discussions](https://github.com/lejmer/bray/discussions),
in the **BEPs** category. Each proposal links to one canonical discussion.
Pull requests submit and revise the Markdown documents.

## Index

| BEP | Title | Type | Status |
| --- | --- | --- | --- |
| [1](bep-0001.md) | The BEP Process | Process | Draft |

Keep this index consistent with each document's metadata when admitting a proposal or changing its title, type, or status.
BEP numbers identify proposals independently of GitHub issue, discussion, and pull request numbers.

## Submit a proposal

1. Optionally discuss the problem in GitHub Discussions.
2. Copy [the template](template.md) to `beps/bep-0000.md` and fill in the applicable sections and metadata.
   Use `bep: null` until an editor assigns a number.
3. Open a pull request against the repository's default branch.
4. Link the canonical GitHub Discussion. An early discussion can become that thread.
5. An editor checks the submission, assigns the next unused positive BEP number, renames the file, and updates the index
   before admitting the draft.

Use filenames with at least four digits, such as `bep-0002.md`. Assigned numbers are permanent and are never reused.
Admission and merging document changes do not constitute design acceptance.

## Adoption

BEP 1 is the initial process proposal. Its Draft status means formal adoption is pending.
Adoption requires an explicit BDFL decision in its canonical GitHub Discussion, identifying the approved revision and
effective date. Any shortened review follows the expedited-decision rules in BEP 1.

Existing proposals and their decision history remain available after rejection, withdrawal, or supersession.
