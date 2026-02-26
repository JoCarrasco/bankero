use crate::cli::{LoginArgs, SyncArgs, SyncCmd};
use crate::config::{AppConfig, funny_name_from_uuid, now_utc, workspace_slug, write_config};
use crate::db::{Db, StoredRate};
use crate::domain::EventPayload;
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::BufWriter;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, Instant};
use uuid::Uuid;

fn should_auto_accept_sync(test_once: bool) -> bool {
    if test_once {
        return true;
    }
    matches!(
        std::env::var("BANKERO_SYNC_AUTO_ACCEPT").as_deref(),
        Ok("1") | Ok("true") | Ok("yes") | Ok("y")
    )
}

fn prompt_accept_sync(peer: Option<SocketAddr>) -> Result<bool> {
    let peer_display = peer
        .map(|p| p.to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    print!("Incoming sync from {peer_display}. Accept? (y/n): ");
    std::io::stdout().flush().ok();

    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) => return Ok(false),
        Ok(_) => {}
        Err(_) => return Ok(false),
    }

    let s = line.trim().to_ascii_lowercase();
    if s == "y" || s == "yes" {
        return Ok(true);
    }
    if s == "n" || s == "no" {
        return Ok(false);
    }

    println!("Please answer y or n.");
    prompt_accept_sync(peer)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireEvent {
    pub id: Uuid,
    pub payload: EventPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireRate {
    pub provider: String,
    pub base: String,
    pub quote: String,
    pub as_of: DateTime<Utc>,
    pub rate: rust_decimal::Decimal,
}

fn resolve_sync_dir(args_dir: Option<String>, cfg: &AppConfig) -> Result<PathBuf> {
    if let Some(dir) = args_dir {
        return Ok(PathBuf::from(dir));
    }
    if let Some(dir) = cfg.sync_dir.clone() {
        return Ok(PathBuf::from(dir));
    }
    Err(anyhow!(
        "No sync folder configured. Run: bankero login --sync-dir <path> (or set BANKERO_SYNC_DIR)."
    ))
}

fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("Failed to create dir {}", path.display()))
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("atomic_write requires a parent dir")?;
    ensure_dir(parent)?;

    let tmp = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("bankero")
    ));

    {
        let mut f = fs::File::create(&tmp)
            .with_context(|| format!("Failed to create temp file {}", tmp.display()))?;
        f.write_all(contents)
            .with_context(|| format!("Failed to write temp file {}", tmp.display()))?;
        f.sync_all()
            .with_context(|| format!("Failed to sync temp file {}", tmp.display()))?;
    }

    fs::rename(&tmp, path)
        .with_context(|| format!("Failed to rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

fn jsonl_write<T: Serialize>(path: &Path, items: &[T]) -> Result<()> {
    let mut buf = Vec::new();
    for item in items {
        serde_json::to_writer(&mut buf, item)?;
        buf.push(b'\n');
    }
    atomic_write(path, &buf)
}

fn jsonl_read_lines(path: &Path) -> Result<Vec<String>> {
    let file =
        fs::File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line.with_context(|| format!("Failed reading {}", path.display()))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.push(trimmed.to_string());
    }
    Ok(out)
}

fn sync_root(sync_dir: &Path) -> PathBuf {
    sync_dir.join("bankero")
}

fn workspace_root(sync_dir: &Path, workspace: &str) -> PathBuf {
    sync_root(sync_dir)
        .join("workspaces")
        .join(workspace_slug(workspace))
}

fn device_root(sync_dir: &Path, workspace: &str, device_id: Uuid) -> PathBuf {
    workspace_root(sync_dir, workspace)
        .join("devices")
        .join(device_id.to_string())
}

pub fn handle_login(args: LoginArgs, cfg: &mut AppConfig, cfg_path: &Path) -> Result<()> {
    let mut changed = false;
    if let Some(dir) = args.sync_dir {
        cfg.sync_dir = Some(dir);
        changed = true;
    }

    if let Some(name) = args.name {
        cfg.device_name = Some(name);
        changed = true;
    } else if args.regen_name {
        cfg.device_name = Some(funny_name_from_uuid(Uuid::new_v4()));
        changed = true;
    }

    if changed {
        write_config(cfg_path, cfg)?;
    }

    println!("device_id\t{}", cfg.device_id);
    println!(
        "device_name\t{}",
        cfg.device_name.as_deref().unwrap_or("<unknown>")
    );
    println!("workspace\t{}", cfg.current_workspace);
    if let Some(dir) = cfg.sync_dir.as_deref() {
        println!("sync_dir\t{}", dir);
    } else {
        println!("sync_dir\t<not set>");
    }

    Ok(())
}

pub fn handle_sync(db: &Db, args: SyncArgs, cfg: &mut AppConfig, cfg_path: &Path) -> Result<()> {
    match args.cmd {
        SyncCmd::Status => {
            let sync_dir = resolve_sync_dir(args.dir, cfg)?;
            sync_status(db, cfg, &sync_dir)
        }
        SyncCmd::Now => {
            let sync_dir = resolve_sync_dir(args.dir, cfg)?;
            let (imported_events, imported_rates) = sync_now(db, cfg, &sync_dir)?;
            cfg.last_sync_at = Some(now_utc());
            write_config(cfg_path, cfg)?;
            println!(
                "synced\t{}\t(imported events: {}, imported rates: {})",
                sync_dir.display(),
                imported_events,
                imported_rates
            );
            Ok(())
        }
        SyncCmd::Discover { timeout_ms, target } => {
            sync_discover(cfg, cfg_path, timeout_ms, target)
        }
        SyncCmd::Expose {
            name,
            test_bind,
            test_udp_port,
            test_tcp_port,
            test_once,
            test_print_ports,
        } => sync_expose(
            db,
            cfg,
            cfg_path,
            name,
            test_bind,
            test_udp_port,
            test_tcp_port,
            test_once,
            test_print_ports,
        ),
        SyncCmd::External(argv) => sync_external(db, cfg, cfg_path, argv),
    }
}

// -------------------------
// LAN sync (MVP)
// -------------------------

const DISCOVERY_PORT: u16 = 45_667;
const SYNC_PORT: u16 = 45_668;
const DISCOVERY_MAGIC: &str = "bankero-sync-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscoverRequest {
    magic: String,
    workspace: String,
    nonce: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscoverResponse {
    magic: String,
    workspace: String,
    nonce: u64,
    device_id: Uuid,
    device_name: String,
    user_host: String,
    version: String,
    tcp_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedPeer {
    device_id: Uuid,
    device_name: String,
    user_host: String,
    version: String,
    addr: IpAddr,
    tcp_port: u16,
    last_seen_at: DateTime<Utc>,
}

fn peers_cache_path(cfg_path: &Path) -> Result<PathBuf> {
    let dir = cfg_path
        .parent()
        .context("config path has no parent directory")?;
    Ok(dir.join("peers.json"))
}

fn load_peers_cache(cfg_path: &Path) -> Result<Vec<CachedPeer>> {
    let path = peers_cache_path(cfg_path)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let peers: Vec<CachedPeer> = serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(peers)
}

fn write_peers_cache(cfg_path: &Path, peers: &[CachedPeer]) -> Result<()> {
    let path = peers_cache_path(cfg_path)?;
    let json = serde_json::to_string_pretty(peers)?;
    fs::write(&path, json).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn local_user_host() -> String {
    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
    let host = std::env::var("HOSTNAME").ok().or_else(|| {
        fs::read_to_string("/etc/hostname")
            .ok()
            .map(|s| s.lines().next().unwrap_or("unknown").trim().to_string())
    });
    format!("{}@{}", user, host.unwrap_or_else(|| "unknown".to_string()))
}

fn sync_discover(
    cfg: &AppConfig,
    cfg_path: &Path,
    timeout_ms: u64,
    target: Option<String>,
) -> Result<()> {
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .context("Failed to bind UDP socket for discovery")?;
    sock.set_broadcast(true)
        .context("Failed to enable UDP broadcast")?;

    let nonce = u64::from_le_bytes([
        cfg.device_id.as_bytes()[0],
        cfg.device_id.as_bytes()[1],
        cfg.device_id.as_bytes()[2],
        cfg.device_id.as_bytes()[3],
        cfg.device_id.as_bytes()[4],
        cfg.device_id.as_bytes()[5],
        cfg.device_id.as_bytes()[6],
        cfg.device_id.as_bytes()[7],
    ]) ^ (now_utc().timestamp_millis() as u64);

    let req = DiscoverRequest {
        magic: DISCOVERY_MAGIC.to_string(),
        workspace: cfg.current_workspace.clone(),
        nonce,
    };
    let payload = serde_json::to_vec(&req)?;

    if let Some(target) = target {
        let addr = SocketAddr::from_str(&target)
            .with_context(|| format!("Invalid --target socket address '{target}'"))?;
        let _ = sock.send_to(&payload, addr);
    } else {
        let broadcast = SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), DISCOVERY_PORT);
        let _ = sock.send_to(&payload, broadcast);
        // Also probe localhost to make local testing work even if broadcast is blocked.
        let _ = sock.send_to(
            &payload,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DISCOVERY_PORT),
        );
    }

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    sock.set_read_timeout(Some(Duration::from_millis(100)))
        .context("Failed to set discovery timeout")?;

    let mut buf = [0u8; 64 * 1024];
    let mut peers_by_id: std::collections::BTreeMap<Uuid, CachedPeer> =
        std::collections::BTreeMap::new();

    while Instant::now() < deadline {
        match sock.recv_from(&mut buf) {
            Ok((n, from)) => {
                let resp: DiscoverResponse = match serde_json::from_slice(&buf[..n]) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if resp.magic != DISCOVERY_MAGIC {
                    continue;
                }
                if resp.nonce != nonce {
                    continue;
                }
                if resp.workspace != cfg.current_workspace {
                    continue;
                }

                peers_by_id.insert(
                    resp.device_id,
                    CachedPeer {
                        device_id: resp.device_id,
                        device_name: resp.device_name,
                        user_host: resp.user_host,
                        version: resp.version,
                        addr: from.ip(),
                        tcp_port: resp.tcp_port,
                        last_seen_at: now_utc(),
                    },
                );
            }
            Err(err) => {
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut
                {
                    continue;
                }
                return Err(err).context("Discovery UDP recv failed");
            }
        }
    }

    let mut peers: Vec<CachedPeer> = peers_by_id.into_values().collect();
    peers.sort_by(|a, b| {
        a.device_name
            .cmp(&b.device_name)
            .then(a.user_host.cmp(&b.user_host))
    });
    write_peers_cache(cfg_path, &peers)?;

    for (idx, p) in peers.iter().enumerate() {
        println!(
            "@{} \"{}\" - {} - bankero v{}",
            idx + 1,
            p.device_name,
            p.user_host,
            p.version
        );
    }
    Ok(())
}

