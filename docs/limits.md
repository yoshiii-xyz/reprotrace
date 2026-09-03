# Limits

This file describes behavior in the 0.1.0 MVP.

- Only the current Linux host has been tested.
- Windows, macOS, WSL, NFS, overlay-specific behavior, network filesystems,
  containers, and remote execution were not tested.
- Cargo fingerprint files are internal and their layout can change. The trace
  records bounded file evidence and does not parse a private Cargo decision API.
- Dep-info is the input source. Inputs that Cargo does not place in dep-info,
  or that are generated through an unobserved external process, may be absent.
- A file larger than 16 MiB receives a prefix digest with `complete: false`.
- Sysroot hashing is bounded. `sysroot_digest_complete: false` means the
  recorded digest does not cover the entire sysroot.
- Directory scans stop at 50,000 entries and report `truncated`.
- An unreadable or missing fingerprint directory is reported, not treated as a
  complete empty directory.
- sccache provider detection can identify a configured wrapper or local Cargo
  configuration mention. Cache hit and miss outcomes remain `unknown` unless
  suitable log evidence is supplied to a future adapter.
- No human usability, adoption, hardware, remote filesystem, or cross-platform
  validation is claimed.
