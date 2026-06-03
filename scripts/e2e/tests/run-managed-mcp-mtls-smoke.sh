#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RESULT_ROOT="${1:?missing result root}"
ADC_STATE_ROOT="${2:?missing ADC_HOME}"
MCP_BIN="${3:?missing adc-mcp binary}"
ADC_BIN="${4:?missing adc binary}"

mkdir -p "$RESULT_ROOT"
CERT_DIR="$RESULT_ROOT/certs"
mkdir -p "$CERT_DIR"

CA_CERT="$CERT_DIR/ca.pem"
CA_KEY="$CERT_DIR/ca.key"
SERVER_CERT="$CERT_DIR/server.pem"
SERVER_KEY="$CERT_DIR/server.key"
SERVER_CSR="$CERT_DIR/server.csr"
SERVER_EXT="$CERT_DIR/server.ext"
CLIENT_CERT="$CERT_DIR/client.pem"
CLIENT_KEY="$CERT_DIR/client.key"
CLIENT_CSR="$CERT_DIR/client.csr"
CLIENT_EXT="$CERT_DIR/client.ext"
TOKEN="$RESULT_ROOT/managed.token"

openssl req -x509 -newkey rsa:2048 -nodes -days 2 \
  -subj /CN=adc-managed-e2e-ca \
  -addext basicConstraints=critical,CA:TRUE \
  -addext keyUsage=critical,keyCertSign,cRLSign \
  -keyout "$CA_KEY" -out "$CA_CERT" >/dev/null 2>&1
openssl req -newkey rsa:2048 -nodes \
  -subj /CN=adc-managed.test \
  -keyout "$SERVER_KEY" -out "$SERVER_CSR" >/dev/null 2>&1
printf '%s\n' 'subjectAltName=DNS:adc-managed.test' 'extendedKeyUsage=serverAuth' >"$SERVER_EXT"
openssl x509 -req -in "$SERVER_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" -CAcreateserial \
  -days 2 -extfile "$SERVER_EXT" -out "$SERVER_CERT" >/dev/null 2>&1
openssl req -newkey rsa:2048 -nodes \
  -subj /CN=adc-controller.test \
  -keyout "$CLIENT_KEY" -out "$CLIENT_CSR" >/dev/null 2>&1
printf '%s\n' 'extendedKeyUsage=clientAuth' >"$CLIENT_EXT"
openssl x509 -req -in "$CLIENT_CSR" -CA "$CA_CERT" -CAkey "$CA_KEY" -CAcreateserial \
  -days 2 -extfile "$CLIENT_EXT" -out "$CLIENT_CERT" >/dev/null 2>&1

printf '%s\n' e2e-managed-mtls-token >"$TOKEN"
port=39246
ADC_HOME="$ADC_STATE_ROOT" "$MCP_BIN" --target-mode \
  --managed-listen "127.0.0.1:$port" \
  --managed-token-file "$TOKEN" \
  --managed-tls-server-cert "$SERVER_CERT" \
  --managed-tls-server-key "$SERVER_KEY" \
  --managed-tls-client-ca "$CA_CERT" \
  >/dev/null 2>"$RESULT_ROOT/server.stderr" &
server_pid=$!
trap 'kill "$server_pid" 2>/dev/null || true; wait "$server_pid" 2>/dev/null || true' EXIT

for _ in 1 2 3 4 5 6 7 8 9 10; do
  if (: >/dev/tcp/127.0.0.1/$port) 2>/dev/null; then
    break
  fi
  sleep 0.2
done

ADC_HOME="$ADC_STATE_ROOT" "$ADC_BIN" fleet enroll \
  --target-id e2e-managed-mtls \
  --transport managed_mcp \
  --host 127.0.0.1 \
  --port "$port" \
  --auth-token-file "$TOKEN" \
  --tls-ca-file "$CA_CERT" \
  --tls-client-cert-file "$CLIENT_CERT" \
  --tls-client-key-file "$CLIENT_KEY" \
  --tls-server-name adc-managed.test \
  --tag e2e-managed-mtls >"$RESULT_ROOT/enroll.json"
ADC_HOME="$ADC_STATE_ROOT" "$ADC_BIN" fleet preflight \
  --selector tag=e2e-managed-mtls >"$RESULT_ROOT/preflight.json"
ADC_HOME="$ADC_STATE_ROOT" "$ADC_BIN" fleet snapshot \
  --selector tag=e2e-managed-mtls \
  --fleet-run-id F-E2E-MANAGED-MTLS >"$RESULT_ROOT/snapshot.json"

grep -q '"ready_count": 1' "$RESULT_ROOT/preflight.json"
grep -q '"captured_count": 1' "$RESULT_ROOT/snapshot.json"
grep -q '"transport": "managed_mcp"' "$RESULT_ROOT/snapshot.json"
