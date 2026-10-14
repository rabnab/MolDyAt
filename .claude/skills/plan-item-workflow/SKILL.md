---
name: plan-item-workflow
description: End-to-end workflow for implementing one item of docs/plan/refactoring-plan.md (G1, M3, I1, P4, ...) or a GitHub issue - picking the item, branching, the gates every refactor must pass, commits, updating the plan, and opening the PR with gh. Use when starting work on a plan item or issue, or when finishing one and opening its PR.
---

# Plan Item Workflow

Each item in `docs/plan/refactoring-plan.md` is one issue and one PR. IDs (`M1`, `P3`, ...) are plan-local; the issue number is in the plan's §4 issue table. Policies referenced here live in `.github/copilot-instructions.md` and `CONTRIBUTING.md`.

## 1. Pick and understand the item
- Check the item's **Deps** column in §3: all hard prerequisites must have landed (§4 table, "Landed on `master`"). Follow the phase order in §4 unless the user says otherwise.
- Read the issue: `gh issue view <N> --json number,title,body,milestone,projectItems,labels`. If the item has no issue yet, create one (`gh issue create --title ... --body ...`, linking the parent `#219`/`#220` or the phase's parent), and add its number to the §4 table in the same PR.
- If the scope in the issue and the plan disagree, ask the user before starting.

## 2. Branch
```bash
git switch master && git pull --rebase
git switch -c feature/master/<N>-<short-slug>    # bugfix/master/... for bug fixes
```
Never commit to `master`.

## 3. Implement with the right gates
Decide first whether the item may change simulation results.

| Item kind | Results may change? | Required evidence |
|---|---|---|
| Refactor (M, C, V, most G) | no | `cargo ci-test-headless-regression`, `cargo ci-test-drift`, `cargo test --workspace thread_count` pass **with fixtures and thresholds untouched** |
| Performance (P) | no | the refactor gates, plus before/after numbers: follow the `perf-compare` skill |
| Physics / model (I, some P) | yes, intentionally | follow the `physics-change` skill: baselines, threshold rationale, fixture diff |
| Headless output, CLI | output may change | follow the `headless-contract` skill |
| Operators, sidebar tools | – | follow the `operator-framework` skill |

Reproducibility is a hard requirement (plan decision 4): no float `sum`/`reduce` whose order depends on Rayon scheduling. Collect parallel results in index order and reduce serially or in fixed chunks.

Always, before each commit (`.rs`, `.toml` or `.lock` changed):
```bash
cargo fmt
cargo clippy --workspace -- -D warnings
cargo test -p <package> <filter>      # smallest scope first, then the gates above
```
New `.rs` / `.wgsl` files start with the MPL-2.0 header. Tests go in topic files under `tests/`.

## 4. Commits
- Imperative title with the issue number, matching the history: `Reuse step scratch buffers instead of allocating per step (#228)`.
- One logical change per commit; keep refactor and behaviour change in separate commits when both are needed.
- End with the attribution trailer required by the session.

## 5. Update the docs in the same PR
- `docs/plan/refactoring-plan.md`: move the item from "Open" to "Landed on `master`" in the §4 table (it lands with the merge). If the item changes a decision or an open question, update §6/§7.
- Update the concept docs that describe the changed area (`docs/architecture.md`, `docs/operators.md`, `docs/headless.md`, ...). Follow the `docs-maintenance` skill; when a whole phase or plan is done, move it to `docs/done/`.
- If a rule in `.github/copilot-instructions.md` or a skill no longer matches the code, fix it.

## 6. Open the PR
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
- **Physics Impact:** tick only what was verified; say "no change to results: fixtures and drift thresholds untouched" for refactors.
- **Testing Commands:** the exact commands run, including the gates.
- Performance or threshold evidence goes in the body (tables from the respective skills).
- End the body with the attribution line required by the session.

Then set the metadata explicitly (copy it from the issue):
```bash
gh pr edit <PR> --milestone "<milestone of the issue>"
gh pr edit <PR> --add-project "<project of the issue>"
```

## 7. After review
- Address comments with new commits; rebase on `master` if it moved (`git pull --rebase --autostash`, then `--force-with-lease`).
- Squash and merge when appropriate. After the merge, check that the issue closed and the plan table is right.
