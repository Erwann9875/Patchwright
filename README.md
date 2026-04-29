# Patchwright

Patchwright is a local-first Rust foundation for a software-engineering coding agent. The model proposes candidate actions and patches; compilers, tests, linters, type checkers, benchmarks, policies, and reviewers decide what is true.

The first build focuses on a language-agnostic core, a Rust adapter, local execution, OpenAI-compatible model access, and strict verification.

`solve` and `verify` execute inside temporary git worktrees so rejected attempts and build artifacts do not mutate the source checkout. The source repo must be clean before sandboxed execution.

## Development

Run the full local verification suite:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p patchwright-cli -- bench startup
```

Run basic commands:

```bash
cargo run -p patchwright-cli -- --version
cargo run -p patchwright-cli -- status
cargo run -p patchwright-cli -- config check
cargo run -p patchwright-cli -- verify --repo .
cargo run -p patchwright-cli -- solve --repo . --task "summarize"
```
