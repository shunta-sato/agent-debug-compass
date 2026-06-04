# Schema Registry

This directory contains the public Agent-facing contract schemas for Agent Debug Compass.

The schemas are intentionally small. A schema is added only when the corresponding CLI, MCP, fixture, or test output uses the contract.

Run:

```sh
python3 -m pip install -r scripts/contract/requirements.txt
make contract
```

`make contract` validates static golden fixtures, generated CLI/MCP fixtures,
semantic invariants, sequence trace consistency, and the contract coverage
manifest. Validation uses a pinned Draft 2020-12 JSON Schema dependency and
allows only local refs inside `schemas/`.
