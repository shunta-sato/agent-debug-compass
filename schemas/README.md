# Schema Registry

This directory contains the public Agent-facing contract schemas for Agent Debug Compass.

The schemas are intentionally small. A schema is added only when the corresponding CLI, MCP, fixture, or test output uses the contract.

Run:

```sh
scripts/contract/validate-contracts.py --schema-dir schemas --fixture-dir tests/golden
```
