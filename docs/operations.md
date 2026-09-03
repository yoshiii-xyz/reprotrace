# Operations

## Local installation

From the repository root:

```bash
cargo install --path .
```

For an uninstalled binary, use `cargo run --` in the examples.

## Safe execution

Use a separate output path for each controlled build:

```bash
reprotrace record --output .reprotrace/first.json -- cargo build -p app
reprotrace record --output .reprotrace/second.json -- cargo build -p app
reprotrace explain .reprotrace/first.json .reprotrace/second.json
```

The requested command runs with its normal working directory and arguments.
Reprotrace adds only the temporary wrapper environment needed to observe rustc.
If a wrapper was already configured, it is chained rather than discarded.

## Logging and retention

The final JSON file is retained at the requested path. Temporary wrapper logs
are created beside it, read after the child exits, and deleted. The trace keeps
relative paths, content digests, compiler identity, selected environment
digests, and any explicit warnings. Keep traces private when the source tree or
command arguments are sensitive.

Reprotrace does not read or write Cargo credential files. Do not put tokens or
credentials in a trace fixture, repository file, log, or commit message.

## Cleanup

The default output directory is `.reprotrace`. It can be removed after the
evidence is archived according to local policy. Build artifacts remain under
Cargo's normal target directory.

## Troubleshooting

- If no rustc invocations appear, inspect `warnings`, the child result, and the
  target directory location. An unchanged Cargo build can legitimately have no
  wrapper calls.
- If fingerprint status is `missing`, verify that the command used the target
  directory discovered by Cargo metadata.
- If paths are represented as `<external>`, the input is outside the workspace
  root or the workspace root could not be discovered.
- If a trace is rejected, run `reprotrace inspect TRACE` and preserve the exact
  error without editing the trace by hand.
