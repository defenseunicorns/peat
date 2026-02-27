use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

#[derive(Clone)]
struct NodeState {
    backend: String,
    role: String,
    ready: bool,
}

struct Orchestrator {
    expected_nodes: HashSet<String>,
    nodes: HashMap<String, NodeState>,
    start_time: Instant,
}

impl Orchestrator {
    fn new(expected_nodes: HashSet<String>) -> Self {
        Self {
            expected_nodes,
            nodes: HashMap::new(),
            start_time: Instant::now(),
        }
    }

    fn expected_count(&self) -> usize {
        self.expected_nodes.len()
    }

    fn register(&mut self, node_id: &str, backend: &str, role: &str) -> String {
        self.nodes
            .entry(node_id.to_string())
            .or_insert(NodeState {
                backend: backend.to_string(),
                role: role.to_string(),
                ready: false,
            });
        format!(
            r#"{{"status":"registered","node_count":{}}}"#,
            self.nodes.len()
        )
    }

    fn ready(&mut self, node_id: &str) -> String {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.ready = true;
        }
        let ready_count = self.nodes.values().filter(|n| n.ready).count();
        format!(
            r#"{{"status":"ready","ready_count":{},"expected":{}}}"#,
            ready_count,
            self.expected_count()
        )
    }

    fn missing_nodes(&self) -> Vec<String> {
        let mut missing: Vec<String> = self
            .expected_nodes
            .iter()
            .filter(|n| !self.nodes.contains_key(*n))
            .cloned()
            .collect();
        missing.sort();
        missing
    }

    fn unexpected_nodes(&self) -> Vec<String> {
        let mut unexpected: Vec<String> = self
            .nodes
            .keys()
            .filter(|n| !self.expected_nodes.contains(*n))
            .cloned()
            .collect();
        unexpected.sort();
        unexpected
    }

    fn status(&self) -> String {
        let expected = self.expected_count();
        let registered = self.nodes.len();
        let ready = self.nodes.values().filter(|n| n.ready).count();
        let elapsed = self.start_time.elapsed().as_secs();

        let progress_pct = if expected > 0 {
            (registered as f64 / expected as f64 * 100.0).min(100.0)
        } else {
            0.0
        };
        let ready_pct = if expected > 0 {
            (ready as f64 / expected as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        // Count by role
        let mut by_role: HashMap<String, (usize, usize)> = HashMap::new();
        for node in self.nodes.values() {
            let entry = by_role.entry(node.role.clone()).or_insert((0, 0));
            entry.0 += 1;
            if node.ready {
                entry.1 += 1;
            }
        }

        // Count by backend
        let mut by_backend: HashMap<String, usize> = HashMap::new();
        for node in self.nodes.values() {
            *by_backend.entry(node.backend.clone()).or_insert(0) += 1;
        }

        let by_role_json: Vec<String> = by_role
            .iter()
            .map(|(k, v)| format!(r#""{}":{{"registered":{},"ready":{}}}"#, k, v.0, v.1))
            .collect();

        let by_backend_json: Vec<String> = by_backend
            .iter()
            .map(|(k, v)| format!(r#""{}":{}"#, k, v))
            .collect();

        let missing = self.missing_nodes();
        let missing_count = missing.len();

        format!(
            r#"{{"expected_nodes":{},"registered":{},"ready":{},"missing":{},"progress_pct":{:.1},"ready_pct":{:.1},"elapsed_secs":{},"by_role":{{{}}},"by_backend":{{{}}}}}"#,
            expected,
            registered,
            ready,
            missing_count,
            progress_pct,
            ready_pct,
            elapsed,
            by_role_json.join(","),
            by_backend_json.join(",")
        )
    }

    fn reset(&mut self) {
        self.nodes.clear();
        self.start_time = Instant::now();
    }
}

fn parse_json_field(json: &str, field: &str) -> Option<String> {
    let pattern = format!(r#""{}":"#, field);
    if let Some(start) = json.find(&pattern) {
        let rest = &json[start + pattern.len()..];
        if rest.starts_with('"') {
            let rest = &rest[1..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

/// Parse node names from containerlab YAML topology file
fn parse_topology_file(path: &str) -> Result<HashSet<String>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path, e))?;

    let mut nodes = HashSet::new();
    let mut in_nodes_section = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Detect "nodes:" section
        if trimmed == "nodes:" {
            in_nodes_section = true;
            continue;
        }

        // Exit nodes section on other top-level keys (no leading whitespace or "links:")
        if in_nodes_section && !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
            in_nodes_section = false;
        }

        // In nodes section, look for node names (lines ending with ":")
        if in_nodes_section {
            // Node names are indented exactly 4 spaces and end with ":"
            // e.g. "    company-1-commander:"
            if line.starts_with("    ") && !line.starts_with("      ") && trimmed.ends_with(':') {
                let node_name = trimmed.trim_end_matches(':');
                // Skip the orchestrator itself
                if node_name != "orchestrator" {
                    nodes.insert(node_name.to_string());
                }
            }
        }
    }

    if nodes.is_empty() {
        return Err("No nodes found in topology file".to_string());
    }

    Ok(nodes)
}

async fn handle_request(mut stream: TcpStream, orchestrator: Arc<RwLock<Orchestrator>>) {
    let mut buffer = vec![0u8; 4096];
    let n = match stream.read(&mut buffer).await {
        Ok(n) if n > 0 => n,
        _ => return,
    };

    let request = String::from_utf8_lossy(&buffer[..n]);
    let first_line = match request.lines().next() {
        Some(line) => line,
        None => return,
    };

    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }

    let method = parts[0];
    let path = parts[1];

    let body = request
        .split("\r\n\r\n")
        .nth(1)
        .or_else(|| request.split("\n\n").nth(1))
        .unwrap_or("");

    let (status, content_type, response_body) = match (method, path) {
        ("GET", "/status") => {
            let orch = orchestrator.read().await;
            ("200 OK", "application/json", orch.status())
        }
        ("GET", "/missing") => {
            let orch = orchestrator.read().await;
            let missing = orch.missing_nodes();
            let json = format!(
                r#"{{"missing_count":{},"missing_nodes":[{}]}}"#,
                missing.len(),
                missing
                    .iter()
                    .map(|n| format!(r#""{}""#, n))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            ("200 OK", "application/json", json)
        }
        ("GET", "/unexpected") => {
            let orch = orchestrator.read().await;
            let unexpected = orch.unexpected_nodes();
            let json = format!(
                r#"{{"unexpected_count":{},"unexpected_nodes":[{}]}}"#,
                unexpected.len(),
                unexpected
                    .iter()
                    .map(|n| format!(r#""{}""#, n))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            ("200 OK", "application/json", json)
        }
        ("GET", "/") => {
            let orch = orchestrator.read().await;
            let expected = orch.expected_count();
            let registered = orch.nodes.len();
            let ready = orch.nodes.values().filter(|n| n.ready).count();
            let elapsed = orch.start_time.elapsed().as_secs();
            let missing = orch.missing_nodes();
            let progress_pct = if expected > 0 {
                (registered as f64 / expected as f64 * 100.0).min(100.0)
            } else {
                0.0
            };
            let ready_pct = if expected > 0 {
                (ready as f64 / expected as f64 * 100.0).min(100.0)
            } else {
                0.0
            };

            let missing_html = if missing.is_empty() {
                "<span style=\"color: #00ff00;\">✓ All nodes registered!</span>".to_string()
            } else {
                format!(
                    "<span style=\"color: #ff6600;\">{} missing:</span><br><code>{}</code>",
                    missing.len(),
                    missing.join(", ")
                )
            };

            let status_color = if ready == expected && expected > 0 {
                "#00ff00" // Green - all ready
            } else if registered == expected && expected > 0 {
                "#ffff00" // Yellow - all registered but not ready
            } else {
                "#ff6600" // Orange - still waiting
            };

            let html = format!(
                r#"<!DOCTYPE html>
<html>
<head>
    <title>Lab Orchestrator</title>
    <meta http-equiv="refresh" content="2">
    <style>
        body {{ font-family: monospace; background: #1a1a1a; color: #00ff00; padding: 20px; }}
        .box {{ border: 1px solid #00ff00; padding: 10px; margin: 10px 0; }}
        .progress {{ background: #333; height: 20px; width: 100%; }}
        .progress-bar {{ background: {}; height: 100%; transition: width 0.5s; }}
        h1 {{ border-bottom: 2px solid #00ff00; }}
        a {{ color: #00ff00; }}
        code {{ background: #333; padding: 2px 5px; }}
    </style>
</head>
<body>
    <h1>Lab Orchestrator (Rust/Tokio)</h1>
    <div class="box">
        <strong>Elapsed:</strong> {}s
    </div>
    <div class="box">
        <strong>Registered:</strong> {} / {} ({:.1}%)<br>
        <div class="progress"><div class="progress-bar" style="width: {:.1}%"></div></div>
    </div>
    <div class="box">
        <strong>Ready:</strong> {} / {} ({:.1}%)<br>
        <div class="progress"><div class="progress-bar" style="width: {:.1}%"></div></div>
    </div>
    <div class="box">
        <strong>Missing Nodes:</strong><br>
        {}
    </div>
    <div class="box">
        <a href="/status">JSON Status</a> |
        <a href="/missing">Missing Nodes API</a>
    </div>
</body>
</html>"#,
                status_color,
                elapsed,
                registered,
                expected,
                progress_pct,
                progress_pct,
                ready,
                expected,
                ready_pct,
                ready_pct,
                missing_html
            );
            ("200 OK", "text/html", html)
        }
        ("POST", "/register") => {
            let node_id = parse_json_field(body, "node_id").unwrap_or_else(|| "unknown".to_string());
            let backend = parse_json_field(body, "backend").unwrap_or_else(|| "unknown".to_string());
            let role = parse_json_field(body, "role").unwrap_or_else(|| "unknown".to_string());
            let mut orch = orchestrator.write().await;
            let resp = orch.register(&node_id, &backend, &role);
            ("200 OK", "application/json", resp)
        }
        ("POST", "/ready") => {
            let node_id = parse_json_field(body, "node_id").unwrap_or_else(|| "unknown".to_string());
            let mut orch = orchestrator.write().await;
            let resp = orch.ready(&node_id);
            ("200 OK", "application/json", resp)
        }
        ("POST", "/reset") => {
            let mut orch = orchestrator.write().await;
            orch.reset();
            ("200 OK", "application/json", r#"{"status":"reset"}"#.to_string())
        }
        ("POST", "/metrics") => {
            ("200 OK", "application/json", r#"{"status":"ok"}"#.to_string())
        }
        ("POST", "/error") => {
            ("200 OK", "application/json", r#"{"status":"recorded"}"#.to_string())
        }
        _ => (
            "404 Not Found",
            "application/json",
            r#"{"error":"not found"}"#.to_string(),
        ),
    };

    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        content_type,
        response_body.len(),
        response_body
    );

    let _ = stream.write_all(response.as_bytes()).await;
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut port = 8080u16;
    let mut topology_file: Option<String> = None;
    let mut expected_count: Option<usize> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                if i + 1 < args.len() {
                    port = args[i + 1].parse().unwrap_or(8080);
                    i += 1;
                }
            }
            "--topology" => {
                if i + 1 < args.len() {
                    topology_file = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--expected-nodes" => {
                if i + 1 < args.len() {
                    expected_count = Some(args[i + 1].parse().unwrap_or(447));
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    // Load expected nodes from topology file or use count
    let expected_nodes: HashSet<String> = if let Some(ref path) = topology_file {
        match parse_topology_file(path) {
            Ok(nodes) => {
                println!("Loaded {} expected nodes from topology: {}", nodes.len(), path);
                nodes
            }
            Err(e) => {
                eprintln!("ERROR: {}", e);
                std::process::exit(1);
            }
        }
    } else if let Some(count) = expected_count {
        // Fallback: generate placeholder names (for backward compatibility)
        println!("WARNING: No topology file provided, using count-based tracking ({})", count);
        (0..count).map(|i| format!("node-{}", i)).collect()
    } else {
        eprintln!("ERROR: Must provide --topology <file> or --expected-nodes <count>");
        std::process::exit(1);
    };

    let node_count = expected_nodes.len();
    let orchestrator = Arc::new(RwLock::new(Orchestrator::new(expected_nodes)));
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Failed to bind");

    println!("Lab Orchestrator (Rust/Tokio) running on http://0.0.0.0:{}", port);
    println!("Expecting {} nodes", node_count);
    println!("Dashboard: http://localhost:{}/", port);
    println!("Status API: http://localhost:{}/status", port);
    println!("Missing API: http://localhost:{}/missing", port);

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let orch = Arc::clone(&orchestrator);
                tokio::spawn(async move {
                    handle_request(stream, orch).await;
                });
            }
            Err(e) => {
                eprintln!("Connection error: {}", e);
            }
        }
    }
}
