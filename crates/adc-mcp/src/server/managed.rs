use std::{
    borrow::Cow,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::Arc,
    thread,
    time::Duration,
};

use rmcp::model::{CallToolRequestParam, ListResourceTemplatesResult, ListToolsResult};
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer},
    server::WebPkiClientVerifier,
    RootCertStore, ServerConfig, ServerConnection, StreamOwned,
};

use super::{arg_value, resource_templates, tool_definitions, AdcMcpServer, ServerMode};

pub(super) fn run_managed_mcp_listener(
    listen_addr: &str,
    token_file: &str,
    mode: ServerMode,
    tls_config: Option<Arc<ServerConfig>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let auth = Arc::new(ManagedMcpAuth::new(PathBuf::from(token_file))?);
    auth.validate()?;
    let listener = TcpListener::bind(listen_addr)?;
    eprintln!(
        "managed_mcp.listen addr={} mode={:?} artifact_root={}",
        listener.local_addr()?,
        mode,
        adc_core::snapshot::default_artifact_root().display()
    );
    let server = Arc::new(AdcMcpServer {
        artifact_root: adc_core::snapshot::default_artifact_root(),
        mode,
    });
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let server = Arc::clone(&server);
                let auth = Arc::clone(&auth);
                let tls_config = tls_config.clone();
                thread::spawn(move || {
                    let result = if let Some(config) = tls_config {
                        handle_managed_mcp_tls_stream(stream, &server, &auth, config)
                    } else {
                        handle_managed_mcp_plain_stream(stream, &server, &auth)
                    };
                    if let Err(err) = result {
                        eprintln!(
                            "managed_mcp.request outcome=error error={}",
                            bounded_text(&err)
                        );
                    }
                });
            }
            Err(err) => eprintln!("managed_mcp.accept outcome=error error={err}"),
        }
    }
    Ok(())
}

fn handle_managed_mcp_plain_stream(
    stream: TcpStream,
    server: &AdcMcpServer,
    auth: &ManagedMcpAuth,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|err| format!("failed to set read timeout: {err}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|err| format!("failed to set write timeout: {err}"))?;
    handle_managed_mcp_stream(stream, server, auth)
}

fn handle_managed_mcp_tls_stream(
    stream: TcpStream,
    server: &AdcMcpServer,
    auth: &ManagedMcpAuth,
    config: Arc<ServerConfig>,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|err| format!("failed to set read timeout: {err}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|err| format!("failed to set write timeout: {err}"))?;
    let connection =
        ServerConnection::new(config).map_err(|err| format!("TLS server setup failed: {err}"))?;
    let stream = StreamOwned::new(connection, stream);
    handle_managed_mcp_stream(stream, server, auth)
}

fn handle_managed_mcp_stream(
    mut stream: impl Read + Write,
    server: &AdcMcpServer,
    auth: &ManagedMcpAuth,
) -> Result<(), String> {
    let mut reader = BufReader::new(&mut stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|err| format!("failed to read request line: {err}"))?;
    if !request_line.starts_with("POST /mcp ") {
        return write_http_json(
            reader.get_mut(),
            404,
            serde_json::json!({"error": "not_found"}),
        );
    }

    let mut content_length = None;
    let mut bearer_token = None;
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|err| format!("failed to read header: {err}"))?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        let Some((name, value)) = trimmed.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name == "content-length" {
            content_length = value.parse::<usize>().ok();
        } else if name == "authorization" {
            bearer_token = value.strip_prefix("Bearer ").map(str::to_string);
        }
    }

    let authorized = match auth.is_authorized(bearer_token.as_deref()) {
        Ok(authorized) => authorized,
        Err(err) => {
            eprintln!(
                "managed_mcp.request outcome=auth_unavailable error={}",
                bounded_text(&err)
            );
            return write_http_json(
                reader.get_mut(),
                503,
                serde_json::json!({"error": "auth_unavailable"}),
            );
        }
    };
    if !authorized {
        eprintln!("managed_mcp.request outcome=unauthorized");
        return write_http_json(
            reader.get_mut(),
            401,
            serde_json::json!({"error": "unauthorized"}),
        );
    }
    let content_length = content_length.ok_or_else(|| "missing content-length".to_string())?;
    let mut body = vec![0_u8; content_length];
    reader
        .read_exact(&mut body)
        .map_err(|err| format!("failed to read request body: {err}"))?;
    let request: serde_json::Value =
        serde_json::from_slice(&body).map_err(|err| format!("invalid json body: {err}"))?;
    let response = handle_managed_mcp_jsonrpc(server, request);
    write_http_json(reader.get_mut(), 200, response)
}