fn sync_expose(
    db: &Db,
    cfg: &mut AppConfig,
    cfg_path: &Path,
    name: Option<String>,
    test_bind: Option<String>,
    test_udp_port: Option<u16>,
    test_tcp_port: Option<u16>,
    test_once: bool,
    test_print_ports: bool,
) -> Result<()> {
    let chosen = name
        .or_else(|| cfg.device_name.clone())
        .unwrap_or_else(|| "bankero".to_string());
    if cfg.device_name.as_deref() != Some(&chosen) {
        cfg.device_name = Some(chosen.clone());
        write_config(cfg_path, cfg)?;
    }

    let workspace = cfg.current_workspace.clone();
    let device_id = cfg.device_id;
    let user_host = local_user_host();
    let version = env!("CARGO_PKG_VERSION").to_string();

    let bind_ip: IpAddr = if let Some(s) = test_bind {
        IpAddr::from_str(&s).with_context(|| format!("Invalid --test-bind '{s}'"))?
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    };
    let udp_port = test_udp_port.unwrap_or(DISCOVERY_PORT);
    let tcp_port = test_tcp_port.unwrap_or(SYNC_PORT);

    let listener = TcpListener::bind(SocketAddr::new(bind_ip, tcp_port))
        .with_context(|| format!("Failed to bind TCP sync address {}:{}", bind_ip, tcp_port))?;
    let tcp_local = listener
        .local_addr()
        .context("Failed to read TCP local addr")?;

    let udp = UdpSocket::bind(SocketAddr::new(bind_ip, udp_port)).with_context(|| {
        format!(
            "Failed to bind UDP discovery address {}:{}",
            bind_ip, udp_port
        )
    })?;
    udp.set_broadcast(true).ok();

    let udp_local = udp.local_addr().context("Failed to read UDP local addr")?;

    if test_print_ports {
        println!("lan_udp\t{}", udp_local);
        println!("lan_tcp\t{}", tcp_local);
    }

    let tcp_port_for_discovery = tcp_local.port();

    let udp_thread = std::thread::spawn(move || {
        let mut buf = [0u8; 64 * 1024];
        loop {
            let Ok((n, from)) = udp.recv_from(&mut buf) else {
                continue;
            };
            let req: DiscoverRequest = match serde_json::from_slice(&buf[..n]) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if req.magic != DISCOVERY_MAGIC {
                continue;
            }
            if req.workspace != workspace {
                continue;
            }
            let resp = DiscoverResponse {
                magic: DISCOVERY_MAGIC.to_string(),
                workspace: workspace.clone(),
                nonce: req.nonce,
                device_id,
                device_name: chosen.clone(),
                user_host: user_host.clone(),
                version: version.clone(),
                tcp_port: tcp_port_for_discovery,
            };
            if let Ok(bytes) = serde_json::to_vec(&resp) {
                let _ = udp.send_to(&bytes, from);
            }
        }
    });

    println!(
        "Exposed as \"{}\" waiting for sync events",
        cfg.device_name.as_deref().unwrap_or("bankero")
    );

    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };

        let peer = stream.peer_addr().ok();
        if !should_auto_accept_sync(test_once) {
            let accept = prompt_accept_sync(peer)?;
            if !accept {
                let mut w = BufWriter::new(stream);
                let _ = write_msg(
                    &mut w,
                    &SyncMsg::Error {
                        message: "Sync rejected by user".to_string(),
                    },
                );
                println!("rejected sync");
                continue;
            }
        }

        println!("received sync event");
        println!("syncing..");
        match handle_sync_connection_server(db, cfg, stream) {
            Ok(stats) => {
                println!("sync complete");
                println!("sync summary:");
                println!("- sent events: {}", stats.sent_events);
                println!("- sent rates: {}", stats.sent_rates);
                println!("- imported events: {}", stats.imported_events);
                println!("- imported rates: {}", stats.imported_rates);
            }
            Err(err) => {
                eprintln!("sync failed: {err:#}");
            }
        }

        if test_once {
            break;
        }
    }

    // Intentionally detach the UDP responder thread; the expose command is long-running.
    // For test mode (`--test-once`), the process will exit and the thread will stop.
    let _ = udp_thread;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum SyncMsg {
    #[serde(rename = "hello")]
    Hello {
        workspace: String,
        device_id: Uuid,
        device_name: String,
        user_host: String,
        version: String,
    },

    #[serde(rename = "hello_ack")]
    HelloAck {
        device_id: Uuid,
        device_name: String,
        user_host: String,
        version: String,
    },

    #[serde(rename = "push_begin")]
    PushBegin { events: usize, rates: usize },

    #[serde(rename = "event")]
    Event { id: Uuid, payload: EventPayload },

    #[serde(rename = "rate")]
    Rate {
        provider: String,
        base: String,
        quote: String,
        as_of: DateTime<Utc>,
        rate: rust_decimal::Decimal,
    },

    #[serde(rename = "push_end")]
    PushEnd,

    #[serde(rename = "pull_begin")]
    PullBegin { events: usize, rates: usize },

    #[serde(rename = "pull_end")]
    PullEnd,

    #[serde(rename = "summary")]
    Summary {
        imported_events: usize,
        imported_rates: usize,
    },

    #[serde(rename = "error")]
    Error { message: String },
}

