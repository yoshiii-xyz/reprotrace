# Design

## Data model

The version 1 trace is stable JSON with these top-level groups:

- `command`, `workspace`, and `packages` identify the requested operation.
- `compiler` contains rustc version output and a bounded sysroot digest.
- `environment` contains SHA-256 digests for selected non-secret values.
- `omitted_environment` names selected secret-like values that were present
  but not retained.
- `inputs` contains normalized dep-info paths and bounded file digests.
- `fingerprints` contains the after-build fingerprint snapshot, while
  `fingerprints_before` contains the before-build snapshot.
- `rustc_invocations` contains normalized wrapper observations.
- `cache` contains provider detection and an explicit outcome state.
- `result` contains child status, exit code or signal, and elapsed time.
- `warnings` records incomplete or unavailable observations.

Unordered vectors are sorted before serialization. Elapsed times are volatile;
the explanation output does not include them.

## Core interfaces

`record` runs the requested command with the current binary in
`RUSTC_WRAPPER`. Cargo passes the actual rustc path as the wrapper's first
argument. If an existing wrapper was configured, reprotrace invokes it with the
same compiler path and arguments after logging the observation.

Workspace discovery uses `cargo metadata --format-version 1 --no-deps` and
falls back to the nearest manifest directory when metadata is unavailable.
The requested command is never rewritten to add Cargo flags.

## Input boundaries

Dep-info files are parsed as Makefile-like records. Escaped spaces and line
continuations are handled. Paths inside the workspace become relative paths.
Paths outside it become `<external>/<digest>` so the trace can compare stable
identity without retaining an absolute path.

Fingerprint files are discovered below the target directory only when a path
component is `.fingerprint`. Their bytes are hashed up to 16 MiB per file.
Sysroot hashing is bounded to 50,000 files and 8 MiB of file content.

## Failure behavior

Malformed traces fail inspection and comparison. A child command failure still
produces a trace and `record` returns the child's status. Missing or unreadable
fingerprint directories are represented in scan status and never become a
successful completeness claim. A wrapper log write failure is printed to the
child's standard error and the trace records the missing invocation evidence if
the log cannot be read.

## Resource limits

Directory scans stop at 50,000 entries. File hashes stop at 16 MiB, and the
sysroot digest stops at 8 MiB. These limits are encoded in the trace through
`complete`, `readable`, `truncated`, and warning fields.

## Portability boundary

The current implementation uses Linux process and exit-status behavior and is
validated on the current Linux host. The JSON model avoids platform-specific
claims, but Windows and macOS behavior is not promised by the MVP.
