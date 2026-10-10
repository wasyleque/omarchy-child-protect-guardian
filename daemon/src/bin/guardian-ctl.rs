//! `guardian-ctl` — local control client (Stage 1). Implementation delegated.

use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::exit;

use base64::Engine as _;
use guardian::ipc::{ClientMessage, ServerMessage};
use guardian::request::{Decision, InstallSource};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: guardian-ctl <command> [arguments...]");
        eprintln!("Commands:");
        eprintln!("  list              - List pending requests");
        eprintln!("  allow <id>        - Allow a request");
        eprintln!("  deny <id>         - Deny a request");
        eprintln!("  request <source> <package> [reason] - Submit a new request");
        eprintln!("  alert <message>   - Send a tamper/integrity alert to the parent");
        eprintln!("  schedule   - Show screen-time status");
        eprintln!("  pair --topic <t> [--server <u>] [--app <u>]  - Show a pairing QR");
        eprintln!("  audit-verify <path>   - Verify the audit log chain");
        exit(2);
    }
    
    // Handle audit-verify command early
    if args[1] == "audit-verify" {
        let path = match args.get(2) {
            Some(p) => p,
            None => {
                eprintln!("Usage: guardian-ctl audit-verify <path>");
                exit(2);
            }
        };
        
        match guardian::audit::verify(std::path::Path::new(path)) {
            Ok(n) => {
                println!("audit OK: {} entries", n);
                return Ok(());
            }
            Err(e) => {
                eprintln!("audit FAILED: {}", e);
                exit(1);
            }
        }
    }
    
    // Handle pair command early
    if args[1] == "pair" {
        let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
        let server = flag("--server").unwrap_or_else(|| "https://ntfy.sh".to_string());
        let app = flag("--app").unwrap_or_else(|| "https://wasyleque.github.io/omarchy-child-protect-guardian/parent-app/".to_string());
        let topic = match flag("--topic") {
            Some(t) => t,
            None => { eprintln!("Usage: guardian-ctl pair --topic <request_topic> [--server <url>] [--app <url>]"); exit(2); }
        };
        // Include the daemon's public key so the app can verify published challenges (reject fakes).
        let dpub = flag("--daemon-pubkey")
            .or_else(|| std::fs::read_to_string("/etc/guardian/daemon.pub").ok().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty());
        let payload = match &dpub {
            Some(d) => serde_json::json!({"server": server, "topic": topic, "dpub": d}).to_string(),
            None => serde_json::json!({"server": server, "topic": topic}).to_string(),
        };
        let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let url = format!("{}#pair={}", app, enc);
        match qrcode::QrCode::new(url.as_bytes()) {
            Ok(code) => { println!("{}", code.render::<qrcode::render::unicode::Dense1x2>().quiet_zone(true).build()); }
            Err(e) => eprintln!("QR error: {}", e),
        }
        println!("Scan with the parent phone, or open:\n{}", url);
        return Ok(());
    }
    
    // `request` submits an install → the SUBMIT socket (any uid). Everything else is a control
    // action → the owner-only CONTROL socket. SECURITY: the socket paths are FIXED in release builds;
    // the env overrides (for the test harness) are honored only in debug, so a child can't point a
    // wrapper's `guardian-ctl request` at a fake always-allow socket via GUARDIAN_SUBMIT_SOCKET.
    const CONTROL_DEFAULT: &str = "/run/guardian/guardian.sock";
    const SUBMIT_DEFAULT: &str = "/run/guardian/submit.sock";
    let socket_path = if cfg!(debug_assertions) {
        if args[1] == "request" {
            env::var("GUARDIAN_SUBMIT_SOCKET").unwrap_or_else(|_| SUBMIT_DEFAULT.to_string())
        } else {
            env::var("GUARDIAN_SOCKET").unwrap_or_else(|_| CONTROL_DEFAULT.to_string())
        }
    } else if args[1] == "request" {
        SUBMIT_DEFAULT.to_string()
    } else {
        CONTROL_DEFAULT.to_string()
    };

    let mut stream = UnixStream::connect(socket_path)?;

    match args[1].as_str() {
        "list" => {
            send_message(&mut stream, ClientMessage::List)?;
            let response = read_response(&mut stream)?;
            
            if let ServerMessage::Pending { requests } = response {
                if requests.is_empty() {
                    println!("no pending requests");
                } else {
                    for req in requests {
                        let reason = req.reason.as_deref().unwrap_or("(no reason)");
                        println!("{}  {:?}  {}  {}", req.id, req.source, req.package, reason);
                    }
                }
            } else {
                eprintln!("Unexpected response");
                exit(1);
            }
        }
        "allow" | "deny" => {
            if args.len() < 3 {
                eprintln!("Usage: guardian-ctl {} <id>", args[1]);
                exit(2);
            }
            
            let id = match uuid::Uuid::parse_str(&args[2]) {
                Ok(id) => id,
                Err(_) => {
                    eprintln!("invalid request id (expected a UUID)");
                    exit(1);
                }
            };

            let decision = if args[1] == "allow" {
                Decision::Allow
            } else {
                Decision::Deny
            };

            send_message(&mut stream, ClientMessage::Resolve { id, decision })?;
            let response = read_response(&mut stream)?;
            
            if let ServerMessage::Resolved { ok, .. } = response {
                if ok {
                    println!("resolved");
                } else {
                    println!("no such request");
                }
            } else {
                eprintln!("Unexpected response");
                exit(1);
            }
        }
        "request" => {
            if args.len() < 4 {
                eprintln!("Usage: guardian-ctl request <source> <package> [reason]");
                exit(2);
            }
            
            let source_str = &args[2];
            let package = &args[3];
            
            let reason = if args.len() > 4 {
                Some(args[4..].join(" "))
            } else {
                None
            };
            
            let source = source_str.parse::<InstallSource>()
                .map_err(|_| format!("Invalid install source: {}", source_str))?;
            
            send_message(&mut stream, ClientMessage::Submit { source, package: package.to_string(), reason })?;
            let response = read_response(&mut stream)?;
            
            if let ServerMessage::Decision { decision, .. } = response {
                match decision {
                    Decision::Allow => println!("ALLOW"),
                    Decision::Deny => println!("DENY"),
                }
            } else {
                eprintln!("Unexpected response");
                exit(1);
            }
        }
        "alert" => {
            let message = if args.len() > 2 {
                args[2..].join(" ")
            } else {
                eprintln!("Usage: guardian-ctl alert <message>");
                exit(2);
            };
            send_message(&mut stream, ClientMessage::Alert { message })?;
            match read_response(&mut stream)? {
                ServerMessage::Alerted { ok } => {
                    println!("{}", if ok { "alert sent" } else { "alert recorded (no remote configured)" });
                }
                _ => {
                    eprintln!("Unexpected response");
                    exit(1);
                }
            }
        }
        "schedule" => {
            send_message(&mut stream, ClientMessage::ScheduleStatus)?;
            match read_response(&mut stream)? {
                ServerMessage::Schedule { status, remaining_secs } => match remaining_secs {
                    Some(s) => println!("{status} ({} min remaining today)", s / 60),
                    None => println!("{status}"),
                },
                _ => {
                    eprintln!("Unexpected response");
                    exit(1);
                }
            }
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            eprintln!("Usage: guardian-ctl <command> [arguments...]");
            eprintln!("Commands:");
            eprintln!("  list              - List pending requests");
            eprintln!("  allow <id>        - Allow a request");
            eprintln!("  deny <id>         - Deny a request");
            eprintln!("  request <source> <package> [reason] - Submit a new request");
        eprintln!("  alert <message>   - Send a tamper/integrity alert to the parent");
        eprintln!("  schedule   - Show screen-time status");
            eprintln!("  pair --topic <t> [--server <u>] [--app <u>]  - Show a pairing QR");
            eprintln!("  audit-verify <path>   - Verify the audit log chain");
            exit(2);
        }
    }
    
    Ok(())
}

fn send_message(stream: &mut UnixStream, message: ClientMessage) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(&message)?;
    writeln!(stream, "{}", json)?;
    stream.flush()?;
    Ok(())
}

fn read_response(stream: &mut UnixStream) -> Result<ServerMessage, Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    
    reader.read_line(&mut line)?;
    
    if line.is_empty() {
        return Err("Empty response".into());
    }
    
    // Remove the trailing newline
    if line.ends_with('\n') {
        line.pop();
    }
    
    let response: ServerMessage = serde_json::from_str(&line)?;
    Ok(response)
}
