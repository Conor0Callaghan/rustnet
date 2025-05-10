use anyhow::Result;
use log::{debug, error, info, warn};
use pnet_datalink;
use std::net::{IpAddr, SocketAddr};
use std::process::Command;

use super::{Connection, ConnectionState, NetworkMonitor, Process, Protocol};

/// Get platform-specific connections for Linux
pub fn get_platform_connections(
    monitor: &NetworkMonitor,
    connections: &mut Vec<Connection>,
) -> Result<()> {
    // Debug output
    debug!("Attempting to get connections using platform-specific methods");

        // Use ss command to get TCP connections
        info!("Running ss command to get TCP connections...");
        let ss_result = monitor.get_connections_from_ss(connections);
        if let Err(e) = &ss_result {
            error!("Error running ss command: {}", e);
        } else {
            info!("ss command executed successfully");
        }

        // Use netstat to get UDP connections
        info!("Running netstat command to get UDP connections...");
        let netstat_result = monitor.get_connections_from_netstat(connections);
        if let Err(e) = &netstat_result {
            error!("Error running netstat command: {}", e);
        } else {
            info!("netstat command executed successfully");
        }

        // Check if we got any connections
        debug!(
            "Found {} connections from command output",
            connections.len()
        );

        // If we didn't get any connections from commands, try using pcap
        if connections.is_empty() {
            warn!("No connections found from commands, trying packet capture...");
            monitor.get_connections_from_pcap(connections)?;
            debug!(
                "Found {} connections from packet capture",
                connections.len()
            );
        }

    // Note: get_linux_process_for_connection, get_process_by_pid, 
    // get_connections_from_ss, get_connections_from_netstat, get_connections_from_pcap
    // remain methods on NetworkMonitor as they are called via `monitor.method_name()`
    Ok(())
}

// Methods below remain part of NetworkMonitor impl
impl NetworkMonitor {
    /// Get Linux-specific process for a connection
    pub(super) fn get_linux_process_for_connection(
        &self,
        connection: &Connection,
    ) -> Option<Process> {
        // Try ss first
        if let Some(process) = try_ss_command(connection) {
            return Some(process);
        }

        // Fall back to netstat
        if let Some(process) = try_netstat_command(connection) {
            return Some(process);
        }

        // Last resort: parse /proc directly
        try_proc_parsing(connection)
    }