fn write_msg(w: &mut BufWriter<TcpStream>, msg: &SyncMsg) -> Result<()> {
    serde_json::to_writer(&mut *w, msg)?;
    w.write_all(b"\n")?;
    w.flush()?;
    Ok(())
}

fn read_msg(line: &str) -> Result<SyncMsg> {
    let msg: SyncMsg = serde_json::from_str(line)
        .with_context(|| format!("Failed to parse sync message: {}", line))?;
    Ok(msg)
}

#[derive(Debug, Clone, Copy)]
struct SyncStats {
    imported_events: usize,
    imported_rates: usize,
    sent_events: usize,
    sent_rates: usize,
}

fn handle_sync_connection_server(db: &Db, cfg: &AppConfig, stream: TcpStream) -> Result<SyncStats> {
    let peer = stream.peer_addr().ok();
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = BufWriter::new(stream);

    let mut line = String::new();
    reader.read_line(&mut line)?;
    if line.trim().is_empty() {
        return Ok(SyncStats {
            imported_events: 0,
            imported_rates: 0,
            sent_events: 0,
            sent_rates: 0,
        });
    }
    let hello = read_msg(line.trim())?;
    let SyncMsg::Hello { workspace, .. } = hello else {
        write_msg(
            &mut writer,
            &SyncMsg::Error {
                message: "Expected hello".to_string(),
            },
        )?;
        return Ok(SyncStats {
            imported_events: 0,
            imported_rates: 0,
            sent_events: 0,
            sent_rates: 0,
        });
    };

    if workspace != cfg.current_workspace {
        write_msg(
            &mut writer,
            &SyncMsg::Error {
                message: format!(
                    "Workspace mismatch (peer={}, local={})",
                    workspace, cfg.current_workspace
                ),
            },
        )?;
        return Ok(SyncStats {
            imported_events: 0,
            imported_rates: 0,
            sent_events: 0,
            sent_rates: 0,
        });
    }

    write_msg(
        &mut writer,
        &SyncMsg::HelloAck {
            device_id: cfg.device_id,
            device_name: cfg
                .device_name
                .clone()
                .unwrap_or_else(|| "bankero".to_string()),
            user_host: local_user_host(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    )?;

    // Receive push.
    let mut imported_events = 0usize;
    let mut imported_rates = 0usize;
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let msg = read_msg(line.trim())?;
        match msg {
            SyncMsg::PushBegin { .. } => {}
            SyncMsg::Event { id, payload } => {
                if db.insert_event_ignore(id, &payload)? {
                    imported_events += 1;
                }
            }
            SyncMsg::Rate {
                provider,
                base,
                quote,
                as_of,
                rate,
            } => {
                db.set_rate(&provider, &base, &quote, as_of, rate)?;
                imported_rates += 1;
            }
            SyncMsg::PushEnd => break,
            SyncMsg::Error { .. }
            | SyncMsg::Hello { .. }
            | SyncMsg::HelloAck { .. }
            | SyncMsg::PullBegin { .. }
            | SyncMsg::PullEnd
            | SyncMsg::Summary { .. } => {}
        }
    }

    // Send pull.
    let events = db.list_events()?;
    let rates = db.list_all_rates()?;
    let sent_events = events.len();
    let sent_rates = rates.len();
    write_msg(
        &mut writer,
        &SyncMsg::PullBegin {
            events: sent_events,
            rates: sent_rates,
        },
    )?;

    for e in events {
        write_msg(
            &mut writer,
            &SyncMsg::Event {
                id: e.event_id,
                payload: e.payload,
            },
        )?;
    }
    for r in rates {
        write_msg(
            &mut writer,
            &SyncMsg::Rate {
                provider: r.provider,
                base: r.base,
                quote: r.quote,
                as_of: r.as_of,
                rate: r.rate,
            },
        )?;
    }
    write_msg(&mut writer, &SyncMsg::PullEnd)?;

    write_msg(
        &mut writer,
        &SyncMsg::Summary {
            imported_events,
            imported_rates,
        },
    )?;

    if let Some(peer) = peer {
        let _ = peer;
    }
    Ok(SyncStats {
        imported_events,
        imported_rates,
        sent_events,
        sent_rates,
    })
}

fn sync_external(db: &Db, cfg: &mut AppConfig, cfg_path: &Path, argv: Vec<String>) -> Result<()> {
    // Expected: ["@1", "all"]
    if argv.len() < 2 {
        return Err(anyhow!(
            "Invalid sync command. Try: bankero sync discover; then: bankero sync @1 all"
        ));
    }
    let handle = &argv[0];
    let cmd = &argv[1];
    if !handle.starts_with('@') {
        return Err(anyhow!(
            "Invalid peer handle '{}'. Expected like @1. Run: bankero sync discover",
            handle
        ));
    }
    if cmd != "all" {
        return Err(anyhow!(
            "Unknown sync action '{}'. Only 'all' is supported.",
            cmd
        ));
    }
    let idx: usize = handle[1..]
        .parse()
        .with_context(|| format!("Invalid peer handle '{}'", handle))?;
    if idx == 0 {
        return Err(anyhow!("Peer handle must be >= 1"));
    }

    let peers = load_peers_cache(cfg_path)?;
    let Some(peer) = peers.get(idx - 1).cloned() else {
        return Err(anyhow!(
            "No peer {} in cache. Run: bankero sync discover",
            handle
        ));
    };

    println!("sync in-progress");
    let addr = SocketAddr::new(peer.addr, peer.tcp_port);
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
        .with_context(|| format!("Failed to connect to {}", addr))?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(10))).ok();

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = BufWriter::new(stream);

    let hello = SyncMsg::Hello {
        workspace: cfg.current_workspace.clone(),
        device_id: cfg.device_id,
        device_name: cfg
            .device_name
            .clone()
            .unwrap_or_else(|| "bankero".to_string()),
        user_host: local_user_host(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    write_msg(&mut writer, &hello)?;

    let mut line = String::new();
    reader.read_line(&mut line)?;
    let ack = read_msg(line.trim())?;
    match ack {
        SyncMsg::HelloAck { .. } => {}
        SyncMsg::Error { message } => return Err(anyhow!(message)),
        _ => return Err(anyhow!("Unexpected response from peer")),
    }

    let events = db.list_events()?;
    let rates = db.list_all_rates()?;

    let sent_events = events.len();
    let sent_rates = rates.len();
    write_msg(
        &mut writer,
        &SyncMsg::PushBegin {
            events: sent_events,
            rates: sent_rates,
        },
    )?;
    for e in events {
        write_msg(
            &mut writer,
            &SyncMsg::Event {
                id: e.event_id,
                payload: e.payload,
            },
        )?;
    }
    for r in rates {
        write_msg(
            &mut writer,
            &SyncMsg::Rate {
                provider: r.provider,
                base: r.base,
                quote: r.quote,
                as_of: r.as_of,
                rate: r.rate,
            },
        )?;
    }
    write_msg(&mut writer, &SyncMsg::PushEnd)?;

    // Receive pull.
    let mut imported_events = 0usize;
    let mut imported_rates = 0usize;
    let mut peer_imported_events = 0usize;
    let mut peer_imported_rates = 0usize;
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let msg = read_msg(line.trim())?;
        match msg {
            SyncMsg::PullBegin { .. } => {}
            SyncMsg::Event { id, payload } => {
                if db.insert_event_ignore(id, &payload)? {
                    imported_events += 1;
                }
            }
            SyncMsg::Rate {
                provider,
                base,
                quote,
                as_of,
                rate,
            } => {
                db.set_rate(&provider, &base, &quote, as_of, rate)?;
                imported_rates += 1;
            }
            SyncMsg::PullEnd => {}
            SyncMsg::Summary {
                imported_events,
                imported_rates,
            } => {
                peer_imported_events = imported_events;
                peer_imported_rates = imported_rates;
                break;
            }
            SyncMsg::Error { message } => return Err(anyhow!(message)),
            _ => {}
        }
    }

    cfg.last_sync_at = Some(now_utc());
    write_config(cfg_path, cfg)?;

    println!("sync complete");
    println!("sync summary:");
    println!("- sent events: {sent_events}");
    println!("- sent rates: {sent_rates}");
    println!("- imported events: {imported_events}");
    println!("- imported rates: {imported_rates}");
    println!("- peer imported events: {peer_imported_events}");
    println!("- peer imported rates: {peer_imported_rates}");
    Ok(())
}

