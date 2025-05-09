use anyhow::{anyhow, Result};
use log::{error, info};
use pcap::{Capture, Device};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant, SystemTime};

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows::*;

#[cfg(target_os = "macos")]
mod macos;

/// Connection protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    TCP,
    UDP,
    // ICMP, // Variant removed as unused
    // Other(u8), // Variant removed as unused
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::TCP => write!(f, "TCP"),
            Protocol::UDP => write!(f, "UDP"),
            // Protocol::ICMP => write!(f, "ICMP"), // Variant removed
            // Protocol::Other(proto) => write!(f, "Proto({})", proto), // Variant removed
        }
    }
}

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Established,
    SynSent,
    SynReceived,
    FinWait1,
    FinWait2,
    TimeWait,
    // Closed, // Variant removed as unused
    CloseWait,
    LastAck,
    Listen,
    Closing,
    Reset, // Added Reset variant
    Unknown,
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionState::Established => write!(f, "ESTABLISHED"),
            ConnectionState::SynSent => write!(f, "SYN_SENT"),
            ConnectionState::SynReceived => write!(f, "SYN_RECEIVED"),
            ConnectionState::FinWait1 => write!(f, "FIN_WAIT_1"),
            ConnectionState::FinWait2 => write!(f, "FIN_WAIT_2"),
            ConnectionState::TimeWait => write!(f, "TIME_WAIT"),
            // ConnectionState::Closed => write!(f, "CLOSED"), // Variant removed
            ConnectionState::CloseWait => write!(f, "CLOSE_WAIT"),
            ConnectionState::LastAck => write!(f, "LAST_ACK"),
            ConnectionState::Listen => write!(f, "LISTEN"),
            ConnectionState::Closing => write!(f, "CLOSING"),
            ConnectionState::Reset => write!(f, "RESET"),
            ConnectionState::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

/// Network connection
#[derive(Debug, Clone)]
pub struct Connection {
    pub protocol: Protocol,
    pub local_addr: SocketAddr,
    pub remote_addr: SocketAddr,
    pub state: ConnectionState,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub created_at: SystemTime,
    pub last_activity: SystemTime,
}

impl Connection {
    /// Create a new connection
    pub fn new(
        protocol: Protocol,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        state: ConnectionState,
    ) -> Self {
        let now = SystemTime::now();
        Self {
            protocol,
            local_addr,
            remote_addr,
            state,
            pid: None,
            process_name: None,
            bytes_sent: 0,
            bytes_received: 0,
            packets_sent: 0,
            packets_received: 0,
            created_at: now,
            last_activity: now,
        }
    }

    /// Get connection age as duration
    pub fn age(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.created_at)
            .unwrap_or(Duration::from_secs(0))
    }

    /// Get time since last activity
    pub fn idle_time(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.last_activity)
            .unwrap_or(Duration::from_secs(0))
    }

    /// Check if connection is active (had activity in the last minute)
    pub fn is_active(&self) -> bool {
        self.idle_time() < Duration::from_secs(60)
    }
}

/// Process information
#[derive(Debug, Clone)]
pub struct Process {
    pub pid: u32,
    pub name: String,
    pub command_line: Option<String>,
    pub user: Option<String>,
    pub cpu_usage: Option<f32>,
    pub memory_usage: Option<u64>,
}

// IP location information - struct removed as unused (dependent on get_ip_location)

/// Network monitor
pub struct NetworkMonitor {
    interface: Option<String>,
    capture: Option<Capture<pcap::Active>>,
    connections: HashMap<String, Connection>,
    // geo_db: Option<maxminddb::Reader<Vec<u8>>>, // Field removed as unused (dependent on get_ip_location)
    collect_process_info: bool,
    filter_localhost: bool,
    last_packet_check: Instant,
}

