# reprotrace

Reprotrace explains observed Cargo rebuild and compiler-cache differences from local evidence.

Status: published 0.1.0 MVP

CI: [workflow](https://github.com/joshiii-xyz/reprotrace/actions/workflows/ci.yml)

## Install

Install the published crate with:

```bash
cargo install reprotrace --version 0.1.0
```

## Quick start

```bash
reprotrace record --output first.json -- cargo build -p PACKAGE
reprotrace inspect first.json
reprotrace explain first.json second.json
```

Run the second record command after the controlled change you want to compare.

## What it solves

Cargo's rebuild decision can depend on command arguments, selected environment
values, compiler identity, dep-info inputs, generated files, and fingerprint
state. Reprotrace records those observable inputs and compares two traces.

## How it works

`record` discovers the Cargo workspace with `cargo metadata --format-version 1 --no-deps`, runs the requested command, and temporarily installs the current
binary as `RUSTC_WRAPPER`. The wrapper records normalized rustc arguments and
chains an existing wrapper when one is configured. After the command exits,
reprotrace hashes dep-info inputs and bounded Cargo fingerprint files.

`explain` reports observed differences and labels the causal attribution as an
inference. Cargo's fingerprint implementation is an internal interface, so
unobserved causes remain explicit unknowns.

## Commands or library API

```text
reprotrace record [--output PATH] -- COMMAND [ARGS]
reprotrace inspect TRACE
reprotrace explain TRACE_A TRACE_B
reprotrace version
```

The Rust library exposes `record_command`, `read_trace`, `explain_traces`, and
the versioned `Trace` data types for local integrations.

## Output and exit codes

Trace files are versioned JSON. Paths inside the workspace are relative.
External paths are represented by a digest. Non-secret selected environment
values are represented by SHA-256 digests, and secret-like names are listed in
`omitted_environment` without their values.

Exit code 0 means the requested operation and recorded child command succeeded.
The `record` command returns the child exit code when the child fails. Exit
code 1 represents a recorded child failure without an exit code. Exit code 2
represents a CLI, trace, or recording error.

## Safety, privacy, and data handling

Reprotrace does not store inherited environment values by default. It does not
read Cargo credential files or network services as part of recording. Trace
files can still reveal project structure through relative paths and should be
treated as local build evidence. Temporary wrapper logs are deleted after they
are read. The output directory is created if needed.

## Limits and non-goals

The current MVP is Linux-first and observes Cargo through documented external
interfaces plus version-sensitive fingerprint files. It does not replace
Cargo, rustc, or sccache. It does not provide a daemon, web UI, telemetry
service, cloud backend, general profiler, or a proof that one changed item was
the sole Cargo cause. See [docs/limits.md](docs/limits.md).

## Testing and development

```bash
cargo fmt --all -- --check
cargo check --all-targets --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --locked
RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --locked
cargo package --locked
cargo audit
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the change workflow and
[docs/release.md](docs/release.md) for the bounded overhead audit.

## Release and support status

Version 0.1.0 is published on [crates.io](https://crates.io/crates/reprotrace/0.1.0)
and released on [GitHub](https://github.com/joshiii-xyz/reprotrace/releases/tag/v0.1.0).
It is a Linux-first MVP and makes no production-readiness claim.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before proposing a change.

## License

Reprotrace is released under the [MIT License](LICENSE).
