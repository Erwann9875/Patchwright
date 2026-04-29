# Contributing to Patchwright

Patchwright uses a small, strict GitHub workflow so commits, pull requests, and release notes stay readable as the project grows.

## Commit Messages

Use Conventional Commits:

```text
<type>(<scope>): <summary>
```

Examples:

```text
feat(core): add agent state machine
fix(exec-local): preserve worktree cleanup on command timeout
perf(cli): avoid config parsing for version command
docs(architecture): add patchwright architecture design
test(lang-rust): cover cargo verifier planning
```

Allowed initial types:

- `feat`: user-visible feature or capability
- `fix`: bug fix
- `perf`: performance improvement
- `refactor`: internal change with no intended behavior change
- `test`: test-only change
- `docs`: documentation-only change
- `ci`: CI workflow change
- `build`: build system or dependency change
- `chore`: maintenance that does not fit another type
- `revert`: explicit revert

Scopes should be short and should map to crates or owned areas:

```text
core
cli
exec-local
model-openai
lang-rust
index
verify
architecture
github
ci
```

Write summaries in the imperative mood, lowercase after the colon unless naming a proper noun, and without a trailing period.

Use `!` for breaking changes and explain the break in the commit body:

```text
feat(core)!: change action execution contract

BREAKING CHANGE: ExecutionBackend::run now requires a Policy argument.
```

## Pull Requests

Pull request titles use the same format as commits:

```text
feat(lang-rust): add cargo verifier planning
```

Each pull request should prefer one logical change. If a PR is squash-merged, the PR title should usually become the final commit message.

PR descriptions should include:

- summary of the change
- verification commands and results
- performance impact when relevant
- risk or rollback notes when relevant
- linked issue when one exists

## Verification

Before opening or merging a PR, run the relevant verification commands and include the results in the PR description. Do not describe code as passing, fixed, or complete unless the command output supports that claim.

