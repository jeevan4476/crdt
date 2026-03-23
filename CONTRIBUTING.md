# Contributing

## Local Development

This repository pins Rust with `rust-toolchain.toml`, so contributors should get a consistent compiler version automatically.

Recommended local checks before opening a pull request:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

## Pull Requests

Use the PR template in `.github/PULL_REQUEST_TEMPLATE.md` and keep each PR scoped to one logical change when possible.

Good PRs for this project usually include:

- the CRDT or subsystem being changed
- the merge semantics or invariant affected
- the tests that prove the change is safe
- any roadmap item the PR completes or advances

## Recommended Branch Protection

In GitHub, configure branch protection for `main` with these settings:

- require a pull request before merging
- require passing status checks before merging
- require branches to be up to date before merging

Recommended required status checks:

- `Format Check`
- `Clippy`
- `Test Suite`

## Release Process

The release workflow creates GitHub releases for version tags matching `v*`, but only if the tagged commit is reachable from `main`.

Typical release flow:

1. Merge the release-ready changes into `main`.
2. Create a version tag, for example `v0.1.0`.
3. Push the tag to GitHub.

Example:

```bash
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

The workflow then:

- verifies the tag points to a commit on `main`
- runs the test suite
- creates a GitHub release with generated release notes
