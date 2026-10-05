use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process;

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

    // Security: the socket path is FIXED and root-owned in production. We must not let a child
    // override it via the environment (e.g. `GUARDIAN_SOCKET=/tmp/fake.sock yay -S game` pointing
    // at a fake daemon that always answers "allow"). The env override is honored only in debug
    // builds, for the test harness.
    const DEFAULT_SOCKET: &str = "/run/guardian/guardian.sock";
    let socket_path = if cfg!(debug_assertions) {
        std::env::var("GUARDIAN_SOCKET").unwrap_or_else(|_| DEFAULT_SOCKET.to_string())
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
        reason: None,
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
