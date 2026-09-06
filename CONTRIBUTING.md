# Contributing

## Set up

```bash
git clone https://github.com/goban-app/katago-server
cd katago-server
make test      # needs Rust (see rust-version in Cargo.toml) and python3
make lint      # rustfmt, clippy (pedantic, -D warnings), cargo-deny if installed
```

A real KataGo is only needed for manual runs; see `docs/development.md`.

## Making changes

- Keep the `/api/v1` contract backward compatible. Adding optional fields is fine; renaming or removing is a major version.
- Every behaviour change needs a test: unit tests next to the code, or an end-to-end case in `tests/api.rs` against the fake KataGo.
- Validation rules live in `src/api/validate.rs`; error types in `src/api/problem.rs`. New problem slugs need a section in `docs/problems.md`.
- Update `docs/` when the API, configuration or deployment changes, and add a line under `## [Unreleased]` in `CHANGELOG.md`.

## Commits and pull requests

Commit messages use conventional prefixes: `feat:`, `fix:`, `perf:`, `docs:`,
`chore:`, `ci:`, `test:`, `refactor:`. Keep pull requests focused; the template
lists the checklist. CI must be green.

## Releases

Maintainers release by bumping `Cargo.toml` and `charts/katago-server/Chart.yaml`
together; see `RELEASING.md`.
