# Rust Dependency and Supply-chain Checks

`make verify` remains the fast local gate: format, Clippy, `cargo check`, tests,
and script smoke tests. The stronger dependency/security gate is separate so a
minimal Raspberry Pi development environment does not require Cargo-installed
security tools.

## Install Tools

Review the pinned tool plan:

```bash
scripts/install/install-rust-security-tools.sh --dry-run
```

Install explicitly:

```bash
scripts/install/install-rust-security-tools.sh --apply
```

The installer uses `cargo install --locked --version` and pins:

- `cargo-deny 0.18.3`
- `cargo-audit 0.22.1`
- `cargo-machete 0.8.0`
- `cargo-vet 0.10.2`

`cargo-deny` is pinned below latest because the current workspace toolchain is
Rust 1.85 and `cargo-deny 0.19.x` requires Rust 1.88.
`cargo-machete` is pinned to 0.8.0 because 0.9.x pulls Cargo metadata
dependencies that require Rust 1.86+.

`cargo-geiger 0.13.0` is optional:

```bash
scripts/install/install-rust-security-tools.sh --apply --with-geiger
```

It currently pulls native OpenSSL build dependencies during installation, so it
is not part of the default target setup. On Debian/Raspberry Pi OS, install
`libssl-dev` first if `openssl-sys` cannot find OpenSSL.

## Run Checks

```bash
make security-check
```

This runs:

- `cargo deny check --config deny.toml bans licenses sources`
- `cargo audit`
- `cargo machete`
- `cargo geiger --all-targets --manifest-path <crate>/Cargo.toml` for each
  workspace crate, only when `cargo-geiger` is installed

`cargo-geiger` writes a report to `reports/security/cargo-geiger.txt`.
Transitive dependency `unsafe` is visible in the report but is not a hard fail.
`cargo-geiger` may return non-zero for generated-file scan warnings even when
it produced a usable report; the wrapper records those warnings in the report
instead of failing the security gate.
Workspace crate `unsafe` is blocked by the workspace `unsafe_code = "forbid"`
lint.

## Release Gate

```bash
make release-gate
```

This composes:

- `make verify`
- `make security-check`
- `make e2e-local`
- `make package-release`

## Policy

`deny.toml` limits checks to Linux target triples for this Raspberry Pi first
workspace, allows the current license set, rejects unknown registries and Git
dependencies, and bans native OpenSSL/SSH dependency surfaces by default.
Advisory scanning is handled by `cargo audit` because the Rust 1.85-compatible
`cargo-deny 0.18.3` cannot parse current RustSec CVSS 4.0 advisories.

`cargo-vet` is intentionally not a hard gate yet. Introduce it as a follow-up
once the project is ready to maintain audit imports/exemptions for the full
transitive dependency graph.
