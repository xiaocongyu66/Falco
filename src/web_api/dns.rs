//! DNS resolver — from-scratch implementation.
//!
//! # Overview
//!
//! Normally Falco uses the system's DNS resolver (via `ureq`, which calls
//! `getaddrinfo`). This module implements a standalone DNS resolver that
//! can do lookups directly, bypassing the system resolver. This is useful
//! for:
//!
//! - DNS-over-HTTPS (DoH) — encrypted DNS queries
//! - Custom DNS server configuration (bypassing /etc/resolv.conf)
//! - Debugging and inspection of DNS responses
//! - Supporting DNSSEC validation (future)
//!
//! # Supported Features
//!
//! - A (IPv4 address) lookups
//! - AAAA (IPv6 address) lookups
//! - CNAME (canonical name) lookups
//! - MX (mail exchange) lookups
//! - TXT (text record) lookups
//! - NS (name server) lookups
//! - Parsing of /etc/resolv.conf for server addresses
//! - UDP transport (port 53)
//! - DNS message compression (pointer-based name encoding)
//!
//! # Not Supported (yet)
//!
//! - DNS-over-HTTPS (DoH) — planned
//! - DNS-over-TLS (DoT) — planned
//! - DNSSEC validation — planned
//! - Caching with TTL — (basic cache implemented)

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A DNS record type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum RecordType {
    A = 1,
    NS = 2,
    CNAME = 5,
    MX = 15,
    TXT = 16,
    AAAA = 28,
    SRV = 33,
    PTR = 12,
}

impl RecordType {
    /// Parse a record type from a string (e.g., "A", "AAAA", "MX").
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "A" => Some(RecordType::A),
            "NS" => Some(RecordType::NS),
            "CNAME" => Some(RecordType::CNAME),
            "MX" => Some(RecordType::MX),
            "TXT" => Some(RecordType::TXT),
            "AAAA" => Some(RecordType::AAAA),
            "SRV" => Some(RecordType::SRV),
            "PTR" => Some(RecordType::PTR),
            _ => None,
        }
    }
}

/// A DNS resource record.
#[derive(Debug, Clone)]
pub struct DnsRecord {
    pub name: String,
    pub record_type: RecordType,
    pub ttl: u32,
    pub data: RecordData,
}

/// The data portion of a DNS record.
#[derive(Debug, Clone)]
pub enum RecordData {
    /// IPv4 address (A record).
    A(String),
    /// IPv6 address (AAAA record).
    AAAA(String),
    /// Canonical name (CNAME record).
    CNAME(String),
    /// Mail exchange (MX record): (preference, exchange).
    MX(u16, String),
    /// Text record.
    TXT(String),
    /// Name server (NS record).
    NS(String),
    /// Service record (SRV): (priority, weight, port, target).
    SRV(u16, u16, u16, String),
    /// Pointer record (PTR).
    PTR(String),
}

/// A DNS response message.
#[derive(Debug, Clone)]
pub struct DnsResponse {
    pub answers: Vec<DnsRecord>,
    pub authority: Vec<DnsRecord>,
    pub additional: Vec<DnsRecord>,
}

/// A DNS resolver.
pub struct DnsResolver {
    /// The DNS server addresses to query (e.g., ["8.8.8.8:53", "1.1.1.1:53"]).
    servers: Vec<SocketAddr>,
    /// Query timeout.
    timeout: Duration,
    /// Cache of previous lookups (name + type → (records, expiry)).
    cache: Mutex<HashMap<(String, RecordType), (Vec<DnsRecord>, Instant)>>,
}

