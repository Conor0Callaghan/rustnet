use std::collections::BTreeMap;
use std::fmt;
use std::net::SocketAddr;
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    TCP,
    UDP,
    ICMP,
    ARP,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::TCP => write!(f, "TCP"),
            Protocol::UDP => write!(f, "UDP"),
            Protocol::ICMP => write!(f, "ICMP"),
            Protocol::ARP => write!(f, "ARP"),
        }
    }
}

impl std::fmt::Display for ApplicationProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplicationProtocol::Http(info) => {
                if let Some(host) = &info.host {
                    write!(f, "HTTP ({})", host)
                } else {
                    write!(f, "HTTP")
                }
            }
            ApplicationProtocol::Https(info) => {
                if info.tls_info.is_none() {
                    write!(f, "HTTPS")
                } else {
                    let info = info.tls_info.as_ref().unwrap();
                    // If SNI is available, include it in the display
                    if let Some(sni) = &info.sni {
                        write!(f, "HTTPS ({})", sni)
                    } else {
                        write!(f, "HTTPS")
                    }
                }
            }
            ApplicationProtocol::Dns(info) => {
                if let Some(query) = &info.query_name {
                    write!(f, "DNS ({})", query)
                } else {
                    write!(f, "DNS")
                }
            }
            ApplicationProtocol::Ssh => write!(f, "SSH"),
            ApplicationProtocol::Quic(info) => {
                if let Some(tls_info) = &info.tls_info {
                    if let Some(sni) = &tls_info.sni {
                        write!(f, "QUIC ({})", sni)
                    } else {
                        write!(f, "QUIC")
                    }
                } else {
                    write!(f, "QUIC")
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    #[allow(dead_code)]
    // Listening is not used in our model because we track connections after they are established
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    #[allow(dead_code)]
    CloseWait,
    #[allow(dead_code)]
    LastAck,
    #[allow(dead_code)]
    TimeWait,
    Closing,
    Closed,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub enum ProtocolState {
    Tcp(TcpState),
    Udp,
    Icmp {
        icmp_type: u8,
        #[allow(dead_code)]
        icmp_code: u8,
    },
    Arp {
        operation: ArpOperation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArpOperation {
    Request,
    Reply,
}

#[derive(Debug, Clone)]
pub enum ApplicationProtocol {
    Http(HttpInfo),
    Https(HttpsInfo),
    Dns(DnsInfo),
    Ssh,
    Quic(QuicInfo),
}

#[derive(Debug, Clone)]
pub struct HttpInfo {
    pub version: HttpVersion,
    pub method: Option<String>,
    pub host: Option<String>,
    pub path: Option<String>,
    pub status_code: Option<u16>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpVersion {
    Http10,
    Http11,
    Http2,
}

#[derive(Debug, Clone)]
pub struct HttpsInfo {
    pub tls_info: Option<TlsInfo>,
}

#[derive(Debug, Clone)]
pub struct TlsInfo {
    pub version: Option<TlsVersion>,
    pub sni: Option<String>,
    pub alpn: Vec<String>,
    pub cipher_suite: Option<u16>,
}

impl TlsInfo {
    pub fn new() -> Self {
        Self {
            version: None,
            sni: None,
            alpn: Vec::new(),
            cipher_suite: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsVersion {
    #[allow(dead_code)]
    Ssl3,
    Tls10,
    Tls11,
    Tls12,
    Tls13,
}

impl fmt::Display for TlsVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TlsVersion::Ssl3 => write!(f, "SSL 3.0"),
            TlsVersion::Tls10 => write!(f, "TLS 1.0"),
            TlsVersion::Tls11 => write!(f, "TLS 1.1"),
            TlsVersion::Tls12 => write!(f, "TLS 1.2"),
            TlsVersion::Tls13 => write!(f, "TLS 1.3"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DnsInfo {
    pub query_name: Option<String>,
    pub query_type: Option<DnsQueryType>,
    #[allow(dead_code)]
    pub response_ips: Vec<std::net::IpAddr>,
    pub is_response: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsQueryType {
    A,          // 1
    NS,         // 2
    CNAME,      // 5
    SOA,        // 6
    PTR,        // 12
    HINFO,      // 13
    MX,         // 15
    TXT,        // 16
    RP,         // 17
    AFSDB,      // 18
    SIG,        // 24
    KEY,        // 25
    AAAA,       // 28
    LOC,        // 29
    SRV,        // 33
    NAPTR,      // 35
    KX,         // 36
    CERT,       // 37
    DNAME,      // 39
    APL,        // 42
    DS,         // 43
    SSHFP,      // 44
    IPSECKEY,   // 45
    RRSIG,      // 46
    NSEC,       // 47
    DNSKEY,     // 48
    DHCID,      // 49
    NSEC3,      // 50
    NSEC3PARAM, // 51
    TLSA,       // 52
    SMIMEA,     // 53
    HIP,        // 55
    CDS,        // 59
    CDNSKEY,    // 60
    OPENPGPKEY, // 61
    CSYNC,      // 62
    ZONEMD,     // 63
    SVCB,       // 64
    HTTPS,      // 65
    EUI48,      // 108
    EUI64,      // 109
    TKEY,       // 249
    TSIG,       // 250
    URI,        // 256
    CAA,        // 257
    TA,         // 32768
    DLV,        // 32769
    Other(u16), // For any other type
}

// QUIC-specific types
#[derive(Debug, Clone)]
pub struct QuicInfo {
    pub version_string: Option<String>,
    pub packet_type: QuicPacketType,
    pub connection_id: Vec<u8>,
    pub connection_id_hex: Option<String>,
    pub connection_state: QuicConnectionState,
    pub tls_info: Option<TlsInfo>, // Extracted TLS handshake info
    pub has_crypto_frame: bool,    // Whether packet contains CRYPTO frame
    pub crypto_reassembler: Option<CryptoFrameReassembler>,
}

impl QuicInfo {
    pub fn new(version: u32) -> Self {
        Self {
            version_string: quic_version_to_string(version),
            connection_id_hex: None,
            packet_type: QuicPacketType::Unknown,
            connection_id: Vec::new(),
            connection_state: QuicConnectionState::Unknown,
            tls_info: None,
            has_crypto_frame: false,
            crypto_reassembler: None,
        }
    }
    /// Initialize reassembler if needed
    pub fn ensure_reassembler(&mut self) {
        if self.crypto_reassembler.is_none() {
            self.crypto_reassembler = Some(CryptoFrameReassembler::new());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuicPacketType {
    Initial,
    ZeroRtt,
    Handshake,
    Retry,
    VersionNegotiation,
    OneRtt, // Short header
    Unknown,
}

impl fmt::Display for QuicPacketType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuicPacketType::Initial => write!(f, "Initial"),
            QuicPacketType::ZeroRtt => write!(f, "0-RTT"),
            QuicPacketType::Handshake => write!(f, "Handshake"),
            QuicPacketType::Retry => write!(f, "Retry"),
            QuicPacketType::VersionNegotiation => write!(f, "Version Negotiation"),
            QuicPacketType::OneRtt => write!(f, "1-RTT"),
            QuicPacketType::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuicConnectionState {
    Initial,
    Handshaking,
    Connected,
    Draining,
    Closed,
    Unknown,
}

impl fmt::Display for QuicConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuicConnectionState::Initial => write!(f, "Initial"),
            QuicConnectionState::Handshaking => write!(f, "Handshaking"),
            QuicConnectionState::Connected => write!(f, "Connected"),
            QuicConnectionState::Draining => write!(f, "Draining"),
            QuicConnectionState::Closed => write!(f, "Closed"),
            QuicConnectionState::Unknown => write!(f, "Unknown"),
        }
    }
}

fn quic_version_to_string(version: u32) -> Option<String> {
    match version {
        0x00000001 => Some("v1".to_string()),
        0x6b3343cf => Some("v2".to_string()),
        0xff00001d => Some("draft-29".to_string()),
        0xff00001c => Some("draft-28".to_string()),
        0xff00001b => Some("draft-27".to_string()),
        0x51303530 => Some("Q050".to_string()),
        0x54303530 => Some("T050".to_string()),
        v if (v >> 8) == 0xff0000 => Some(format!("draft-{}", v & 0xff)),
        _ => None,
    }
}

/// Tracks CRYPTO frame fragments for reassembly
/// This is part of the QuicInfo data model, even though it's used by DPI
#[derive(Debug, Clone)]
pub struct CryptoFrameReassembler {
    /// Fragments indexed by offset - using BTreeMap for ordered iteration
    fragments: BTreeMap<u64, Vec<u8>>,

    /// Highest contiguous byte we've reassembled from offset 0
    contiguous_offset: u64,

    /// Whether we've successfully extracted complete TLS info
    has_complete_tls_info: bool,

    /// Cached TLS info once we've extracted it
    cached_tls_info: Option<TlsInfo>,

    /// Maximum total size we'll buffer (prevent memory exhaustion)
    max_buffer_size: usize,

    /// Current total buffered size
    current_buffer_size: usize,

    /// Timestamp of last update (for cleanup of stale fragments)
    last_update: Instant,
}

impl Default for CryptoFrameReassembler {
    fn default() -> Self {
        Self::new()
    }
}

impl CryptoFrameReassembler {
    pub fn new() -> Self {
        Self {
            fragments: BTreeMap::new(),
            contiguous_offset: 0,
            has_complete_tls_info: false,
            cached_tls_info: None,
            max_buffer_size: 64 * 1024, // 64KB max buffer
            current_buffer_size: 0,
            last_update: Instant::now(),
        }
    }

    /// Add a new CRYPTO frame fragment
    pub fn add_fragment(&mut self, offset: u64, data: Vec<u8>) -> Result<(), &'static str> {
        // Check if this would exceed our buffer limit
        if self.current_buffer_size + data.len() > self.max_buffer_size {
            return Err("Fragment would exceed maximum buffer size");
        }

        self.last_update = Instant::now();

        // Check for overlapping fragments
        let data_end = offset + data.len() as u64;

        // Handle overlaps by keeping the existing data (first write wins)
        for (&frag_offset, frag_data) in &self.fragments {
            let frag_end = frag_offset + frag_data.len() as u64;

            // Check for exact duplicate
            if offset == frag_offset && data_end == frag_end {
                return Ok(());
            }

            // Check for overlap
            if offset < frag_end && data_end > frag_offset {
                return Ok(());
            }
        }

        // Add the fragment
        self.current_buffer_size += data.len();
        self.fragments.insert(offset, data);

        // Try to advance contiguous offset
        self.update_contiguous_offset();

        Ok(())
    }

    /// Update the highest contiguous offset we've seen
    fn update_contiguous_offset(&mut self) {
        let mut current = self.contiguous_offset;

        for (&offset, data) in &self.fragments {
            if offset <= current {
                let fragment_end = offset + data.len() as u64;
                if fragment_end > current {
                    current = fragment_end;
                }
            } else if offset > current {
                break;
            }
        }

        self.contiguous_offset = current;
    }

    /// Get all contiguous data from offset 0
    pub fn get_contiguous_data(&self) -> Option<Vec<u8>> {
        if self.contiguous_offset == 0 {
            return None;
        }

        let mut result = Vec::with_capacity(self.contiguous_offset as usize);
        let mut current_offset = 0u64;

        for (&offset, data) in &self.fragments {
            if offset <= current_offset {
                let skip = (current_offset - offset) as usize;
                if skip < data.len() {
                    result.extend_from_slice(&data[skip..]);
                    current_offset = offset + data.len() as u64;
                }
            }

            if current_offset >= self.contiguous_offset {
                break;
            }
        }

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Check if fragments are stale (no updates for > 30 seconds)
    pub fn is_stale(&self) -> bool {
        self.last_update.elapsed().as_secs() > 30
    }

    /// Clear all fragments (useful after successful extraction or timeout)
    pub fn clear_fragments(&mut self) {
        self.fragments.clear();
        self.current_buffer_size = 0;
    }

    /// Mark as having complete TLS info
    pub fn set_complete_tls_info(&mut self, tls_info: TlsInfo) {
        self.has_complete_tls_info = true;
        self.cached_tls_info = Some(tls_info);
    }

    /// Get cached TLS info if complete
    pub fn get_cached_tls_info(&self) -> Option<&TlsInfo> {
        if self.has_complete_tls_info {
            self.cached_tls_info.as_ref()
        } else {
            None
        }
    }

    /// Get a reference to the fragments for merging purposes
    /// Returns an immutable reference to the internal fragments map
    pub fn get_fragments(&self) -> &BTreeMap<u64, Vec<u8>> {
        &self.fragments
    }
}

#[derive(Debug, Clone)]
pub struct DpiInfo {
    pub application: ApplicationProtocol,
    #[allow(dead_code)]
    pub first_packet_time: Instant,
    #[allow(dead_code)]
    pub last_update_time: Instant,
}

#[derive(Debug, Clone)]
pub struct RateInfo {
    #[allow(dead_code)]
    pub incoming_bps: f64,
    #[allow(dead_code)]
    pub outgoing_bps: f64,
    #[allow(dead_code)]
    pub last_calculation: Instant,
}

impl Default for RateInfo {
    fn default() -> Self {
        Self {
            incoming_bps: 0.0,
            outgoing_bps: 0.0,
            last_calculation: Instant::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Connection {
    // Core identification
    pub protocol: Protocol,
    pub local_addr: SocketAddr,
    pub remote_addr: SocketAddr,
    pub remote_host: Option<String>,

    // Protocol state
    pub protocol_state: ProtocolState,

    // Process information
    pub pid: Option<u32>,
    pub process_name: Option<String>,

    // Traffic statistics
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,

    // Timing
    pub created_at: SystemTime,
    pub last_activity: SystemTime,

    // Service identification
    pub service_name: Option<String>,

    // Deep packet inspection
    pub dpi_info: Option<DpiInfo>,

    // Performance metrics
    #[allow(dead_code)]
    // TODO: implement proper bits per second rate tracking
    pub current_rate_bps: RateInfo,
    #[allow(dead_code)]
    // TODO: implement RTT estimation
    pub rtt_estimate: Option<Duration>,

    // Backward compatibility fields
    pub current_incoming_rate_bps: f64,
    pub current_outgoing_rate_bps: f64,
}

impl Connection {
    /// Create a new connection
    pub fn new(
        protocol: Protocol,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        state: ProtocolState,
    ) -> Self {
        let now = SystemTime::now();
        Self {
            protocol,
            local_addr,
            remote_addr,
            remote_host: None,
            protocol_state: state,
            pid: None,
            process_name: None,
            bytes_sent: 0,
            bytes_received: 0,
            packets_sent: 0,
            packets_received: 0,
            created_at: now,
            last_activity: now,
            service_name: None,
            dpi_info: None,
            current_rate_bps: RateInfo::default(),
            rtt_estimate: None,
            current_incoming_rate_bps: 0.0,
            current_outgoing_rate_bps: 0.0,
        }
    }

    /// Generate a unique key for this connection
    pub fn key(&self) -> String {
        format!(
            "{:?}:{}-{:?}:{}",
            self.protocol, self.local_addr, self.protocol, self.remote_addr
        )
    }

    /// Check if connection is active (had activity in the last minute)
    pub fn is_active(&self) -> bool {
        self.last_activity.elapsed().unwrap_or_default() < Duration::from_secs(300)
    }

    /// Get the age of the connection
    #[allow(dead_code)]
    pub fn age(&self) -> Duration {
        self.created_at.elapsed().unwrap_or_default()
    }

    /// Get time since last activity
    #[allow(dead_code)]
    pub fn idle_time(&self) -> Duration {
        self.last_activity.elapsed().unwrap_or_default()
    }

    /// Get display state
    pub fn state(&self) -> String {
        match &self.protocol_state {
            ProtocolState::Tcp(tcp_state) => format!("{:?}", tcp_state),
            ProtocolState::Udp => "ACTIVE".to_string(),
            ProtocolState::Icmp { icmp_type, .. } => match icmp_type {
                8 => "ECHO_REQUEST".to_string(),
                0 => "ECHO_REPLY".to_string(),
                3 => "DEST_UNREACH".to_string(),
                11 => "TIME_EXCEEDED".to_string(),
                _ => "UNKNOWN".to_string(),
            },
            ProtocolState::Arp { operation } => match operation {
                ArpOperation::Request => "ARP_REQUEST".to_string(),
                ArpOperation::Reply => "ARP_REPLY".to_string(),
            },
        }
    }

    /// Update transfer rates
    #[allow(dead_code)]
    pub fn update_rates(&mut self, new_sent: u64, new_received: u64) {
        let now = Instant::now();
        let elapsed = now
            .duration_since(self.current_rate_bps.last_calculation)
            .as_secs_f64();

        if elapsed > 0.1 {
            let sent_diff = new_sent.saturating_sub(self.bytes_sent) as f64;
            let recv_diff = new_received.saturating_sub(self.bytes_received) as f64;

            self.current_rate_bps = RateInfo {
                outgoing_bps: (sent_diff * 8.0) / elapsed,
                incoming_bps: (recv_diff * 8.0) / elapsed,
                last_calculation: now,
            };

            // Update backward compatibility fields
            self.current_incoming_rate_bps = self.current_rate_bps.incoming_bps;
            self.current_outgoing_rate_bps = self.current_rate_bps.outgoing_bps;
        }
    }

    /// Check if this connection might be QUIC based on port and protocol
    pub fn is_potential_quic(&self) -> bool {
        self.protocol == Protocol::UDP
            && (self.local_addr.port() == 443 || self.remote_addr.port() == 443)
    }

    /// Generate a QUIC-aware connection key that can handle Connection ID changes
    pub fn quic_key(&self, connection_id_hex: Option<&String>) -> String {
        if self.protocol == Protocol::UDP {
            if let Some(conn_id) = connection_id_hex {
                return format!(
                    "QUIC:{}:{}-{}",
                    conn_id, self.local_addr, self.remote_addr
                );
            }
        }
        
        // Fallback to regular key
        self.key()
    }

    /// Get a display string for the application protocol
    pub fn application_display(&self) -> String {
        if let Some(dpi) = &self.dpi_info {
            dpi.application.to_string()
        } else if self.is_potential_quic() {
            "QUIC?".to_string()
        } else {
            match self.service_name.as_deref() {
                Some(name) => name.to_uppercase(),
                None => "Unknown".to_string(),
            }
        }
    }
}
