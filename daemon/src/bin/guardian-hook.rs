use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process;

/// Best-effort: the actual command that triggered this hook (pacman/yay/paru), read from the parent
/// process. Shown to the parent as the "full command" for advanced review. The daemon signs it into
/// the challenge (via `reason`), so a broker can't rewrite it.
fn invoking_command() -> Option<String> {
    // /proc/self/stat: field 4 is ppid (fields after the possibly-space-containing comm in parens).
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after_comm = stat.rsplit_once(')')?.1; // skip "(comm)" which may contain spaces
    let ppid = after_comm.split_whitespace().nth(1)?; // state, ppid, ...
    let raw = std::fs::read(format!("/proc/{ppid}/cmdline")).ok()?;
    if raw.is_empty() {
        return None;
    }
    let cmd: String = raw
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s))
        .collect::<Vec<_>>()
        .join(" ");
    let cmd = cmd.trim().to_string();
    if cmd.is_empty() {
        return None;
    }
    // Keep it sane for a phone notification (truncate by chars, never mid-UTF-8).
    if cmd.chars().count() > 256 {
        Some(format!("{}…", cmd.chars().take(256).collect::<String>()))
    } else {
        Some(cmd)
    }
}

fn main() {
    // Read all lines from stdin
    let mut targets = Vec::new();
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.expect("Failed to read from stdin");
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            targets.push(trimmed.to_string());
        }
    }

    // If no targets, exit successfully
    if targets.is_empty() {
        process::exit(0);
    }

    // Security: the socket path is FIXED in production. We must not let a child override it via the
    // environment (e.g. `GUARDIAN_SUBMIT_SOCKET=/tmp/fake.sock yay -S game` pointing at a fake daemon
    // that always answers "allow"). The env override is honored only in debug builds, for tests.
    const DEFAULT_SOCKET: &str = "/run/guardian/submit.sock";
    let socket_path = if cfg!(debug_assertions) {
        std::env::var("GUARDIAN_SUBMIT_SOCKET").unwrap_or_else(|_| DEFAULT_SOCKET.to_string())
    } else {
        DEFAULT_SOCKET.to_string()
    };

    // Connect to the socket
    let stream = match UnixStream::connect(&socket_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("guardian-hook: cannot reach guardiand ({}) - denying install", e);
            process::exit(1);
        }
    };

    // Build and send the message
    let client_message = guardian::ipc::ClientMessage::Submit {
        source: guardian::request::InstallSource::Pacman,
        package: targets.join(", "),
        reason: invoking_command(),
    };
    
    let json_message = serde_json::to_string(&client_message).expect("Failed to serialize message");
    let mut writer = std::io::BufWriter::new(stream);
    writeln!(writer, "{}", json_message).expect("Failed to write to socket");
    writer.flush().expect("Failed to flush socket");

    // Read the response
    let stream = writer.into_inner().expect("Failed to get stream from writer");
    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    
    match reader.read_line(&mut response_line) {
        Ok(0) => {
            eprintln!("guardian-hook: unexpected EOF from guardiand");
            process::exit(1);
        }
        Ok(_) => {
            // Parse the response
            match serde_json::from_str::<guardian::ipc::ServerMessage>(response_line.trim()) {
                Ok(server_message) => {
                    match server_message {
                        guardian::ipc::ServerMessage::Decision { decision, .. } => {
                            match decision {
                                guardian::request::Decision::Allow => process::exit(0),
                                guardian::request::Decision::Deny => {
                                    eprintln!("guardian-hook: install denied by parent");
                                    process::exit(1);
                                }
                            }
                        }
                        _ => {
                            eprintln!("guardian-hook: unexpected response from guardiand");
                            process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("guardian-hook: failed to parse response from guardiand: {}", e);
                    process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("guardian-hook: failed to read response from guardiand: {}", e);
            process::exit(1);
        }
    }
}
