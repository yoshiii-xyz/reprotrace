# Local operating contract

Reprotrace is a Linux-first Rust CLI and library for explaining observed Cargo
rebuild evidence. The product brief, design, limits, research, source code,
tests, and CI workflows are authoritative.

## Verified commands

- Build: `cargo build --locked`
- Test: `cargo test --all-targets --locked`
- Format: `cargo fmt --all -- --check`
- Lint: `cargo clippy --all-targets --all-features --locked -- -D warnings`
- Docs: `RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked`
- Package: `cargo package --locked`
- Audit: `cargo audit`
- CLI: `cargo run -- version`

## Scope

Keep changes within the Linux recorder and two-trace explanation MVP. Do not
add a frontend, hosted service, telemetry, cloud backend, daemon, general
profiler, build system, or replacement for Cargo, rustc, or sccache.

## Operating loop

1. Plan the change and define a testable success condition.
2. Make only the scoped edit.
3. Read back every changed file.
4. Run the smallest relevant check and then the full gate at milestones.
5. Record exact results and evidence paths.
6. Review `git diff` before committing or pushing.

## Safety and release

Read and hash sensitive data only as described in `docs/design.md`. Never store
credentials, tokens, private keys, registry login files, or secret values in
tracked files, fixtures, logs, or commit messages. Follow `docs/release.md` and
the portfolio release checklist before publishing.
