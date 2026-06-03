# adc-targetd target setup guide

This guide installs the current MVP on Raspberry Pi-class Linux targets.

Current validation scope:

- Verified: Raspberry Pi 5 local/controller install and smoke.
- Verified: Raspberry Pi 4 same-LAN target MCP/fleet endpoint.
- Expected base OS shape: Linux `aarch64` userspace with systemd.
- Not yet verified: Jetson, QCOM/Snapdragon, x86 edge boxes, non-Linux targets, or Jetson-specific GPU/power/thermal collectors.

Treat other Linux boards as experimental until they have their own smoke evidence. Missing board capabilities should be recorded as `data_quality` gaps, but successful Raspberry Pi smoke does not imply Jetson/QCOM support.

## Preconditions

- Linux with systemd.
- Raspberry Pi 5 or Raspberry Pi 4 for the currently verified path.
- Rust toolchain when building on target, or a release bundle built elsewhere.
- `git`, `pkg-config`, `build-essential`, and Linux kernel headers when building the optional KO.
- No Node/npm tooling is required.

Target-privileged features are optional. `adc-targetd` must still run when tracefs, perf, kernel headers, thermal zones, or PCIe/RP1 visibility are unavailable. Missing capabilities are recorded in `data_quality`.

## Build from source

```bash
make verify
make build-release
```

The release binaries are:

- `target/release/adc-targetd`
- `target/release/adc`
- `target/release/adc-mcp`
- `target/release/adc-priv-helper`

## Create a release bundle

```bash
scripts/package/build-release-bundle.sh --force
```

The bundle is written to `dist/` with a `.tar.gz` archive and `.sha256` checksum.

## Install binaries

```bash
sudo install -m 0755 target/release/adc-targetd /usr/local/bin/adc-targetd
sudo install -m 0755 target/release/adc /usr/local/bin/adc
sudo install -m 0755 target/release/adc-mcp /usr/local/bin/adc-mcp
sudo install -m 0755 target/release/adc-priv-helper /usr/local/bin/adc-priv-helper
```

Create a dedicated user and state directory:

```bash
sudo useradd --system --home /var/lib/agent-debug-compass --shell /usr/sbin/nologin adc-targetd || true
sudo install -d -o adc-targetd -g adc-targetd -m 0750 /var/lib/agent-debug-compass
```

## Install systemd service

```bash
sudo install -m 0644 packaging/systemd/adc-targetd.service /etc/systemd/system/adc-targetd.service
sudo systemctl daemon-reload
sudo systemctl enable --now adc-targetd.service
systemctl status adc-targetd.service
```

The service runs as `adc-targetd` with `ADC_HOME=/var/lib/agent-debug-compass`.

Install sample profiles when using the release bundle:

```bash
sudo install -d -m 0755 /etc/adc-targetd/profiles
sudo install -m 0644 profiles/pi5_basic.yaml /etc/adc-targetd/profiles/pi5_basic.yaml
```

Use `ADC_PROFILE_DIR=/etc/adc-targetd/profiles` when running bounded daemon or capture workflows.

## Smoke checks

```bash
ADC_HOME=/var/lib/agent-debug-compass adc status
ADC_HOME=/var/lib/agent-debug-compass adc doctor
ADC_HOME=/var/lib/agent-debug-compass adc capabilities
ADC_HOME=/var/lib/agent-debug-compass adc snapshot --run-id R-SMOKE-001
ADC_HOME=/var/lib/agent-debug-compass adc observe --run-id R-OBSERVE-30S --duration-sec 30 --interval-ms 500
ADC_HOME=/var/lib/agent-debug-compass adc agent-context --run-id R-OBSERVE-30S
ADC_HOME=/var/lib/agent-debug-compass adc agent-context --run-id R-OBSERVE-30S --format otlp-json
ADC_HOME=/var/lib/agent-debug-compass adc agent-context --run-id R-OBSERVE-30S --format perfetto-json
ADC_HOME=/var/lib/agent-debug-compass adc target capture --target local --run-id R-CAPTURE-10S --duration-sec 10 --interval-ms 100
ADC_HOME=/var/lib/agent-debug-compass adc evidence get --run-id R-SMOKE-001
ADC_HOME=/var/lib/agent-debug-compass adc compare --before R-SMOKE-001 --after R-SMOKE-001
```

On Raspberry Pi 5, release-binary smoke and overhead measurement can be run without root:

```bash
scripts/e2e/target/run-pi5-release-smoke.sh --binary-dir ./bin --duration-sec 30
```

The runner records `PI5-SMOKE-*` assertion reports plus the run `overhead_report.json` and `/usr/bin/time -v` output when available.

## Rootless Target MCP Bootstrap

