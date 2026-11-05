---
name: changelog
description: Rules for writing CHANGELOG.md entries - which changes get one, the entry format and length, where new entries go, and how the Unreleased section becomes a release section. Use when finishing a feature, fix or behaviour change, before opening its PR, when resolving a CHANGELOG.md merge conflict, or when cutting a release.
---

# Changelog

`CHANGELOG.md` in the repository root collects user-visible changes between releases. It has one `## Unreleased` section at the top, followed by one section per release, newest first.

## Which changes get an entry
- New features, changed behaviour, removed features, bug fixes users could notice.
- Save-format version bumps, headless output schema or CLI flag changes (mark them **Breaking:** when old saves, scripts or output consumers stop working).
- Notable performance changes, with the rough factor.
- No entry for refactors, tests, CI, docs, skills or dependency bumps without a visible effect.

## Writing an entry
Add it in the same PR as the change, as the **first** bullet under `## Unreleased`, so newer changes stay on top.
- At most three lines (wrapped at about 100 characters). One sentence is usually enough.
- Say what the user sees or can do, not how the code does it: "Click a g(r) legend entry to hide or show that pair", not "Add visibility flags to RdfPlot".
- Imperative or plain past tense, matching the PR title; end with the issue or PR number: `(#402)`.
- Several small, related changes in one PR: one entry, not one per commit.
- A follow-up to a change that is still under `Unreleased`: edit that entry instead of adding a new one.

```markdown
## Unreleased

- Scale the field overlay to the world's averages instead of fixed limits. (#402)
- Click a g(r) legend entry to hide or show that pair. (#400)
```

## Merge conflicts
Two PRs adding entries conflict at the top of `## Unreleased`. Keep both bullets, the one being rebased on top.

## At release
The release skill (step 2) calls for this, on the release branch:
1. Rename `## Unreleased` to `## vX.Y.Z - YYYY-MM-DD` (the tag and the date from `CITATION.cff`).
2. Insert a new, empty `## Unreleased` above it.
3. Reread the section: merge duplicates, drop entries for changes reverted before the release, move **Breaking:** entries to the top.

Never edit the entries of a published release, except to fix a wrong statement.