fn sync_status(db: &Db, cfg: &AppConfig, sync_dir: &Path) -> Result<()> {
    let events = db.count_events().unwrap_or(0);
    let rates = db.count_rates().unwrap_or(0);

    println!("workspace\t{}", cfg.current_workspace);
    println!("device_id\t{}", cfg.device_id);
    println!("sync_dir\t{}", sync_dir.display());

    let ws_root = workspace_root(sync_dir, &cfg.current_workspace);
    let device_root = device_root(sync_dir, &cfg.current_workspace, cfg.device_id);

    println!("sync_ws_root\t{}", ws_root.display());
    println!("sync_device_root\t{}", device_root.display());
    println!("local_events\t{}", events);
    println!("local_rates\t{}", rates);

    match cfg.last_sync_at {
        Some(ts) => println!("last_sync_at\t{}", ts.to_rfc3339()),
        None => println!("last_sync_at\t<never>"),
    }

    if ws_root.exists() {
        println!("sync_ws_root_exists\ttrue");
    } else {
        println!("sync_ws_root_exists\tfalse");
    }

    Ok(())
}

fn export_local(db: &Db, cfg: &AppConfig, sync_dir: &Path) -> Result<()> {
    let dev_root = device_root(sync_dir, &cfg.current_workspace, cfg.device_id);
    ensure_dir(&dev_root)?;

    let events = db.list_events()?;
    let wire_events: Vec<WireEvent> = events
        .into_iter()
        .map(|e| WireEvent {
            id: e.event_id,
            payload: e.payload,
        })
        .collect();

    let events_path = dev_root.join("events.jsonl");
    jsonl_write(&events_path, &wire_events)
        .with_context(|| format!("Failed to write {}", events_path.display()))?;

    let rates = db.list_all_rates()?;
    let wire_rates: Vec<WireRate> = rates
        .into_iter()
        .map(|r: StoredRate| WireRate {
            provider: r.provider,
            base: r.base,
            quote: r.quote,
            as_of: r.as_of,
            rate: r.rate,
        })
        .collect();

    let rates_path = dev_root.join("rates.jsonl");
    jsonl_write(&rates_path, &wire_rates)
        .with_context(|| format!("Failed to write {}", rates_path.display()))?;

    Ok(())
}