impl NetworkMonitor {
    /// Create a new network monitor
    pub fn new(interface: Option<String>, filter_localhost: bool) -> Result<Self> {
        let mut capture = if let Some(iface) = &interface {
            // Open capture on specific interface
            let device = Device::list()?
                .into_iter()
                .find(|dev| dev.name == *iface)
                .ok_or_else(|| anyhow!("Interface not found: {}", iface))?;

            info!("Opening capture on interface: {}", iface);
            let cap = Capture::from_device(device)?
                .immediate_mode(true)
                .timeout(-1) // Set to non-blocking
                .snaplen(65535)
                .promisc(true)
                .open()?;

            Some(cap)
        } else {
            // Get default interface if none specified
            let device = Device::lookup()?.ok_or_else(|| anyhow!("No default device found"))?;

            info!("Opening capture on default interface: {}", device.name);
            let cap = Capture::from_device(device)?
                .immediate_mode(true)
                .timeout(-1) // Set to non-blocking
                .snaplen(65535)
                .promisc(true)
                .open()?;

            Some(cap)
        };

        // Set BPF filter to capture all TCP and UDP traffic
        if let Some(ref mut cap) = capture {
            match cap.filter("tcp or udp", true) {
                Ok(_) => info!("Applied packet filter: tcp or udp"),
                Err(e) => error!("Error setting packet filter: {}", e),
            }
        }

        // Try to load MaxMind database if available - logic removed as geo_db field is removed
        // let geo_db = std::fs::read("GeoLite2-City.mmdb")
        //     .ok()
        //     .map(|data| maxminddb::Reader::from_source(data).ok())
        //     .flatten();

        // if geo_db.is_some() {
        //     info!("Loaded MaxMind GeoIP database");
        // } else {
        //     debug!("MaxMind GeoIP database not found");
        // }

        Ok(Self {
            interface,
            capture,
            connections: HashMap::new(),
            // geo_db, // Field removed
            collect_process_info: false,
            filter_localhost,
            // Initialize last_packet_check to a time in the past
            // to ensure the first call to process_packets runs.
            last_packet_check: Instant::now() - Duration::from_millis(200),
        })
    }

    /// Set whether to collect process information for connections
    pub fn set_collect_process_info(&mut self, collect: bool) {
        self.collect_process_info = collect;
    }

