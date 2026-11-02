# Submit a BEP

To submit a Bray Enhancement Proposal, open a pull request in the
[official Bray repository](https://github.com/lejmer/bray).
[BEP 1](bep-0001.md) defines which changes require a proposal.

1. For feedback before drafting, open a discussion in the repository's **BEPs** category.
   Skip this step if you want to write the draft first.
2. Copy [the template](template.md) to `beps/bep-0000.md`.
3. Fill in the title, authors, creation date, and proposal type.
   Leave `bep` as `null`.
   Keep `status` as `Draft`.
   Retain the delivery fields for your proposal type.
   Remove the delivery fields for other types.
4. Complete the template's eight sections.
   Follow the [writing rules](bep-0001.md#writing-rules).
   For a section that does not apply, explain why.
   List unresolved questions in Open questions.
5. Before admission, link one canonical GitHub Discussion in the `discussion` field.
   If you opened an early discussion, use that thread.
   Otherwise, open a thread in the **BEPs** category.
   If the category is unavailable, ask a repository maintainer to create it.
6. Open a pull request against the repository's default branch.
   Request admission of the draft.
   An editor checks the draft, assigns a number, renames the file, and updates the index.
7. After the editor assigns a number, add the BEP number to the discussion title.
   Respond to design feedback in that discussion.
   Submit document changes through pull requests.
8. When the proposal meets the [review requirements](bep-0001.md#submission-and-review), request `In review` status in its discussion.
