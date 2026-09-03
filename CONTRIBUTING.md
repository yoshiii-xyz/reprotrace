# Contributing

Keep changes narrow and tied to the product brief. Every parser or protocol
boundary needs normal, malformed, adversarial, and bounded-input coverage where
the boundary applies.

Before proposing a change:

1. State the problem and measurable success condition.
2. Read the relevant design and limits documents.
3. Make the smallest scoped edit.
4. Read back changed files.
5. Run format, check, Clippy, tests, docs, package, and audit checks as
   applicable.
6. Include exact command results and known limits in the review description.

Do not include credentials, private audit output, generated target files, or
unreproducible machine-specific evidence. Keep trace fixtures deterministic.

The repository owner reviews the diff and controls commits, pushes, tags, and
publication.