For a same-network target reachable by SSH, install the target MCP endpoint into the login user's home without sudo and print an inventory stanza:

```bash
scripts/install/install-target-mcp-binaries.sh \
  --host edge-pi-a.local \
  --target-id edge-pi-a \
  --binary-dir ./bin \
  --result-root /tmp/adc-target-bootstrap \
  > /tmp/adc-targets.yaml
```

The script copies `adc-mcp` to `~/.local/bin`, validates it through MCP-over-SSH fleet preflight, and writes `bootstrap_report.json` plus `fleet_preflight.json` when `--result-root` is set. It requires non-interactive SSH; it does not prompt for or store passwords.

## Same-network fleet smoke

Discovery is intentionally conservative: it filters the local neighbor table to a `/24` or narrower IPv4 CIDR and returns bounded target candidates. It does not scan broad networks or expose raw neighbor dumps.

```bash
ADC_HOME=/var/lib/agent-debug-compass adc fleet discover --cidr 192.0.2.0/24
```

Create an explicit inventory from candidates you trust. Remote targets must already have `adc-mcp` installed in `PATH` for the login user, or set `mcp_server_path` to a fixed executable path such as a user-local install. The transport starts fixed `adc-mcp --target-mode` over SSH stdio and calls target-local `obs.*` tools; it is not an arbitrary shell interface.

```bash
cat > /tmp/adc-targets.yaml <<'YAML'
targets:
  - id: local-pi5
    transport: local
  - id: edge-pi-a
    transport: mcp_stdio_over_ssh
    host: edge-pi-a.local
    mcp_server_path: /home/pi/.local/bin/adc-mcp
YAML

ADC_HOME=/var/lib/agent-debug-compass adc fleet preflight --inventory /tmp/adc-targets.yaml
ADC_HOME=/var/lib/agent-debug-compass adc fleet snapshot --inventory /tmp/adc-targets.yaml --fleet-run-id F-SMOKE-SNAPSHOT
ADC_HOME=/var/lib/agent-debug-compass adc fleet observe --inventory /tmp/adc-targets.yaml --fleet-run-id F-SMOKE-CAPTURE --duration-sec 5 --interval-ms 100
ADC_HOME=/var/lib/agent-debug-compass adc fleet evidence --fleet-run-id F-SMOKE-CAPTURE
ADC_HOME=/var/lib/agent-debug-compass adc agent-context --fleet-run-id F-SMOKE-CAPTURE
```

To avoid passing the same inventory path on every run, enroll trusted targets into the rootless managed fleet registry and use selectors:

```bash
ADC_HOME=/var/lib/agent-debug-compass adc fleet init
ADC_HOME=/var/lib/agent-debug-compass adc fleet invite --target-id-hint edge-pi-a --ttl-sec 600
ADC_HOME=/var/lib/agent-debug-compass adc fleet enroll --target-id local-pi5 --transport local --tag edge
ADC_HOME=/var/lib/agent-debug-compass adc fleet enroll \
  --target-id edge-pi-a \
  --transport mcp_stdio_over_ssh \
  --host edge-pi-a.local \
  --mcp-server-path /home/pi/.local/bin/adc-mcp \
  --tag edge
ADC_HOME=/var/lib/agent-debug-compass adc fleet targets
ADC_HOME=/var/lib/agent-debug-compass adc fleet preflight --selector tag=edge
ADC_HOME=/var/lib/agent-debug-compass adc fleet snapshot --selector tag=edge --fleet-run-id F-EDGE-SNAPSHOT
ADC_HOME=/var/lib/agent-debug-compass adc fleet observe --selector tag=edge --fleet-run-id F-EDGE-CAPTURE --duration-sec 5 --interval-ms 100
```

Selectors are `all`, `enrolled`, `target=<id>`, `tag=<tag>`, and `transport=<transport>`. The registry is stored under `ADC_HOME/fleet/targets.json`; join-code invite records keep only hashed join codes at rest. Current execution supports `local`, `mcp_stdio_over_ssh`, and authenticated `managed_mcp` targets.

For a no-SSH managed MCP transport, start an explicit target-mode listener on the target. The listener is off by default, requires a bearer token file, and exposes only target-local `obs.*` tools.

The preferred path is the guarded provisioner. It uses SSH only to copy fixed files and install a rootless user service; steady-state observation uses `managed_mcp` with bearer token plus mTLS:

```bash
scripts/install/provision-managed-mcp-target.sh \
  --ssh-host edge-pi-a \
  --managed-host edge-pi-a.local \
  --target-id edge-pi-a \
  --binary-dir ./bin \
  --listen 0.0.0.0:39245 \
  --managed-port 39245 \
  --tag edge \
  --result-root /tmp/adc-managed-provision-edge-pi-a

ADC_HOME=/var/lib/agent-debug-compass adc fleet snapshot \
  --selector target=edge-pi-a \
  --fleet-run-id F-MANAGED-MCP-SMOKE
```