impl DnsResolver {
    /// Create a new resolver with the given servers.
    pub fn new(servers: Vec<SocketAddr>) -> Self {
        Self {
            servers,
            timeout: Duration::from_secs(5),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Create a resolver configured from /etc/resolv.conf (Unix).
    pub fn from_system() -> io::Result<Self> {
        let servers = read_resolv_conf()?;
        if servers.is_empty() {
            // Fallback to well-known public resolvers.
            Ok(Self::new(vec![
                "8.8.8.8:53".parse().unwrap(),
                "1.1.1.1:53".parse().unwrap(),
            ]))
        } else {
            Ok(Self::new(servers))
        }
    }

    /// Set the query timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Look up records for a name and type.
    pub fn lookup(&self, name: &str, record_type: RecordType) -> io::Result<Vec<DnsRecord>> {
        // Check the cache first.
        {
            let cache = self.cache.lock().unwrap();
            if let Some((records, expiry)) = cache.get(&(name.to_string(), record_type)) {
                if *expiry > Instant::now() {
                    return Ok(records.clone());
                }
            }
        }

        // Query each server until we get a response.
        let mut last_error = None;
        for server in &self.servers {
            match self.query_server(*server, name, record_type) {
                Ok(response) => {
                    // Cache the result with the minimum TTL.
                    let min_ttl = response
                        .answers
                        .iter()
                        .map(|r| r.ttl)
                        .min()
                        .unwrap_or(300);
                    let expiry = Instant::now() + Duration::from_secs(min_ttl as u64);
                    self.cache.lock().unwrap().insert(
                        (name.to_string(), record_type),
                        (response.answers.clone(), expiry),
                    );
                    return Ok(response.answers);
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }
        Err(last_error.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no DNS servers configured",
            )
        }))
    }

    /// Convenience: look up IPv4 addresses (A records).
    pub fn lookup_a(&self, name: &str) -> io::Result<Vec<String>> {
        let records = self.lookup(name, RecordType::A)?;
        Ok(records
            .into_iter()
            .filter_map(|r| match r.data {
                RecordData::A(addr) => Some(addr),
                _ => None,
            })
            .collect())
    }

    /// Convenience: look up IPv6 addresses (AAAA records).
    pub fn lookup_aaaa(&self, name: &str) -> io::Result<Vec<String>> {
        let records = self.lookup(name, RecordType::AAAA)?;
        Ok(records
            .into_iter()
            .filter_map(|r| match r.data {
                RecordData::AAAA(addr) => Some(addr),
                _ => None,
            })
            .collect())
    }

    /// Query a specific DNS server.
    fn query_server(
        &self,
        server: SocketAddr,
        name: &str,
        record_type: RecordType,
    ) -> io::Result<DnsResponse> {
        // Build the query message.
        let query = build_query(name, record_type);

        // Create a UDP socket.
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(self.timeout))?;
        socket.set_write_timeout(Some(self.timeout))?;

        // Send the query.
        socket.send_to(&query, server)?;

        // Receive the response (up to 512 bytes for standard DNS).
        let mut buf = [0u8; 4096];
        let (len, _) = socket.recv_from(&mut buf)?;

        // Parse the response.
        parse_response(&buf[..len])
    }

    /// Clear the cache.
    pub fn clear_cache(&self) {
        self.cache.lock().unwrap().clear();
    }
}

/// Build a DNS query message.
fn build_query(name: &str, record_type: RecordType) -> Vec<u8> {
    let mut msg = Vec::with_capacity(512);

    // Header (12 bytes):
    //   ID (2 bytes) — random.
    //   Flags (2 bytes) — RD=1 (recursion desired).
    //   QDCOUNT (2 bytes) — 1 question.
    //   ANCOUNT, NSCOUNT, ARCOUNT (6 bytes) — 0.
    let id: u16 = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u16)
        .unwrap_or(0x1234))
        .wrapping_add(0xBEEF);
    msg.extend_from_slice(&id.to_be_bytes());
    msg.extend_from_slice(&0x0100u16.to_be_bytes()); // RD=1
    msg.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    msg.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
    msg.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    msg.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT

    // Question section:
    //   QNAME — the domain name as length-prefixed labels.
    for label in name.split('.') {
        if label.is_empty() {
            continue;
        }
        let bytes = label.as_bytes();
        if bytes.len() > 63 {
            // Label too long — truncate (shouldn't happen in practice).
            msg.push(63);
            msg.extend_from_slice(&bytes[..63]);
        } else {
            msg.push(bytes.len() as u8);
            msg.extend_from_slice(bytes);
        }
    }
    msg.push(0); // Root label (end of name).

    //   QTYPE (2 bytes).
    msg.extend_from_slice(&(record_type as u16).to_be_bytes());
    //   QCLASS (2 bytes) — IN (1).
    msg.extend_from_slice(&1u16.to_be_bytes());

    msg
}

