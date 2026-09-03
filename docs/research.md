# Research

Research date: 2026-09-03

## Sources

### Primary documentation

- [Cargo external tools](https://doc.rust-lang.org/cargo/reference/external-tools.html)
  documents versioned `cargo metadata` JSON and line-delimited Cargo JSON
  messages. It states that `compiler-artifact` messages are emitted for each
  compilation step.
- [Cargo metadata](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html)
  documents `--format-version`, workspace members, resolved dependencies, and
  target directory fields.
- [Cargo build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html)
  documents target and build directories, dep-info files, and the use of
  `RUSTC_WRAPPER` for sccache.
- [Cargo configuration](https://doc.rust-lang.org/cargo/reference/config.html)
  documents `build.rustc-wrapper`, including the compiler path as the wrapper's
  first argument, and the `RUSTFLAGS` and target-directory configuration paths.
- [Cargo environment variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html)
  documents `RUSTC_WRAPPER`, `RUSTC_WORKSPACE_WRAPPER`, `RUSTFLAGS`, and
  `CARGO_TARGET_DIR`.
- [Cargo build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html)
  documents `rerun-if-changed`, `rerun-if-env-changed`, and the mtime-based
  behavior of those signals.
- [Cargo fingerprint internals](https://doc.rust-lang.org/stable/nightly-rustc/cargo/core/compiler/fingerprint/index.html)
  documents dep-info translation, environment comments, mtime checks, and the
  fact that fingerprint implementation details are internal.
- [rustc command-line arguments](https://doc.rust-lang.org/rustc/command-line-arguments.html)
  documents `--version`, `--verbose`, `--print`, and `--sysroot` as compiler
  identity probes.
- [sccache README](https://github.com/mozilla/sccache/blob/main/README.md)
  documents compiler-wrapper operation, local and remote cache modes, cache
  statistics, path normalization, logging, and Rust cache caveats.
- [sccache configuration](https://github.com/mozilla/sccache/blob/main/docs/Configuration.md)
  documents `SCCACHE_BASEDIRS`, logging controls, and bounded frame settings.

## Evidence grades

- Cargo metadata, external tools, build cache, configuration, environment, and
  build-script behavior: evidence-backed primary documentation.
- Cargo fingerprint semantics: evidence-backed documentation of current
  implementation behavior, with a principled portability warning because the
  module is internal.
- sccache wrapper and logging behavior: evidence-backed primary project
  documentation. Cache result attribution through arbitrary logs remains an
  implementation decision, not a guaranteed interface.
- The usefulness of a joined cross-run trace: principled product inference
  from the documented separation of these evidence sources. It is not a claim
  of human adoption or willingness to pay.

## Facts versus inference

Facts from the sources:

1. Cargo exposes versioned workspace metadata and JSON build messages.
2. Cargo passes the rustc path and arguments through a configured wrapper.
3. Cargo uses dep-info and internal fingerprint state to decide whether work
   is stale or fresh.
4. Cargo documents target and build directories separately.
5. sccache is a compiler wrapper and reports cache statistics, but it does not
   present itself as a Cargo fingerprint explanation API.

Inferences for this product:

1. Joining a temporary wrapper record with dep-info and fingerprint snapshots
   can explain many controlled local rebuild differences.
2. The tool must label an observed difference as a candidate cause rather than
   claim that it was the sole Cargo decision.
3. A redacted, relative-path trace is safer for local comparison than storing
   inherited environment values or absolute paths by default.

## Current local signals

The implementation host reports Rust and Cargo 1.98.0, cargo-audit 0.22.2,
cargo-fuzz 0.13.2, and an authenticated GitHub account. These are environment
facts for this research pass, not compatibility promises for users.

## Rejected alternatives

- Parse only Cargo's human verbose output: it is useful for diagnosis but does
  not provide the versioned joined trace model needed here.
- Depend only on `cargo metadata`: it describes package structure and targets,
  not the complete command, environment, source digest, or compiler wrapper
  observations for a build.
- Treat sccache statistics as the cause: sccache identifies cache outcomes, but
  Cargo may still decide to rebuild for evidence outside the cache log.
- Link directly to Cargo's internal fingerprint Rust module: it would couple
  the product to a private implementation and undermine the explicit unknown
  state when the layout changes.
- Replace Cargo or implement a new cache: that conflicts with the stated wedge
  and expands the product beyond an explanation layer.

## Decision

Implement a Rust CLI and library that uses documented Cargo integration points,
records a bounded set of local evidence, normalizes paths, hashes or omits
selected environment values, and compares two traces deterministically. Keep
fingerprint files as evidence snapshots rather than a stable parser contract.
Do not add a daemon, web UI, telemetry, hosted service, or general profiler.