Use `--dry-run` first when reviewing a new target. If the SSH alias is not TCP-resolvable, set `--ssh-host example-target` and `--managed-host <target-ip-or-dns>`. The script requires non-interactive SSH and does not handle passwords. For stricter host trust, use `--ssh-strict-host-key-checking yes --ssh-known-hosts-file ./known_hosts`; when `ssh-keyscan` and `ssh-keygen` are available, `provision_report.json` records a best-effort SSH host-key fingerprint. If the SSH alias itself cannot be scanned, the provisioner falls back to `--managed-host`; if neither can be scanned it records an explicit unavailable reason.

The lower-level enrollment kit path remains useful when you want to copy files yourself. It creates controller-side registry material plus target-side token and mTLS files without hand-writing secret flags:

```bash
scripts/install/create-managed-mcp-enrollment-kit.sh \
  --kit-dir ./edge-pi-a-kit \
  --target-id edge-pi-a \
  --host edge-pi-a.local \
  --port 39245 \
  --tag edge

ADC_HOME=/var/lib/agent-debug-compass adc fleet enroll-kit \
  --kit ./edge-pi-a-kit/controller/enrollment-kit.json
```

Copy `./edge-pi-a-kit/target/*` to a private directory on the target and start the listener with those files:

```bash
ADC_HOME=~/.local/share/agent-debug-compass \
  adc-mcp --target-mode \
    --managed-listen 0.0.0.0:39245 \
    --managed-token-file ./managed.token \
    --managed-tls-server-cert ./server.pem \
    --managed-tls-server-key ./server.key \
    --managed-tls-client-ca ./controller-ca.pem
```

For manual bearer-token-only trusted LAN setup, copy the token into a private controller file and enroll the target:

```bash
install -d -m 0700 ~/.local/share/agent-debug-compass/controller-secrets
install -m 0600 /path/from/target/managed-mcp.token ~/.local/share/agent-debug-compass/controller-secrets/edge-pi-a.token
ADC_HOME=/var/lib/agent-debug-compass adc fleet enroll \
  --target-id edge-pi-a \
  --transport managed_mcp \
  --host edge-pi-a.local \
  --port 39245 \
  --auth-token-file ~/.local/share/agent-debug-compass/controller-secrets/edge-pi-a.token \
  --tag edge
ADC_HOME=/var/lib/agent-debug-compass adc fleet preflight --selector target=edge-pi-a
ADC_HOME=/var/lib/agent-debug-compass adc fleet snapshot --selector target=edge-pi-a --fleet-run-id F-MANAGED-MCP-SMOKE
```

The enrollment kit uses bearer token plus mutual TLS. Manual bearer-token-only setup should stay on trusted LANs or behind host firewall rules.
The `--host` value must be a DNS name or IP address reachable by TCP from the controller; SSH config aliases are not resolved by this transport.

To rotate the token, replace the token file on the target and update the controller-side token file. The managed listener reloads the token file on each request, so a listener restart is not required. Keep file permissions private on both sides.

For a rootless supervised target listener, install a systemd user service on the target:

```bash
scripts/install/install-managed-mcp-user-service.sh \
  --listen 0.0.0.0:39245 \
  --generate-token
systemctl --user status adc-mcp-managed.service
```

The installer writes `~/.config/systemd/user/adc-mcp-managed.service`, creates a private token file when requested, and enables the service with `Restart=on-failure`. It does not require sudo.

For mutual TLS, provide a server certificate/key and the CA used to verify controller client certificates:

```bash
scripts/install/install-managed-mcp-user-service.sh \
  --listen 0.0.0.0:39245 \
  --token-file ~/.local/share/agent-debug-compass/managed-mcp.token \
  --tls-server-cert ~/.local/share/agent-debug-compass/tls/server.pem \
  --tls-server-key ~/.local/share/agent-debug-compass/tls/server.key \
  --tls-client-ca ~/.local/share/agent-debug-compass/tls/controller-ca.pem
```

Enroll the controller with the matching trust material:

```bash
ADC_HOME=/var/lib/agent-debug-compass adc fleet enroll \
  --target-id edge-pi-a \
  --transport managed_mcp \
  --host edge-pi-a.local \
  --port 39245 \
  --auth-token-file ~/.local/share/agent-debug-compass/controller-secrets/edge-pi-a.token \
  --tls-ca-file ~/.local/share/agent-debug-compass/controller-secrets/target-ca.pem \
  --tls-client-cert-file ~/.local/share/agent-debug-compass/controller-secrets/controller.pem \
  --tls-client-key-file ~/.local/share/agent-debug-compass/controller-secrets/controller.key \
  --tls-server-name edge-pi-a.local \
  --tag edge
```