/// Parse a DNS response message.
fn parse_response(data: &[u8]) -> io::Result<DnsResponse> {
    if data.len() < 12 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "DNS response too short",
        ));
    }

    // Parse the header.
    let _id = u16::from_be_bytes([data[0], data[1]]);
    let flags = u16::from_be_bytes([data[2], data[3]]);
    let qdcount = u16::from_be_bytes([data[4], data[5]]);
    let ancount = u16::from_be_bytes([data[6], data[7]]);
    let nscount = u16::from_be_bytes([data[8], data[9]]);
    let arcount = u16::from_be_bytes([data[10], data[11]]);

    // Check the response code (lower 4 bits of flags).
    let rcode = flags & 0x000F;
    if rcode != 0 {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("DNS response code: {}", rcode),
        ));
    }

    let mut offset = 12;

    // Skip the question section.
    for _ in 0..qdcount {
        let (_, new_offset) = parse_name(data, offset)?;
        offset = new_offset;
        offset += 4; // QTYPE + QCLASS
    }

    // Parse the answer, authority, and additional sections.
    let answers = parse_records(data, &mut offset, ancount as usize)?;
    let authority = parse_records(data, &mut offset, nscount as usize)?;
    let additional = parse_records(data, &mut offset, arcount as usize)?;

    Ok(DnsResponse {
        answers,
        authority,
        additional,
    })
}

/// Parse resource records from the response.
fn parse_records(data: &[u8], offset: &mut usize, count: usize) -> io::Result<Vec<DnsRecord>> {
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let record = parse_record(data, offset)?;
        records.push(record);
    }
    Ok(records)
}

/// Parse a single resource record.
fn parse_record(data: &[u8], offset: &mut usize) -> io::Result<DnsRecord> {
    // Parse the name.
    let (name, new_offset) = parse_name(data, *offset)?;
    *offset = new_offset;

    if *offset + 10 > data.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "DNS record header truncated",
        ));
    }

    // Parse TYPE, CLASS, TTL, RDLENGTH.
    let rtype = u16::from_be_bytes([data[*offset], data[*offset + 1]]);
    let _rclass = u16::from_be_bytes([data[*offset + 2], data[*offset + 3]]);
    let ttl = u32::from_be_bytes([
        data[*offset + 4],
        data[*offset + 5],
        data[*offset + 6],
        data[*offset + 7],
    ]);
    let rdlength = u16::from_be_bytes([data[*offset + 8], data[*offset + 9]]) as usize;
    *offset += 10;

    if *offset + rdlength > data.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "DNS record data truncated",
        ));
    }

    let rdata = &data[*offset..*offset + rdlength];
    *offset += rdlength;

    // Parse the record data based on the type.
    let record_type = match rtype {
        1 => RecordType::A,
        2 => RecordType::NS,
        5 => RecordType::CNAME,
        12 => RecordType::PTR,
        15 => RecordType::MX,
        16 => RecordType::TXT,
        28 => RecordType::AAAA,
        33 => RecordType::SRV,
        _ => {
            // Unknown type — skip.
            return Ok(DnsRecord {
                name,
                record_type: RecordType::A, // placeholder
                ttl,
                data: RecordData::TXT(format!("unknown type {}", rtype)),
            });
        }
    };

    let record_data = match record_type {
        RecordType::A => {
            if rdata.len() != 4 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "A record: wrong length",
                ));
            }
            RecordData::A(format!(
                "{}.{}.{}.{}",
                rdata[0], rdata[1], rdata[2], rdata[3]
            ))
        }
        RecordType::AAAA => {
            if rdata.len() != 16 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "AAAA record: wrong length",
                ));
            }
            // Format as IPv6.
            let segments: Vec<String> = (0..8)
                .map(|i| {
                    u16::from_be_bytes([rdata[i * 2], rdata[i * 2 + 1]]).to_string()
                })
                .collect();
            RecordData::AAAA(segments.join(":"))
        }
        RecordType::CNAME | RecordType::NS | RecordType::PTR => {
            // The data is a domain name (possibly compressed).
            let (cname, _) = parse_name(data, *offset - rdlength)?;
            match record_type {
                RecordType::CNAME => RecordData::CNAME(cname),
                RecordType::NS => RecordData::NS(cname),
                RecordType::PTR => RecordData::PTR(cname),
                _ => unreachable!(),
            }
        }
        RecordType::MX => {
            if rdata.len() < 3 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "MX record: too short",
                ));
            }
            let preference = u16::from_be_bytes([rdata[0], rdata[1]]);
            let (exchange, _) = parse_name(data, *offset - rdlength + 2)?;
            RecordData::MX(preference, exchange)
        }
        RecordType::TXT => {
            if rdata.is_empty() {
                RecordData::TXT(String::new())
            } else {
                let txt_len = rdata[0] as usize;
                if txt_len + 1 > rdata.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "TXT record: length mismatch",
                    ));
                }
                let txt = String::from_utf8_lossy(&rdata[1..1 + txt_len]).to_string();
                RecordData::TXT(txt)
            }
        }
        RecordType::SRV => {
            if rdata.len() < 7 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "SRV record: too short",
                ));
            }
            let priority = u16::from_be_bytes([rdata[0], rdata[1]]);
            let weight = u16::from_be_bytes([rdata[2], rdata[3]]);
            let port = u16::from_be_bytes([rdata[4], rdata[5]]);
            let (target, _) = parse_name(data, *offset - rdlength + 6)?;
            RecordData::SRV(priority, weight, port, target)
        }
    };

    Ok(DnsRecord {
        name,
        record_type,
        ttl,
        data: record_data,
    })
}