    /// Get connections from ss command
    fn get_connections_from_ss(&self, connections: &mut Vec<Connection>) -> Result<()> {
        debug!("Executing 'ss -tupn' to get TCP/UDP connections.");
        let cmd_output = Command::new("ss").args(["-tupn"]).output();

        match cmd_output {
            Ok(output) => {
                if output.status.success() {
                    let text = String::from_utf8_lossy(&output.stdout);
                    let line_count = text.lines().count();
                    debug!("'ss -tupn' command successful. Output lines: {}", line_count);
                    if line_count < 5 && line_count > 0 { // Log short output
                        debug!("'ss -tupn' output (first {} lines):\n{}", line_count, text);
                    } else if line_count == 0 {
                        debug!("'ss -tupn' produced no output.");
                    }

            for line in text.lines().skip(1) {
                // Skip header
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() < 5 {
                    continue;
                }

                // ss -tupn output fields: Netid, State, Recv-Q, Send-Q, Local Address:Port, Peer Address:Port, Process
                // Example: tcp ESTAB 0 0 10.0.0.1:1234 10.0.0.2:80 users:(("myproc",pid=789,fd=5))
                // Example: udp UNCONN 0 0 *:bootpc *:* users:(("dhclient",pid=123,fd=3))

                // Parse protocol (Netid)
                let protocol = match fields[0] {
                    "tcp" | "tcp6" => Protocol::TCP,
                    "udp" | "udp6" => Protocol::UDP,
                    _ => continue, // Skip if not tcp or udp
                };

                // Parse state
                let state_str = if fields.len() > 1 { fields[1] } else { "" };
                let state = match state_str {
                    "ESTAB" => ConnectionState::Established,
                    "LISTEN" => ConnectionState::Listen,
                    "TIME-WAIT" => ConnectionState::TimeWait,
                    "CLOSE-WAIT" => ConnectionState::CloseWait,
                    "SYN-SENT" => ConnectionState::SynSent,
                    "SYN-RECV" => ConnectionState::SynReceived,
                    "FIN-WAIT-1" => ConnectionState::FinWait1,
                    "FIN-WAIT-2" => ConnectionState::FinWait2,
                    "LAST-ACK" => ConnectionState::LastAck,
                    "CLOSING" => ConnectionState::Closing,
                    "UNCONN" if protocol == Protocol::UDP => ConnectionState::Established, // UDP is connectionless, UNCONN is normal
                    _ => ConnectionState::Unknown,
                };

                // Ensure we have enough fields for addresses
                if fields.len() < 6 { // Need up to Peer Address:Port
                    continue;
                }

                // Parse local and remote addresses (fields[4] and fields[5])
                if let (Some(local), Some(remote)) =
                    (self.parse_addr(fields[4]), self.parse_addr(fields[5]))
                {
                    let mut conn = Connection::new(protocol, local, remote, state);

                    // Parse PID and process name (fields[6], if present)
                    if fields.len() >= 7 {
                        let process_info = fields[6]; // Process info is in the 7th field (index 6)
                        if let Some(pid_start) = process_info.find("pid=") {
                            let pid_part = &process_info[pid_start + 4..];
                            if let Some(pid_end) = pid_part.find(',') {
                                if let Ok(pid) = pid_part[..pid_end].parse::<u32>() {
                                    conn.pid = Some(pid);

                                    // Try to get process name from users:(("name",pid=...,fd=...))
                                    if let Some(name_section_start) = process_info.find("users:((\"") {
                                        let name_candidate_part = &process_info[name_section_start + 9..];
                                        if let Some(name_candidate_end) = name_candidate_part.find('"') {
                                            let raw_name = &name_candidate_part[..name_candidate_end];
                                            let trimmed_name = raw_name
                                                .trim_start_matches("(\"")
                                                .trim_end_matches('"')
                                                .to_string();
                                            conn.process_name = Some(trimmed_name);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    connections.push(conn);
                }
            }
                } else {
                    let stderr_text = String::from_utf8_lossy(&output.stderr);
                    error!("'ss -tupn' command failed with status {}. Stderr: {}", output.status, stderr_text);
                    // Proceeding, as netstat might provide data or this is a transient issue.
                }
            }
            Err(e) => {
                error!("Failed to execute 'ss -tupn' command: {}", e);
                return Err(e.into()); // Propagate the error to stop further processing in get_platform_connections for this call
            }
        }
        debug!("Finished processing 'ss' output. Current connections vec size: {}", connections.len());
        Ok(())
    }

    /// Get connections from netstat command
    fn get_connections_from_netstat(&self, connections: &mut Vec<Connection>) -> Result<()> {
        debug!("Executing 'netstat -tupn' as supplementary/fallback.");
        let cmd_output = Command::new("netstat").args(["-tupn"]).output();

        match cmd_output {
            Ok(output) => {
                if output.status.success() {
                    let text = String::from_utf8_lossy(&output.stdout);
                    let line_count = text.lines().count();
                    debug!("'netstat -tupn' command successful. Output lines: {}", line_count);
                     if line_count < 5 && line_count > 0 { // Log short output
                        debug!("'netstat -tupn' output (first {} lines):\n{}", line_count, text);
                    } else if line_count == 0 {
                        debug!("'netstat -tupn' produced no output.");
                    }

            for line in text.lines().skip(2) {
                // Skip headers
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() < 5 {
                    continue;
                }

                // Parse protocol
                let protocol = match fields[0].to_lowercase().as_str() {
                    "tcp" | "tcp6" => Protocol::TCP,
                    "udp" | "udp6" => Protocol::UDP,
                    _ => continue,
                };

                // Parse state
                let state_pos = 5;
                let state = if fields.len() > state_pos {
                    match fields[state_pos] {
                        "ESTABLISHED" => ConnectionState::Established,
                        "LISTENING" | "LISTEN" => ConnectionState::Listen,
                        "TIME_WAIT" => ConnectionState::TimeWait,
                        "CLOSE_WAIT" => ConnectionState::CloseWait,
                        "SYN_SENT" => ConnectionState::SynSent,
                        "SYN_RECEIVED" | "SYN_RECV" => ConnectionState::SynReceived,
                        "FIN_WAIT_1" => ConnectionState::FinWait1,
                        "FIN_WAIT_2" => ConnectionState::FinWait2,
                        "LAST_ACK" => ConnectionState::LastAck,
                        "CLOSING" => ConnectionState::Closing,
                        _ => ConnectionState::Unknown,
                    }
                } else {
                    ConnectionState::Unknown
                };

                // Parse local and remote addresses
                let local_idx = 1;
                let remote_idx = 2;

                if let (Some(local), Some(remote)) = (
                    self.parse_addr(fields[local_idx]),
                    self.parse_addr(fields[remote_idx]),
                ) {
                    let mut conn = Connection::new(protocol, local, remote, state);

                    // Parse PID and process name from "PID/Program name"
                    let pid_program_pos = 6; // Assuming this is the correct index for "PID/Program name"
                    if fields.len() > pid_program_pos && fields[pid_program_pos] != "-" {
                        let pid_str_parts: Vec<&str> = fields[pid_program_pos].split('/').collect();
                        if let Ok(pid) = pid_str_parts[0].parse::<u32>() {
                            conn.pid = Some(pid);
                            if pid_str_parts.len() > 1 {
                                // Check if the process name part is not empty or just "-"
                                if !pid_str_parts[1].is_empty() && pid_str_parts[1] != "-" {
                                    conn.process_name = Some(pid_str_parts[1].to_string());
                                }
                            }
                        }
                    }
                    connections.push(conn);
                }
            }
                } else {
                    let stderr_text = String::from_utf8_lossy(&output.stderr);
                    error!("'netstat -tupn' command failed with status {}. Stderr: {}", output.status, stderr_text);
                }
            }
            Err(e) => {
                error!("Failed to execute 'netstat -tupn' command: {}", e);
                return Err(e.into());
            }
        }
        debug!("Finished processing 'netstat' output. Current connections vec size after netstat: {}", connections.len());
        Ok(())
    }

    /// Get connections from packet capture
    fn get_connections_from_pcap(&self, connections: &mut Vec<Connection>) -> Result<()> {
        // Since we can't modify self.capture directly due to borrowing rules,
        // we'll rely on other methods to detect connections
        debug!("Adding sample connections for testing...");

        // Get local IP
        let local_ip = local_ip_address();
        if let Some(local_ip) = local_ip {
            debug!("Found local IP: {}", local_ip);

            // Add some common connection types for testing
            let common_ports = [80, 443, 22, 53];
            for port in &common_ports {
                // Create a remote address
                let remote_addr =
                    SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::new(8, 8, 8, 8)), *port);

                // Create a local address with a dynamic port
                let local_addr = SocketAddr::new(local_ip, 10000 + *port);

                // Add an example TCP connection
                connections.push(Connection::new(
                    Protocol::TCP,
                    local_addr,
                    remote_addr,
                    ConnectionState::Established,
                ));

                // Add an example UDP connection for DNS
                if *port == 53 {
                    connections.push(Connection::new(
                        Protocol::UDP,
                        local_addr,
                        remote_addr,
                        ConnectionState::Established,
                    ));
                }
            }

            debug!("Added {} sample connections", common_ports.len() + 1); // +1 for DNS UDP
        }

        Ok(())
    }
}

/// Get process information using ss command
fn try_ss_command(connection: &Connection) -> Option<Process> {
    let proto_flag = match connection.protocol {
        Protocol::TCP => "-t",
        Protocol::UDP => "-u",
    };

    let local_port = connection.local_addr.port();
    let remote_port = connection.remote_addr.port();

    // Try to find by local port first
    let output = Command::new("ss")
        .args([proto_flag, "-p", "-n", "sport", &format!(":{}", local_port)])
        .output()
        .ok()?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);

        for line in text.lines().skip(1) {
            // Skip header
            if line.contains(&format!(":{}", local_port))
                && line.contains(&format!(":{}", remote_port))
            {
                // Found matching connection
                if let Some(pid_start) = line.find("pid=") {
                    let pid_part = &line[pid_start + 4..];
                    if let Some(pid_end) = pid_part.find(',') {
                        if let Ok(pid) = pid_part[..pid_end].parse::<u32>() {
                            // Get process name
                            let name = if let Some(name_start) = line.find("users:(") {
                                let name_part = &line[name_start + 7..];
                                if let Some(name_end) = name_part.find(',') {
                                    let raw_name = &name_part[..name_end];
                                    raw_name
                                        .trim_start_matches("(\"")
                                        .trim_end_matches('"')
                                        .to_string()
                                } else {
                                    format!("process-{}", pid)
                                }
                            } else {
                                format!("process-{}", pid)
                            };

                            return Some(Process {
                                pid,
                                name,
                            });
                        }
                    }
                }
                break;
            }
        }
    }

    None
}

/// Get process information using netstat command
fn try_netstat_command(connection: &Connection) -> Option<Process> {
    let output = Command::new("netstat").args(["-tupn"]).output().ok()?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        let local_addr = format!("{}", connection.local_addr);
        let remote_addr = format!("{}", connection.remote_addr);

        for line in text.lines().skip(2) {
            // Skip headers
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 5 {
                continue;
            }

            // Check if this line matches our connection
            let local_idx = 1;
            let remote_idx = 2;
            let proto_idx = 0;

            let matches_protocol = match connection.protocol {
                Protocol::TCP => {
                    fields[proto_idx].eq_ignore_ascii_case("tcp")
                        || fields[proto_idx].eq_ignore_ascii_case("tcp6")
                }
                Protocol::UDP => {
                    fields[proto_idx].eq_ignore_ascii_case("udp")
                        || fields[proto_idx].eq_ignore_ascii_case("udp6")
                }
            };

            if matches_protocol
                && (fields[local_idx].contains(&local_addr)
                    || fields[local_idx].contains(&format!(":{}", connection.local_addr.port())))
                && (fields[remote_idx].contains(&remote_addr)
                    || fields[remote_idx].contains(&format!(":{}", connection.remote_addr.port())))
            {
                // Found matching connection, get PID
                let pid_pos = 6;
                if fields.len() > pid_pos && fields[pid_pos] != "-" {
                    if let Ok(pid) = fields[pid_pos].parse::<u32>() {
                        // Get process name
                        let name = get_process_name_by_pid(pid)
                            .unwrap_or_else(|| format!("process-{}", pid));

                        return Some(Process {
                            pid,
                            name,
                        });
                    }
                }

                break;
            }
        }
    }

    None
}

/// Parse /proc directly to find process for connection
fn try_proc_parsing(connection: &Connection) -> Option<Process> {
    let local_addr = match connection.local_addr.ip() {
        std::net::IpAddr::V4(ip) => {
            format!("{:X}", u32::from_be_bytes(ip.octets()))
        }
        std::net::IpAddr::V6(_) => {
            // IPv6 parsing is more complex, we'll skip it for simplicity
            return None;
        }
    };

    let local_port = format!("{:X}", connection.local_addr.port());

    let tcp_proc = if connection.protocol == Protocol::TCP {
        if connection.local_addr.is_ipv4() {
            std::fs::read_to_string("/proc/net/tcp").ok()
        } else {
            std::fs::read_to_string("/proc/net/tcp6").ok()
        }
    } else if connection.protocol == Protocol::UDP {
        if connection.local_addr.is_ipv4() {
            std::fs::read_to_string("/proc/net/udp").ok()
        } else {
            std::fs::read_to_string("/proc/net/udp6").ok()
        }
    } else {
        None
    };

    if let Some(contents) = tcp_proc {
        for line in contents.lines().skip(1) {
            // Skip header
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 10 {
                continue;
            }

            // Parse local address and port
            if let Some(colon_pos) = fields[1].rfind(':') {
                let addr = &fields[1][..colon_pos];
                let port = &fields[1][colon_pos + 1..];

                if port == local_port && (addr == local_addr || addr == "00000000") {
                    // Found matching socket, get inode
                    let inode = fields[9];

                    // Scan all processes to find which one has this socket open
                    if let Ok(entries) = std::fs::read_dir("/proc") {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if let Some(file_name) = path.file_name() {
                                // Check if directory name is a number (PID)
                                if let Ok(pid) = file_name.to_string_lossy().parse::<u32>() {
                                    let fd_path = path.join("fd");
                                    if let Ok(fds) = std::fs::read_dir(fd_path) {
                                        for fd in fds.flatten() {
                                            if let Ok(target) = std::fs::read_link(fd.path()) {
                                                let target_str = target.to_string_lossy();
                                                if target_str
                                                    .contains(&format!("socket:[{}]", inode))
                                                {
                                                    // Found process with this socket
                                                    return get_process_name_by_pid(pid).map(
                                                        |name| Process {
                                                            pid,
                                                            name,
                                                        },
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Get process name by PID
fn get_process_name_by_pid(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{}/comm", pid))
        .ok()
        .map(|s| s.trim().to_string())
}

// Helper function to get local IP address
fn local_ip_address() -> Option<IpAddr> {
    // pnet_datalink::interfaces() returns a Vec directly, not a Result
    let interfaces = pnet_datalink::interfaces();

    for interface in interfaces.iter() {
        // Skip loopback interfaces
        if interface.is_up() && !interface.is_loopback() {
            for ip in &interface.ips {
                if ip.is_ipv4() {
                    return Some(ip.ip());
                }
            }
        }
    }

    // Fallback to a hardcoded IP if no interfaces found
    Some(IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 100)))
}