For a reusable release smoke against one real target MCP endpoint, run:

```bash
scripts/e2e/target/run-target-mcp-fleet-smoke.sh \
  --inventory /tmp/adc-targets.yaml \
  --binary-dir ./bin \
  --duration-sec 30 \
  --result-root /tmp/adc-target-mcp-fleet-smoke
```

The runner infers expected captured targets from the inventory; pass `--expected-targets N` when the smoke intentionally covers a subset. It records `FLEET-SMOKE-*` assertion reports, preflight/snapshot/capture JSON, `fleet_evidence.yaml`, fleet Agent context JSON/Markdown, and `summary.json`.

If a target is unreachable, denied by SSH, or fails during collection, the fleet command still returns partial evidence when other targets succeed. Inspect `data_quality`, `target_matrix`, and target-scoped `evidence_ref` entries in `fleet_evidence.yaml`.

Run the local E2E harness:

```bash
scripts/e2e/run-e2e.sh
```

The harness writes assertion reports under `e2e-results/local`. CPU, memory, kmsg mock, and loopback network workloads run through bounded daemon mode locally. Target-privileged ftrace/perf and runtime KO checks are documented skips unless the target is prepared for those operations.

If privileged target smoke was already run, import its assertion reports explicitly:

```bash
ADC_E2E_IMPORT_TARGET_SMOKE=1 scripts/e2e/run-e2e.sh
```

## Limited sudoers for privileged smoke

To let an automation session run only the fixed privileged smoke entrypoint without a sudo password, install the limited sudoers rule:

```bash
scripts/install/install-target-smoke-sudoers.sh --dry-run
sudo scripts/install/install-target-smoke-sudoers.sh --apply
sudo -n /usr/local/libexec/agent-debug-compass/adc-target-smoke-root --allow-privileged-smoke self-check
```

This rule grants `NOPASSWD` only for `/usr/local/libexec/agent-debug-compass/adc-target-smoke-root`. It does not grant arbitrary shell, build tools, or repository script execution. The installed root entrypoint is copied to a root-owned path and still requires `--allow-privileged-smoke`.

After installation, privileged target smoke can be run without an interactive password:

```bash
scripts/e2e/target/run-privileged-smoke.sh capability-report
scripts/e2e/target/run-privileged-smoke.sh ftrace-perf-smoke --duration-sec 2
scripts/e2e/target/run-privileged-smoke.sh ko-runtime-smoke
scripts/e2e/target/run-privileged-smoke.sh safe-kprobe-smoke --allow-kprobe-smoke --symbol do_sys_openat2
```

Reports are written under `/var/tmp/agent-debug-compass-target-smoke/<user>/`.

If `perf` is not installed, install only the target userspace package after reviewing the dry-run:

```bash
scripts/install/install-target-perf.sh --dry-run
sudo scripts/install/install-target-perf.sh --apply
scripts/e2e/target/run-privileged-smoke.sh ftrace-perf-smoke --duration-sec 2
```

For target overhead checks against release binaries, run:

```bash
cargo build --workspace --release
scripts/e2e/target/run-perf.sh --duration-sec 5
```

The perf runner writes `PERF-001` through `PERF-004` assertion reports under `/var/tmp/agent-debug-compass-perf/<user>/` by default and records documented skips when release binaries or privileged ftrace/perf access are unavailable.

For KO runtime smoke, build the KO as the normal user and then install only that module into the root-owned smoke path:

```bash
make ko-selftest
scripts/install/install-target-smoke-ko.sh --dry-run
sudo scripts/install/install-target-smoke-ko.sh --apply
scripts/e2e/target/run-privileged-smoke.sh ko-runtime-smoke
```

The KO installer validates `modinfo -F name` before copying and installs the module as root-owned, non-writable by group/other.

To remove the rule:

```bash
sudo scripts/install/install-target-smoke-ko.sh --remove
sudo scripts/install/install-target-smoke-sudoers.sh --remove
```

## Optional KO build smoke

```bash
kernel/adc_sensor_probe/tests/selftest.sh --build-only
```

Runtime load/unload and kprobe smoke require explicit root approval on the target. The KO only emits observation events and does not make RCA decisions.

## MCP smoke

```bash
ADC_HOME=/var/lib/agent-debug-compass adc-mcp --tool-list-json
ADC_HOME=/var/lib/agent-debug-compass adc-mcp --target-mode --tool-list-json
```

The MCP server exposes bounded `obs.*` tools and resources. It does not expose an arbitrary shell tool or raw artifact dump. `--target-mode` is for enrolled targets and hides controller fleet/discovery surfaces.
