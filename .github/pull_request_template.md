## Summary
<!-- Brief description of what this PR accomplishes -->

## Relations
<!-- Reference related issues and PRs -->
- Closes #
- Depends on #
- Part of #
- Related to #

## Goals
<!-- Clear list of objectives for this PR -->
-
-
-

## Implementation Overview
<!-- High-level description of the approach taken -->
-
-
-

## Key Changes
<!-- List major changes in this PR -->
- **Component Name**: Description
- **Component Name**: Description

## Technical Details
<!-- Architecture decisions, algorithms, or technical considerations -->
-
-

## Physics Impact (if applicable)
<!-- CRITICAL: For physics-related changes only -->
- [ ] Energy conservation verified
- [ ] Force calculations unchanged OR documented changes
- [ ] Integration method unchanged OR validated
- [ ] No impact on physics calculations

## Testing & Validation
<!-- Checklist for verifying functionality -->
- [ ] Feature works as expected
- [ ] No visual regressions
- [ ] Performance acceptable (no significant regressions)
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo fmt` applied
- [ ] Tests pass: `cargo test`

## Testing Commands
```bash
# Add relevant commands to test this PR
cargo build
cargo test
```

## Dependencies
<!-- New dependencies or significant dependency changes -->
-

## Breaking Changes
<!-- List any breaking changes to APIs or behavior -->
- [ ] No breaking changes
- [ ] Breaking changes (describe below):

## Notes
<!-- Additional context, warnings, or follow-up items -->
-

## Review Checklist for AI Agents
- [ ] All clippy warnings resolved (`-D warnings` enforced)
- [ ] Code formatted with `cargo fmt`
- [ ] No unsafe code without proper documentation and justification
- [ ] Method length kept reasonable (< 100 lines per function)
- [ ] Builder pattern used for dependency injection where appropriate
- [ ] Physics calculations maintain energy conservation (if applicable)
- [ ] LF line endings used (Unix-style `\n`)
- [ ] Documentation added for new public APIs
- [ ] Test coverage for critical paths