    /// Get active connections
    pub fn get_connections(&mut self) -> Result<Vec<Connection>> {
        // Process packets from capture
        self.process_packets()?;

        // Get connections from system methods
        let mut connections = Vec::new();

        // Use platform-specific code to get connections
        self.get_platform_connections(&mut connections)?;

        // Add connections from packet capture
        for (_, conn) in &self.connections {
            // Check if this connection exists in the list already
            let exists = connections.iter().any(|c| {
                c.protocol == conn.protocol
                    && c.local_addr == conn.local_addr
                    && c.remote_addr == conn.remote_addr
            });

            if !exists && conn.is_active() {
                connections.push(conn.clone());
            }
        }

        // Update with processes only if flag is set
        if self.collect_process_info {
            for conn in &mut connections {
                if conn.pid.is_none() {
                    // Use the platform-specific method
                    if let Some(process) = self.get_platform_process_for_connection(conn) {
                        conn.pid = Some(process.pid);
                        conn.process_name = Some(process.name.clone());
                    }
                }
            }
        }

        // Sort connections by last activity
        connections.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));

        // Filter localhost connections if the flag is set
        if self.filter_localhost {
            connections.retain(|conn| {
                !(conn.local_addr.ip().is_loopback() && conn.remote_addr.ip().is_loopback())
            });
        }

        Ok(connections)
    }

    /// Process packets from capture
    fn process_packets(&mut self) -> Result<()> {
        // Only check packets every 100ms to avoid too frequent checks
        if self.last_packet_check.elapsed() < Duration::from_millis(100) {
            return Ok(());
        }
        self.last_packet_check = Instant::now();

        // Define a helper function to process a single packet
        // This avoids the borrowing issues
        let process_single_packet =
            |data: &[u8],
             connections: &mut HashMap<String, Connection>,
             _interface: &Option<String>| {
                // Check if it's an ethernet frame
                if data.len() < 14 {
                    return; // Too short for Ethernet
                }

                // Skip Ethernet header (14 bytes) to get to IP header
                let ip_data = &data[14..];

                // Make sure we have enough data for an IP header
                if ip_data.len() < 20 {
                    return; // Too short for IP
                }

                // Check if it's IPv4
                let version_ihl = ip_data[0];
                let version = version_ihl >> 4;
                if version != 4 {
                    return; // Not IPv4
                }

                // Extract protocol (TCP=6, UDP=17)
                let protocol = ip_data[9];

                // Extract source and destination IP
                let src_ip = IpAddr::from([ip_data[12], ip_data[13], ip_data[14], ip_data[15]]);
                let dst_ip = IpAddr::from([ip_data[16], ip_data[17], ip_data[18], ip_data[19]]);

                // Calculate IP header length
                let ihl = version_ihl & 0x0F;
                let ip_header_len = (ihl as usize) * 4;

                // Skip to TCP/UDP header
                let transport_data = &ip_data[ip_header_len..];
                if transport_data.len() < 8 {
                    return; // Too short for TCP/UDP
                }

                // Determine if packet is outgoing based on IP address
                // For now using a simple heuristic - consider private IPs as local
                let is_outgoing = match src_ip {
                    IpAddr::V4(ipv4) => {
                        let octets = ipv4.octets();
                        // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 127.0.0.0/8
                        octets[0] == 10
                            || (octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31)
                            || (octets[0] == 192 && octets[1] == 168)
                            || octets[0] == 127
                    }
                    IpAddr::V6(_) => false, // Simplification
                };

                match protocol {
                    6 => {
                        // TCP
                        if transport_data.len() < 20 {
                            return; // Too short for TCP
                        }

                        // Extract ports
                        let src_port = ((transport_data[0] as u16) << 8) | transport_data[1] as u16;
                        let dst_port = ((transport_data[2] as u16) << 8) | transport_data[3] as u16;

                        // Extract TCP flags
                        let flags = transport_data[13];

                        // Determine connection state from flags
                        let state = match flags {
                            0x02 => ConnectionState::SynSent,     // SYN
                            0x12 => ConnectionState::SynReceived, // SYN+ACK
                            0x10 => ConnectionState::Established, // ACK
                            0x01 => ConnectionState::FinWait1,    // FIN
                            0x11 => ConnectionState::FinWait2,    // FIN+ACK
                            0x04 => ConnectionState::Reset,       // RST
                            0x14 => ConnectionState::Closing,     // RST+ACK
                            _ => ConnectionState::Established,    // Default to established
                        };

                        // Determine local and remote addresses
                        let (local_addr, remote_addr) = if is_outgoing {
                            (
                                SocketAddr::new(src_ip, src_port),
                                SocketAddr::new(dst_ip, dst_port),
                            )
                        } else {
                            (
                                SocketAddr::new(dst_ip, dst_port),
                                SocketAddr::new(src_ip, src_port),
                            )
                        };

                        // Create or update connection
                        let conn_key = format!(
                            "{:?}:{}-{:?}:{}",
                            Protocol::TCP,
                            local_addr,
                            Protocol::TCP,
                            remote_addr
                        );

                        if let Some(conn) = connections.get_mut(&conn_key) {
                            conn.last_activity = SystemTime::now();
                            if is_outgoing {
                                conn.packets_sent += 1;
                                conn.bytes_sent += data.len() as u64;
                            } else {
                                conn.packets_received += 1;
                                conn.bytes_received += data.len() as u64;
                            }
                            conn.state = state;
                        } else {
                            let mut conn =
                                Connection::new(Protocol::TCP, local_addr, remote_addr, state);
                            conn.last_activity = SystemTime::now();
                            if is_outgoing {
                                conn.packets_sent += 1;
                                conn.bytes_sent += data.len() as u64;
                            } else {
                                conn.packets_received += 1;
                                conn.bytes_received += data.len() as u64;
                            }
                            connections.insert(conn_key, conn);
                        }
                    }
                    17 => {
                        // UDP
                        // Extract ports
                        let src_port = ((transport_data[0] as u16) << 8) | transport_data[1] as u16;
                        let dst_port = ((transport_data[2] as u16) << 8) | transport_data[3] as u16;

                        // Determine local and remote addresses
                        let (local_addr, remote_addr) = if is_outgoing {
                            (
                                SocketAddr::new(src_ip, src_port),
                                SocketAddr::new(dst_ip, dst_port),
                            )
                        } else {
                            (
                                SocketAddr::new(dst_ip, dst_port),
                                SocketAddr::new(src_ip, src_port),
                            )
                        };

                        // Create or update connection
                        let conn_key = format!(
                            "{:?}:{}-{:?}:{}",
                            Protocol::UDP,
                            local_addr,
                            Protocol::UDP,
                            remote_addr
                        );

                        if let Some(conn) = connections.get_mut(&conn_key) {
                            conn.last_activity = SystemTime::now();
                            if is_outgoing {
                                conn.packets_sent += 1;
                                conn.bytes_sent += data.len() as u64;
                            } else {
                                conn.packets_received += 1;
                                conn.bytes_received += data.len() as u64;
                            }
                        } else {
                            let mut conn = Connection::new(
                                Protocol::UDP,
                                local_addr,
                                remote_addr,
                                ConnectionState::Unknown,
                            );
                            conn.last_activity = SystemTime::now();
                            if is_outgoing {
                                conn.packets_sent += 1;
                                conn.bytes_sent += data.len() as u64;
                            } else {
                                conn.packets_received += 1;
                                conn.bytes_received += data.len() as u64;
                            }
                            connections.insert(conn_key, conn);
                        }
                    }
                    _ => {} // Ignore other protocols
                }
            };

        // Get packets from the capture
        if let Some(ref mut cap) = self.capture {
            // Process up to 100 packets
            for _ in 0..100 {
                match cap.next_packet() {
                    Ok(packet) => {
                        // Use the local helper function to avoid borrowing issues
                        process_single_packet(packet.data, &mut self.connections, &self.interface);
                    }
                    Err(_) => {
                        break; // No more packets or error
                    }
                }
            }
        }

        Ok(())
    }

    /// We don't need this method anymore since packet processing is done inline
    // fn process_packet(&mut self, packet: Packet) { ... }

    /// Get platform-specific process for a connection
    pub fn get_platform_process_for_connection(&self, connection: &Connection) -> Option<Process> {
        #[cfg(target_os = "linux")]
        {
            return self.get_linux_process_for_connection(connection);
        }
        #[cfg(target_os = "macos")]
        {
            // Try lsof first (more detailed)
            if let Some(process) = macos::try_lsof_command(connection) {
                return Some(process);
            }
            // Fall back to netstat (limited on macOS)
            return macos::try_netstat_command(connection);
        }
        #[cfg(target_os = "windows")]
        {
            // Try netstat
            if let Some(process) = windows::try_netstat_command(connection) {
                return Some(process);
            }
            // Fall back to API calls if we implement them
            return windows::try_windows_api(connection);
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            None
        }
    }

    /// Get platform-specific connections
    fn get_platform_connections(&mut self, connections: &mut Vec<Connection>) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            // Use Linux-specific implementation
            linux::get_platform_connections(self, connections)?;
        }
        #[cfg(target_os = "macos")]
        {
            // Use macOS-specific implementation
            macos::get_platform_connections(self, connections)?;
        }
        #[cfg(target_os = "windows")]
        {
            // Use Windows-specific implementation
            windows::get_platform_connections(self, connections)?;
        }

        Ok(())
    }

    /// Parse an address string into a SocketAddr
    fn parse_addr(&self, addr_str: &str) -> Option<std::net::SocketAddr> {
        // Handle IPv6 address format [addr]:port
        let addr_str = addr_str.trim();

        // Direct parse attempt
        if let Ok(addr) = addr_str.parse() {
            return Some(addr);
        }

        // Handle common formats
        if addr_str.contains(':') {
            // Try parsing as "addr:port"
            return addr_str.parse().ok();
        } else {
            // If only port is provided, assume 127.0.0.1:port
            if let Ok(port) = addr_str.parse::<u16>() {
                return Some(std::net::SocketAddr::new(
                    std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
                    port,
                ));
            }
        }

        None
    }
}
