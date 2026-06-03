# Supply-chain Audit Notes

This directory is reserved for future `cargo-vet` configuration.

Current policy:

- `cargo-deny` and `cargo-audit` are the active dependency security gates.
- `cargo-vet` is installed by `scripts/install/install-rust-security-tools.sh`
  but is not a hard gate yet.
- Before enabling `cargo vet` in `make security-check`, initialize vet metadata
  and review every exemption/import so the gate does not become a blanket allow.
