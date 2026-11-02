# Set up Bray Discussions

Configure the [Bray discussion board](https://github.com/lejmer/bray/discussions) with the categories and forms in this repository.
Use a GitHub account with permission to manage the repository's discussion categories.

## Configure the categories

1. Open the repository's **Discussions** tab.
2. Next to **Categories**, select the edit control.
3. Select **New category**.
4. Enter **BEPs** as the name.
   Select a document emoji from the emoji picker.
5. Select the **Open-ended discussion** format.
6. Copy the BEPs description from the [category reference](discussions.md#categories).
7. Save the category.
8. Set the other categories' formats and descriptions to match the reference.
   Create a listed category if it is missing.

If you do not need Polls, check its discussions before removing it.
If it contains threads, keep the category until you choose where those threads belong.
GitHub moves threads when a category is deleted.

## Enable the forms

1. Merge the [category form files](discussions.md#category-forms) into the repository's default branch.
2. Start a new discussion.
3. Select **BEPs**.
   Confirm that GitHub displays the document link, summary, and review question fields.
4. Cancel the draft.
5. Repeat the check for **Ideas** and **Q&A**.

If a form does not appear, compare the category slug with the form's filename.
The filename before `.yml` must match the slug.
For example, `q-a.yml` matches the `q-a` category.

## Pin a navigation post

1. Start a discussion in **General**.
2. Use **Start here** as the title.
3. Copy the following text into the body.

```markdown
Use these categories to find or start a discussion.

- Announcements contains maintainer updates about releases and project news.
- BEPs contains proposal reviews and decisions.
- Ideas contains suggestions to explore before a formal proposal.
- Q&A contains questions about Bray, its compiler, and its tooling.
- General contains other Bray discussions.
- Show and tell contains projects built with Bray.

Search for an existing thread before opening another on the same topic.
Report reproducible bugs in [Issues](https://github.com/lejmer/bray/issues).

For a formal proposal, read [BEP 1](https://github.com/lejmer/bray/blob/develop/beps/bep-0001.md)
and the [submission guide](https://github.com/lejmer/bray/blob/develop/beps/submit-a-bep.md).
Each BEP has one canonical thread.
Its document records the current status and links to the BDFL or delegate's decision.
```

4. Publish the discussion.
5. Pin the discussion to the board.

## Open BEP 1's canonical discussion

1. Search **BEPs** for an existing discussion of **BEP 1: The BEP process**.
   If a canonical thread exists, use that thread.
2. If no thread exists, start one in **BEPs**.
3. Enter **BEP 1: The BEP process** as the title.
4. Link to [BEP 1](https://github.com/lejmer/bray/blob/develop/beps/bep-0001.md) in the document field.
5. Describe the proposal's purpose in the summary field.
   The proposal defines Bray's governance, proposal types, statuses, review procedure, and document requirements.
6. Ask reviewers to check whether the roles, review steps, and decision rules are clear.
7. Publish the discussion.
8. Submit a pull request that sets BEP 1's `discussion` field to the thread's URL.

When ready to decide, follow [BEP 1's review rules](../../beps/bep-0001.md#submission-and-review).
The BDFL or delegate announces the proposed outcome and the final comment deadline in the thread.