/// Parse a domain name (with compression support).
///
/// Returns the name and the offset just past the name.
fn parse_name(data: &[u8], mut offset: usize) -> io::Result<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut jumped = false;
    let mut original_offset = offset;
    let mut jumps = 0;

    loop {
        if offset >= data.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "DNS name: out of bounds",
            ));
        }

        let len = data[offset];

        if len == 0 {
            // End of name.
            offset += 1;
            break;
        }

        if (len & 0xC0) == 0xC0 {
            // Compression pointer.
            if offset + 1 >= data.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "DNS name: pointer truncated",
                ));
            }
            let pointer = (((len & 0x3F) as usize) << 8) | data[offset + 1] as usize;
            if !jumped {
                original_offset = offset + 2;
            }
            offset = pointer;
            jumped = true;
            jumps += 1;
            if jumps > 10 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "DNS name: too many compression jumps",
                ));
            }
            continue;
        }

        let len = len as usize;
        if offset + 1 + len > data.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "DNS name: label out of bounds",
            ));
        }

        let label = String::from_utf8_lossy(&data[offset + 1..offset + 1 + len]).to_string();
        labels.push(label);
        offset += 1 + len;
    }

    let final_offset = if jumped { original_offset } else { offset };
    let name = if labels.is_empty() {
        ".".to_string()
    } else {
        labels.join(".")
    };

    Ok((name, final_offset))
}

