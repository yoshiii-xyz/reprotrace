# Security policy

Reprotrace is local inspection software. It does not need registry credentials
to record a build and does not read Cargo credential files. Trace output can
contain relative project paths, compiler identity, command arguments after
redaction, and content digests. Treat traces as potentially sensitive local
artifacts.

Do not submit credentials, private keys, tokens, credential files, or private
audit output in an issue or pull request. Report a security defect privately to
the repository owner with a minimal reproducer that contains no secret data.

The project supports only the platforms and evidence boundaries listed in
\`docs/limits.md\`. A missing or bounded observation is a limitation, not a
security guarantee.
