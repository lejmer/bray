# Bray Discussions

The [official Bray discussion board](https://github.com/lejmer/bray/discussions) hosts questions, ideas, project updates, and BEP discussions.
Reproducible bugs and tracked work belong in [Issues](https://github.com/lejmer/bray/issues).

## Categories

Each category has a format and a defined purpose.

| Category | Format | Description |
| --- | --- | --- |
| Announcements | Announcement | Maintainer updates about Bray releases and project news. |
| BEPs | Open-ended discussion | Discuss Bray Enhancement Proposals, review their designs, and record decisions. |
| Ideas | Open-ended discussion | Explore suggestions before writing a formal proposal. |
| Q&A | Question and answer | Ask for help with Bray, its compiler, or its tooling. |
| General | Open-ended discussion | Discuss Bray topics that do not fit another category. |
| Show and tell | Open-ended discussion | Share programs, libraries, tools, and experiments built with Bray. |

GitHub's announcement format limits new threads to maintainers and administrators.
Community members can comment and reply.
The [GitHub category documentation](https://docs.github.com/en/discussions/managing-discussions-for-your-community/managing-categories-for-discussions)
describes the formats and category controls.

The Polls category is optional.
Poll results provide feedback.
BEP decisions follow the BDFL authority and review rules in [BEP 1](../../beps/bep-0001.md).

## BEP discussions

Each BEP has one canonical thread in BEPs.
That thread contains design feedback, final comment announcements, and decisions.
The BEP document records the current status and links to the decision.

The same thread stays in BEPs through review, acceptance, rejection, deferral, or withdrawal.
The document's status tracks the decision.
Categories describe the subject of a discussion.

An early thread may become a BEP's canonical discussion.
If the thread starts in Ideas, a maintainer moves it to BEPs before admission.
The existing URL and conversation stay with the proposal.

## Category forms

GitHub loads category forms from `.github/DISCUSSION_TEMPLATE/` on the default branch.
Each filename matches a category slug.

| Category | Form file |
| --- | --- |
| BEPs | [`beps.yml`](../../.github/DISCUSSION_TEMPLATE/beps.yml) |
| Ideas | [`ideas.yml`](../../.github/DISCUSSION_TEMPLATE/ideas.yml) |
| Q&A | [`q-a.yml`](../../.github/DISCUSSION_TEMPLATE/q-a.yml) |

The BEPs form asks for a document link, a summary, and review questions.
The document link is optional so discussion can start before a draft exists.
The Ideas form asks about the problem and a possible approach.
The Q&A form asks for the question and any relevant version, attempted solution, code, or output.

The [GitHub form documentation](https://docs.github.com/en/discussions/managing-discussions-for-your-community/creating-discussion-category-forms)
describes how GitHub selects and displays these forms.
