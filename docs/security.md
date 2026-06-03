# Security

Agent Debug Compass is designed for Agent-first debugging without exposing broad target control.

## Principles

- No arbitrary shell tool is available through MCP.
- Rootless operation is the default.
- Managed MCP listeners are explicit opt-in.
- Token authentication is required for managed MCP.
- Mutual TLS can be enabled for managed MCP.
- Raw artifacts stay behind bounded refs.
- Partial success and failures are recorded in `data_quality`.

## Target Mode

`adc-mcp --target-mode` exposes target-local observation tools only. Controller fleet/discovery tools are not available in target mode.

## Managed MCP

Managed MCP is intended for trusted lab LANs where targets explicitly run a listener. The listener is not started by default. Use `scripts/install/install-managed-mcp-user-service.sh` for rootless supervised target setup.

## Public Tree Hygiene

Before pushing a public source tree:

```bash
scripts/package/create-public-tree.sh --output /tmp/agent-debug-compass-public --force
```

The export step runs `scripts/security/check-public-tree.sh` to reject private agent tooling, generated artifacts, old product names, local target aliases, private IP markers, and obvious private-key markers.
