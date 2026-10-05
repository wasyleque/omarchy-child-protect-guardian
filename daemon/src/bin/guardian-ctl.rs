//! `guardian-ctl` — local control client (Stage 1). Implementation delegated.

use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::exit;

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
        exit(2);
    }
    
    let socket_path = env::var("GUARDIAN_SOCKET").unwrap_or_else(|_| "/run/guardian/guardian.sock".to_string());
    
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
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            eprintln!("Usage: guardian-ctl <command> [arguments...]");
            eprintln!("Commands:");
            eprintln!("  list              - List pending requests");
            eprintln!("  allow <id>        - Allow a request");
            eprintln!("  deny <id>         - Deny a request");
            eprintln!("  request <source> <package> [reason] - Submit a new request");
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
