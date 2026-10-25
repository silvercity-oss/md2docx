# Test data

`commonmark-spec-examples.txt` holds the Markdown inputs of the examples in the
[CommonMark specification](https://spec.commonmark.org/) 0.31.2 (the input half
of each example only; one example per `<<<EXAMPLE>>>` line). They are © John
MacFarlane and contributors, licensed under
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/), and are used
here only to check that every input converts without error or corruption.

This directory is not part of the published crate (see `exclude` in
`Cargo.toml`); the tests that read it skip themselves when it is absent unless
`REQUIRE_SPEC_CORPUS` is set.
