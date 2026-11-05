# Contributing to Kandang

Thanks for helping. Kandang is a Linux desktop tool written in Rust.

## Setup

```sh
cargo build
cargo test
cargo fmt
```

Run `cargo fmt` before committing. CI rejects unformatted code.

## Terms

Use the words in [GLOSSARY.md](GLOSSARY.md). In particular, the site's
readable name is the **Project Name**; the Cloudflare project is a
**Pages project**.

## Design decisions

Decisions that are hard to reverse live in [docs/adr/](docs/adr/). If your
change contradicts an accepted ADR, write a new ADR that supersedes it in the
same PR. Don't edit the old one's decision.

## Pull requests

1. Work on a branch; don't push to `main`.
2. Keep one logical change per PR.
3. Add or update tests for behavior you change.
4. Make sure `cargo fmt --check` and `cargo test` pass locally.
5. Describe what changed and why in the PR body.

Dashboard changes: the UI is not covered by automated tests. Describe how you
checked it in the PR.

## Reporting bugs

Open an issue with your Linux distribution, desktop environment, the command
you ran, and the output of `kandang doctor`.

For security problems, do not open a public issue. See [SECURITY.md](SECURITY.md).
