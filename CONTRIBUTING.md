# Contributing

Agent Debug Compass is developed test-first.

Before submitting changes:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -q --workspace
scripts/demo/tests/run-sensor-gateway-demo-test.sh
scripts/e2e/run-e2e.sh
PATH="$HOME/.cargo/bin:$PATH" scripts/security/run-rust-security-checks.sh
```

Keep Agent-facing output cause-neutral, bounded, and ref-first. Do not add arbitrary shell tools, old compatibility surfaces, or root-resident assumptions.

For public-tree checks:

```bash
scripts/package/create-public-tree.sh --output /tmp/agent-debug-compass-public --force
```
