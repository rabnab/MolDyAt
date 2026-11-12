---
name: issue-workflow
description: End-to-end workflow for implementing one GitHub issue - understanding it, branching, the gates every refactor must pass, commits, docs, and opening the PR with gh. Use when starting work on an issue, or when finishing one and opening its PR.
---

# Issue Workflow

Each issue is one PR. Policies referenced here live in `.github/copilot-instructions.md` and `CONTRIBUTING.md`; binding design decisions in `docs/design-decisions.md`. GitHub steps are written as `gh` commands; with GitHub MCP tools connected (cloud sessions), use their equivalents (see "Tools" in `.github/copilot-instructions.md`).

## 1. Understand the issue
- Read it: `gh issue view <N> --json number,title,body,milestone,projectItems,labels`. If it is a sub-issue, read the parent for the order and shared rules (e.g. #460 for performance work).
- Prerequisites named in the issue must have landed on `master`.
- If the issue conflicts with `docs/design-decisions.md`, ask the user before starting.

## 2. Branch
```bash
git switch master && git pull --rebase
git switch -c feature/master/<N>-<short-slug>    # bugfix/master/... for bug fixes
```
Never commit to `master`.

## 3. Implement with the right gates
Decide first whether the item may change simulation results.

| Kind of change | Results may change? | Required evidence |
|---|---|---|
| Refactor | physics the same, bits may differ | `cargo ci-test-headless-regression`, `cargo ci-test-drift`, `cargo test --workspace thread_count` pass with **thresholds untouched**; a fixture refresh is allowed for rounding-level diffs only (CONTRIBUTING, Refactor PRs) |
| Performance | physics the same, bits may differ | the refactor gates, plus before/after numbers: follow the `perf-compare` skill |
| Physics / model | yes, intentionally | follow the `physics-change` skill: baselines, threshold rationale, fixture diff |
| Headless output, CLI | output may change | follow the `headless-contract` skill |
| Operators, sidebar tools | – | follow the `operator-framework` skill |

Bit-exact reproducibility across thread counts is the default, not a requirement: collect parallel results in index order and reduce serially or in fixed chunks, unless a scheduling-dependent float reduction buys real speed. Then follow the thread-count rule in CONTRIBUTING (Refactor PRs, gate 3). Data races stay bugs.

Before each commit: the validation policy in `.github/copilot-instructions.md` (smallest test scope first, then the gates above).

## 4. Commits
- Imperative title with the issue number, matching the history: `Reuse step scratch buffers instead of allocating per step (#228)`.
- One logical change per commit; keep refactor and behaviour change in separate commits when both are needed.
- End with the attribution trailer required by the session.

## 5. Update the docs in the same PR
No PR leaves docs or skills behind its change (design decision 12).
- If the change alters a design decision, update `docs/design-decisions.md` (owner's agreement needed).
- Update the concept docs that describe the changed area (`docs/architecture.md`, `docs/operators.md`, `docs/headless.md`, ...). Follow the `docs-maintenance` skill; when a whole phase or plan is done, move it to `docs/done/`.
- If a rule in `.github/copilot-instructions.md` or a skill no longer matches the code, fix it.
- A user-visible change gets a `CHANGELOG.md` entry; follow the `changelog` skill.

## 6. Shrink the diff
Before the PR, review your own diff with the goal of removing lines (minimal code policy in `.github/copilot-instructions.md`; tests are exempt). `/simplify` can help where available.
- Run `scripts/code_delta.sh` and go through every production file it lists.
- For each new type, `pub` function or field: extend an existing one instead, or note in the PR why that was not possible.
- Drop outputs, options and comments the issue did not ask for; delete or merge what the change made redundant or duplicated.
- Fill the PR's "Code Size" section: the script's summary line, what was deleted, and the justifications.

## 7. Open the PR
Check divergence, then push:
```bash
git --no-pager rev-list --left-right --count HEAD...origin/master
git push -u origin HEAD          # after a rebase: git push --force-with-lease
```
Create the PR from `.github/pull_request_template.md`, filled in, not left as placeholders:
```bash
gh pr create --base master --title "<commit-style title> (#<N>)" --body-file <filled-template.md>
```
- **Relations:** `Closes #<N>`, `Part of #<parent>`.
- **Physics Impact:** tick only what was verified; say "no physical change: drift thresholds untouched, fixtures untouched or refreshed for rounding only" for refactors.
- **Testing Commands:** the exact commands run, including the gates.
- Performance or threshold evidence goes in the body (tables from the respective skills).
- End the body with the attribution line required by the session.

Then set the metadata explicitly (copy it from the issue):
```bash
gh pr edit <PR> --milestone "<milestone of the issue>"
gh pr edit <PR> --add-project "<project of the issue>"
```

## 8. After review
- Address comments with new commits; rebase on `master` if it moved (`git pull --rebase --autostash`, then `--force-with-lease`).
- Squash and merge when appropriate. After the merge, check that the issue closed.