pub(super) fn managed_mcp_tls_server_config_from_args(
    args: &[String],
) -> Result<Option<Arc<ServerConfig>>, Box<dyn std::error::Error>> {
    let cert_file = arg_value(args, "--managed-tls-server-cert");
    let key_file = arg_value(args, "--managed-tls-server-key");
    let client_ca_file = arg_value(args, "--managed-tls-client-ca");
    let any_tls = cert_file.is_some() || key_file.is_some() || client_ca_file.is_some();
    if !any_tls {
        return Ok(None);
    }
    let cert_file = cert_file.ok_or("--managed-tls-server-cert is required for managed mTLS")?;
    let key_file = key_file.ok_or("--managed-tls-server-key is required for managed mTLS")?;
    let client_ca_file =
        client_ca_file.ok_or("--managed-tls-client-ca is required for managed mTLS")?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let certs = load_tls_certs(&cert_file)?;
    let key = load_tls_private_key(&key_file)?;
    let mut client_roots = RootCertStore::empty();
    for cert in load_tls_certs(&client_ca_file)? {
        client_roots.add(cert)?;
    }
    let verifier = WebPkiClientVerifier::builder(client_roots.into()).build()?;
    let config = ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)?;
    Ok(Some(Arc::new(config)))
}

fn load_tls_certs(path: &str) -> Result<Vec<CertificateDer<'static>>, Box<dyn std::error::Error>> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?;
    if certs.is_empty() {
        return Err(format!("TLS certificate file {path} did not contain certificates").into());
    }
    Ok(certs)
}

fn load_tls_private_key(path: &str) -> Result<PrivateKeyDer<'static>, Box<dyn std::error::Error>> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    rustls_pemfile::private_key(&mut reader)?
        .ok_or_else(|| format!("TLS private key file {path} did not contain a private key").into())
}

#[derive(Clone)]
struct ManagedMcpAuth {
    token_file: PathBuf,
}

impl ManagedMcpAuth {
    fn new(token_file: PathBuf) -> Result<Self, String> {
        if token_file.as_os_str().is_empty() {
            return Err("managed MCP token file path must not be empty".to_string());
        }
        Ok(Self { token_file })
    }

    fn validate(&self) -> Result<(), String> {
        let token = self.current_token()?;
        if token.is_empty() {
            return Err("managed MCP token file must not be empty".to_string());
        }
        Ok(())
    }

    fn is_authorized(&self, provided: Option<&str>) -> Result<bool, String> {
        let Some(provided) = provided else {
            return Ok(false);
        };
        let expected = self.current_token()?;
        if expected.is_empty() {
            return Err("managed MCP token file must not be empty".to_string());
        }
        Ok(provided == expected)
    }

    fn current_token(&self) -> Result<String, String> {
        Ok(fs::read_to_string(&self.token_file)
            .map_err(|err| {
                format!(
                    "failed to read managed MCP token file {}: {err}",
                    self.token_file.display()
                )
            })?
            .trim()
            .to_string())
    }
}

fn handle_managed_mcp_jsonrpc(
    server: &AdcMcpServer,
    request: serde_json::Value,
) -> serde_json::Value {
    let id = request
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let method = request
        .get("method")
        .and_then(|method| method.as_str())
        .unwrap_or_default();
    match method {
        "initialize" => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "serverInfo": {
                    "name": "adc-mcp",
                    "version": adc_core::VERSION
                }
            }
        }),
        "tools/list" => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": ListToolsResult::with_all_items(tool_definitions(server.mode))
        }),
        "resources/templates/list" => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": ListResourceTemplatesResult::with_all_items(resource_templates(server.mode))
        }),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or_default();
            let Some(name) = params.get("name").and_then(|name| name.as_str()) else {
                return managed_jsonrpc_error(id, -32602, "tools/call requires params.name");
            };
            let arguments = params
                .get("arguments")
                .and_then(|arguments| arguments.as_object())
                .cloned();
            let call = CallToolRequestParam {
                name: Cow::Owned(name.to_string()),
                arguments,
            };
            eprintln!("managed_mcp.tool_call tool={name}");
            match server.call_tool_sync(call) {
                Ok(result) => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result
                }),
                Err(err) => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": err
                }),
            }
        }
        _ => managed_jsonrpc_error(id, -32601, format!("unknown method: {method}")),
    }
}

fn managed_jsonrpc_error(
    id: serde_json::Value,
    code: i64,
    message: impl Into<String>,
) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message.into()
        }
    })
}

fn write_http_json(
    stream: &mut impl Write,
    status: u16,
    value: serde_json::Value,
) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        503 => "Service Unavailable",
        _ => "Error",
    };
    let body = value.to_string();
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|err| format!("failed to write response: {err}"))
}

fn bounded_text(value: &str) -> String {
    const LIMIT: usize = 512;
    if value.len() <= LIMIT {
        value.to_string()
    } else {
        format!("{}...", &value[..LIMIT])
    }
}