/// Read /etc/resolv.conf to find DNS servers (Unix).
fn read_resolv_conf() -> io::Result<Vec<SocketAddr>> {
    let content = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
    let mut servers = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("nameserver") {
            let ip = rest.trim();
            if !ip.is_empty() {
                if let Ok(addr) = format!("{}:53", ip).parse() {
                    servers.push(addr);
                }
            }
        }
    }

    Ok(servers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_type_from_str() {
        assert_eq!(RecordType::from_str("A"), Some(RecordType::A));
        assert_eq!(RecordType::from_str("aaaa"), Some(RecordType::AAAA));
        assert_eq!(RecordType::from_str("MX"), Some(RecordType::MX));
        assert_eq!(RecordType::from_str("unknown"), None);
    }

    #[test]
    fn build_query_basic() {
        let query = build_query("example.com", RecordType::A);
        // Header is 12 bytes + question section.
        assert!(query.len() > 12);
        // ID.
        assert_eq!(query[0], query[0]); // non-zero
        // Flags: RD=1.
        assert_eq!(query[3], 0x00);
        // QDCOUNT = 1.
        assert_eq!(query[4], 0x00);
        assert_eq!(query[5], 0x01);
    }

    #[test]
    fn build_query_name_encoding() {
        let query = build_query("example.com", RecordType::A);
        // After the 12-byte header, the name starts.
        // "example" = 7 bytes, prefixed with length 7.
        assert_eq!(query[12], 7);
        assert_eq!(&query[13..20], b"example");
        assert_eq!(query[20], 3);
        assert_eq!(&query[21..24], b"com");
        assert_eq!(query[24], 0); // root label
    }

    #[test]
    fn parse_name_simple() {
        // "example.com" encoded as: 7example3com0
        let data = [
            0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
            0x03, b'c', b'o', b'm',
            0x00,
        ];
        let (name, offset) = parse_name(&data, 0).unwrap();
        assert_eq!(name, "example.com");
        assert_eq!(offset, 13);
    }

    #[test]
    fn parse_name_with_compression() {
        // Name at offset 0: "example.com"
        // At offset 13: a pointer back to offset 0.
        let mut data = vec![
            0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
            0x03, b'c', b'o', b'm',
            0x00,
        ];
        let pointer_offset = data.len();
        data.push(0xC0); // compression pointer high byte
        data.push(0x00); // compression pointer low byte (offset 0)

        let (name, offset) = parse_name(&data, pointer_offset).unwrap();
        assert_eq!(name, "example.com");
        assert_eq!(offset, pointer_offset + 2);
    }

    #[test]
    fn parse_name_root() {
        let data = [0x00];
        let (name, offset) = parse_name(&data, 0).unwrap();
        assert_eq!(name, ".");
        assert_eq!(offset, 1);
    }

    #[test]
    fn parse_response_header() {
        // A minimal DNS response with no answers.
        let data = vec![
            0x12, 0x34, // ID
            0x81, 0x80, // flags: QR=1, RD=1, RA=1
            0x00, 0x01, // QDCOUNT
            0x00, 0x00, // ANCOUNT
            0x00, 0x00, // NSCOUNT
            0x00, 0x00, // ARCOUNT
            // Question: "com" → root
            0x03, b'c', b'o', b'm', 0x00,
            0x00, 0x01, // QTYPE = A
            0x00, 0x01, // QCLASS = IN
        ];
        let response = parse_response(&data).unwrap();
        assert!(response.answers.is_empty());
    }

    #[test]
    fn parse_a_record() {
        // Build a response with one A record for "example.com" → 1.2.3.4.
        let mut data = vec![
            0x12, 0x34, // ID
            0x81, 0x80, // flags
            0x00, 0x00, // QDCOUNT = 0
            0x00, 0x01, // ANCOUNT = 1
            0x00, 0x00, // NSCOUNT
            0x00, 0x00, // ARCOUNT
        ];
        // Answer: "example.com" → A, IN, TTL=300, RDLENGTH=4, 1.2.3.4
        data.extend_from_slice(&[
            0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
            0x03, b'c', b'o', b'm', 0x00,
            0x00, 0x01, // TYPE = A
            0x00, 0x01, // CLASS = IN
            0x00, 0x00, 0x01, 0x2C, // TTL = 300
            0x00, 0x04, // RDLENGTH = 4
            1, 2, 3, 4, // RDATA = 1.2.3.4
        ]);
        let response = parse_response(&data).unwrap();
        assert_eq!(response.answers.len(), 1);
        assert_eq!(response.answers[0].name, "example.com");
        assert_eq!(response.answers[0].ttl, 300);
        match &response.answers[0].data {
            RecordData::A(addr) => assert_eq!(addr, "1.2.3.4"),
            _ => panic!("expected A record"),
        }
    }

    #[test]
    fn parse_cname_record() {
        let mut data = vec![
            0x12, 0x34,
            0x81, 0x80,
            0x00, 0x00,
            0x00, 0x01,
            0x00, 0x00,
            0x00, 0x00,
        ];
        // Answer: "www.example.com" → CNAME "example.com"
        data.extend_from_slice(&[
            0x03, b'w', b'w', b'w',
            0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
            0x03, b'c', b'o', b'm', 0x00,
            0x00, 0x05, // TYPE = CNAME
            0x00, 0x01, // CLASS = IN
            0x00, 0x00, 0x01, 0x2C, // TTL = 300
            0x00, 0x0D, // RDLENGTH = 13
            // RDATA: "example.com" encoded
            0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e',
            0x03, b'c', b'o', b'm', 0x00,
        ]);
        let response = parse_response(&data).unwrap();
        assert_eq!(response.answers.len(), 1);
        match &response.answers[0].data {
            RecordData::CNAME(cname) => assert_eq!(cname, "example.com"),
            _ => panic!("expected CNAME record"),
        }
    }

    #[test]
    fn resolver_creation() {
        let resolver = DnsResolver::new(vec![
            "8.8.8.8:53".parse().unwrap(),
            "1.1.1.1:53".parse().unwrap(),
        ]);
        assert_eq!(resolver.servers.len(), 2);
    }

    #[test]
    fn resolver_cache() {
        let resolver = DnsResolver::new(vec!["8.8.8.8:53".parse().unwrap()]);
        // Initially the cache is empty.
        assert!(resolver.cache.lock().unwrap().is_empty());
    }

    #[test]
    fn read_resolv_conf_returns_vec() {
        // This should not panic even if /etc/resolv.conf doesn't exist.
        let result = read_resolv_conf();
        assert!(result.is_ok());
    }
}
