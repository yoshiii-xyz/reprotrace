# Product brief

## Exact problem

Cargo can rebuild a package because observable build evidence changed across
command arguments, selected environment values, compiler state, source or
generated inputs, and Cargo's fingerprint state. That evidence is spread across
the build process and its output directories. A developer needs a deterministic
comparison that distinguishes an observation from a causal inference.

## Target technical users

The first users are Rust developers and CI maintainers who already know how to
run Cargo builds and need local or CI build evidence. Human adoption testing is
outside this program.

## Current alternatives

- Cargo's verbose logging and dep-info files expose low-level build evidence.
- `cargo metadata --format-version 1` exposes workspace and dependency data.
- Cargo JSON build messages expose compiler artifacts and build-script results.
- sccache reports cache statistics and can act as a compiler wrapper.
- Cargo's own FAQ explains common rebuild causes.

## Observed weakness in alternatives

These interfaces are useful individually, but they do not provide one
versioned, redacted, cross-run explanation that joins environment, compiler,
input, fingerprint, and cache evidence. Cargo's fingerprint module is an
internal implementation detail, and sccache is a cache rather than a Cargo
fingerprint explanation layer.

## Switching wedge

Reprotrace produces a small local JSON trace and a deterministic comparison for
two controlled builds. It preserves uncertainty when an internal or external
cause was not observed.

## MVP boundary

- Linux recorder for a requested command, with Cargo as the primary target.
- Process arguments and normalized working-directory evidence.
- Hashes or omissions for selected environment values.
- Compiler identity and bounded sysroot evidence.
- rustc invocation evidence through a temporary wrapper.
- dep-info input paths and bounded content digests.
- before and after snapshots of Cargo fingerprint files.
- cache provider identification with explicit unknown outcomes.
- `inspect` and deterministic `explain` commands.

## Non-goals

No daemon, web UI, hosted service, telemetry, cloud backend, general profiler,
build-system replacement, cache implementation, or perfect attribution claim.

## Evidence and inference

The source links and evidence grades are recorded in
[docs/research.md](research.md). The product records observed differences and
then labels any statement that connects a difference to a rebuild as an
inference.
