# Release

The 0.1.0 MVP is not released. This document defines the dry-run and release
evidence required before publication.

## Dry run

Run from a clean checkout with the locked dependency graph:

```bash
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked
cargo package --locked
cargo audit
```

Run the bounded overhead harness against a fixture project:

```bash
./scripts/measure-overhead.sh target/debug/reprotrace /path/to/fixture
```

The harness uses a 60 second timeout for each build and prints the baseline
and traced wall times. It is evidence about this fixture and host only.

## Version consistency

Before a release, the version in `Cargo.toml`, `Cargo.lock`, the changelog,
release documentation, and registry metadata must agree. `cargo package --locked` is inspected before any publication.

## Publication order

```text
local dry run
review and commit
push main
publish crate if applicable
verify registry availability
push annotated tag
wait for hosted release workflow
verify all artifacts
public install and smoke audit
record evidence
```

The current workflows use immutable action commits. The release workflow
packages and builds the tagged revision; publication remains a deliberate
release-gate action until registry and artifact evidence is complete.

## Rollback

Do not overwrite a published crate version. If a published artifact is faulty,
follow the registry's documented yank or deprecation process, record the
decision in the changelog, and publish a corrected version only after a new
full gate.
