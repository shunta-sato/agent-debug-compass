# Security Policy

## Supported Scope

Security reports should focus on Agent Debug Compass source, release scripts, MCP surfaces, managed MCP listener behavior, install scripts, and release bundles.

## Security Posture

- Agent-facing MCP tools are bounded `obs.*` operations.
- No arbitrary shell tool is exposed.
- Managed MCP listeners are default-off and require explicit token configuration.
- Mutual TLS is optional but supported for managed MCP.
- Rootless operation is the default path.
- Missing, denied, truncated, throttled, or failed evidence is reported through `data_quality`.

## Reporting

For a public repository, open a private security advisory when available. If advisories are not enabled, open a minimal issue that says a security report is available without including exploit details.

Do not include secrets, private keys, tokens, or live target identifiers in public reports.
