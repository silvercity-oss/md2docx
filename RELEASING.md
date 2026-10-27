# Releasing

Releases are cut from `main` by tagging; the `Release` workflow does the rest and stops for a
human before anything is published.

## Checklist

1. **Changelog.** Move the entries under a heading `## [x.y.z] - YYYY-MM-DD` in `CHANGELOG.md`.
2. **Version.** Set `version` in `Cargo.toml`, then `cargo update -w` so `Cargo.lock` follows.
3. **Local gate** (what CI will run again):
   `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets &&
   cargo test --doc && cargo package --locked`
4. **Pull request.** Open it against `main`, wait for every required check, get the review.
5. **Tag** the merge commit: `git tag -s vX.Y.Z -m "md2docx X.Y.Z" && git push origin vX.Y.Z`.
6. **Approve** the `crates-io` environment when the `Release` workflow pauses for it. It then
   verifies that the tag matches `Cargo.toml` and the changelog, publishes the crate, and
   attaches `md2docx-cli` binaries and a `SHA256SUMS` file to a GitHub release.

## Notes

* Publishing needs the `CARGO_REGISTRY_TOKEN` secret on the `crates-io` environment (scoped to
  this crate, publish-new/publish-update only). Nothing else holds a registry token.
* A published version cannot be changed. If a release is bad, `cargo yank` it and release a
  patch version.
* `cargo-semver-checks` compares the API against the latest tag in CI, so an accidental break in a
  minor or patch release fails the pull request.
* Every GitHub Action in `.github/workflows` is pinned to a full commit SHA with its version in a
  trailing comment (`uses: owner/action@<sha> # v5`), because a tag can be moved. Dependabot
  proposes the bumps weekly; review the upstream diff before merging one, especially for the
  release workflow. `dtolnay/rust-toolchain` is pinned at one SHA and picks the Rust version with
  its `toolchain:` input.
