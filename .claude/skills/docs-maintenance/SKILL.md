---
name: docs-maintenance
description: Rules for creating, moving, rewriting or deleting files in docs/ and keeping every reference to them valid. Use when adding a doc, writing or finishing a plan, moving a plan to docs/done/, renaming a doc, or updating README/CONTRIBUTING doc lists.
---

# Docs Maintenance

## Layout
| Folder | Holds | Style |
|---|---|---|
| `docs/` | How the application works now: architecture, simulation, units, operators, headless mode, drift suite, perf baseline | Present tense, describes the code as it is |
| `docs/plan/` | Plans not yet (fully) implemented | Proposals, work items, open questions |
| `docs/done/` | Implemented plans and reviews, kept as history | Unchanged except for a status line and link fixes |

- Concept docs describe; they do not instruct. Rules for changing code ("when you add X, also update Y") belong in a skill under `.claude/skills/` or in `.github/copilot-instructions.md`. Exceptions are processes that policy points to (for example threshold changes in `docs/drift-test.md`).
- Only one place states a fact. Commands and policies live in `.github/copilot-instructions.md`; docs link to it instead of copying.
- Verify every claim against the code before writing it: types, file paths, function names, constants, key bindings. Prefer naming a type or file over quoting numbers that change.

## Finishing a plan
1. Check that the plan is implemented in the code (not only that the issue is closed). If part is still open, the plan stays in `docs/plan/` or the open items are listed in the status line.
2. `git mv docs/plan/<name>.md docs/done/` (or from `docs/`), so history follows the file.
3. Add a status line under the title, e.g. `Status (YYYY-MM-DD): implemented in #NNN. Items 7 and 9 are still open.`
4. If the plan holds reference material that is still current (formats, CLI flags, workflows), move that part into a concept doc in `docs/` and link to it from the plan.
5. Update the concept docs that describe the area so they match the implementation.
6. In `docs/plan/refactoring-plan.md`, mark landed items in the issue table.

## After any move, rename or delete
Update every reference in the same change:
- `README.md` (Documentation section) and `CONTRIBUTING.md` (docs list)
- relative links inside `docs/` (a file one level deeper needs `../`)
- `.github/copilot-instructions.md` and `.claude/skills/*/SKILL.md`
- doc paths in code comments (`grep -rn "docs/" core wgpu_renderer scripts`)

Find all references first:
```bash
grep -rn --exclude-dir=target --exclude-dir=.git "<old-name>" .
```
Then check links and anchors:
```bash
python3 - <<'EOF'
import re, os, glob
files = glob.glob('docs/**/*.md', recursive=True) + ['README.md', 'CONTRIBUTING.md']
def anchors(p):
    return {re.sub(r'[^\w\- ]', '', l.lstrip('#').strip().lower()).replace(' ', '-')
            for l in open(p) if l.startswith('#')}
for f in files:
    for m in re.finditer(r'\]\(([^)\s]+)\)', open(f).read()):
        link = m.group(1)
        if link.startswith(('http', 'mailto')): continue
        path, _, anc = link.partition('#')
        t = os.path.normpath(os.path.join(os.path.dirname(f), path)) if path else f
        if not os.path.exists(t): print('BROKEN', f, link)
        elif anc and t.endswith('.md') and anc not in anchors(t): print('ANCHOR', f, link)
EOF
```

## Gotchas
- Math: write `\lt` / `\gt` instead of `<` / `>` inside `$$ … $$`. `\sum_{i<j}` is read as an HTML tag by many Markdown renderers and breaks the formula.
- The JetBrains IDE may rewrite path-like text in a moved Markdown file (e.g. `core/src/...` becomes `../core/src/...`) or revert a file right after `git mv`. Check `git diff` after every move.
- Repo-root paths in backticks (`core/src/...`) are not links and must not get `../` prefixes.
- Changing a `.rs` doc comment is a `.rs` change: run `cargo fmt` and `cargo clippy --workspace -- -D warnings`.