fn import_remote(db: &Db, cfg: &AppConfig, sync_dir: &Path) -> Result<(usize, usize)> {
    let ws_root = workspace_root(sync_dir, &cfg.current_workspace);
    let devices_root = ws_root.join("devices");
    if !devices_root.exists() {
        return Ok((0, 0));
    }

    let mut imported_events = 0usize;
    let mut imported_rates = 0usize;

    for entry in fs::read_dir(&devices_root)
        .with_context(|| format!("Failed to read {}", devices_root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let events_path = path.join("events.jsonl");
        if events_path.exists() {
            for line in jsonl_read_lines(&events_path)? {
                let ev: WireEvent = serde_json::from_str(&line).with_context(|| {
                    format!(
                        "Failed to parse WireEvent line in {}: {}",
                        events_path.display(),
                        line
                    )
                })?;

                if db.insert_event_ignore(ev.id, &ev.payload)? {
                    imported_events += 1;
                }
            }
        }

        let rates_path = path.join("rates.jsonl");
        if rates_path.exists() {
            for line in jsonl_read_lines(&rates_path)? {
                let rate: WireRate = serde_json::from_str(&line).with_context(|| {
                    format!(
                        "Failed to parse WireRate line in {}: {}",
                        rates_path.display(),
                        line
                    )
                })?;

                db.set_rate(
                    &rate.provider,
                    &rate.base,
                    &rate.quote,
                    rate.as_of,
                    rate.rate,
                )?;
                imported_rates += 1;
            }
        }
    }

    Ok((imported_events, imported_rates))
}

fn sync_now(db: &Db, cfg: &AppConfig, sync_dir: &Path) -> Result<(usize, usize)> {
    ensure_dir(&sync_root(sync_dir))?;
    export_local(db, cfg, sync_dir)?;
    import_remote(db, cfg, sync_dir)
}
