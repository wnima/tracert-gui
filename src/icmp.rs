use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

const ICMP_ECHO_REPLY: u8 = 0;
const ICMP_TIME_EXCEEDED: u8 = 11;
const ICMP_ECHO_REQUEST: u8 = 8;
const ICMPV6_TIME_EXCEEDED: u8 = 3;
const ICMPV6_ECHO_REQUEST: u8 = 128;
const ICMPV6_ECHO_REPLY: u8 = 129;

fn calculate_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0;

    while i < data.len() {
        let word = if i + 1 < data.len() {
            (data[i] as u32) << 8 | (data[i + 1] as u32)
        } else {
            (data[i] as u32) << 8
        };
        sum += word;
        i += 2;
    }

    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !sum as u16
}

pub struct Icmp;

impl Icmp {
    /// 发送ICMP探测包并等待响应
    pub fn send_icmp_probe_with_ttl(
        &self,
        target_host: &str,
        ttl: u32,
        timeout: Duration,
    ) -> io::Result<(Option<Vec<u8>>, Option<SocketAddr>, Duration)> {
        let target_ip: IpAddr = target_host
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid IP address"))?;

        match target_ip {
            IpAddr::V4(addr) => self.send_icmpv4_probe(addr, ttl, timeout),
            IpAddr::V6(addr) => self.send_icmpv6_probe(addr, ttl, timeout),
        }
    }

    fn send_icmpv4_probe(
        &self,
        target_addr: Ipv4Addr,
        ttl: u32,
        timeout: Duration,
    ) -> io::Result<(Option<Vec<u8>>, Option<SocketAddr>, Duration)> {
        let identifier = (std::process::id() % 65535) as u16;
        let sequence_number = (std::time::Instant::now().elapsed().as_millis() % 65535) as u16;
        let data = b"Hello ICMP v4!";

        let socket = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?;
        self.configure_socket_v4(&socket, ttl)?;

        let packet = self.create_icmpv4_packet(ICMP_ECHO_REQUEST, 0, identifier, sequence_number, data);

        self.send_and_receive(
            socket,
            &packet,
            SocketAddr::new(IpAddr::V4(target_addr), 0),
            timeout,
            identifier,
            sequence_number,
        )
    }

    fn send_icmpv6_probe(
        &self,
        target_addr: Ipv6Addr,
        hop_limit: u32,
        timeout: Duration,
    ) -> io::Result<(Option<Vec<u8>>, Option<SocketAddr>, Duration)> {
        let identifier = (std::process::id() % 65535) as u16;
        let sequence_number = (std::time::Instant::now().elapsed().as_millis() % 65535) as u16;
        let data = b"Hello ICMP v6!";

        let socket = Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6))?;
        self.configure_socket_v6(&socket, hop_limit)?;

        let packet = self.create_icmpv6_packet(ICMPV6_ECHO_REQUEST, 0, identifier, sequence_number, data);

        self.send_and_receive(
            socket,
            &packet,
            SocketAddr::new(IpAddr::V6(target_addr), 0),
            timeout,
            identifier,
            sequence_number,
        )
    }

    fn configure_socket_v4(&self, socket: &Socket, ttl: u32) -> io::Result<()> {
        socket.set_reuse_address(true)?;
        let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        socket.bind(&SockAddr::from(local_addr))?;
        socket.set_ttl_v4(ttl)?;
        socket.set_read_timeout(Some(Duration::from_secs(5)))?;
        Ok(())
    }

    fn configure_socket_v6(&self, socket: &Socket, hop_limit: u32) -> io::Result<()> {
        socket.set_reuse_address(true)?;
        socket.set_only_v6(true)?;
        let local_addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0);
        socket.bind(&SockAddr::from(local_addr))?;
        socket.set_unicast_hops_v6(hop_limit)?;
        socket.set_read_timeout(Some(Duration::from_secs(5)))?;
        Ok(())
    }

    fn send_and_receive(
        &self,
        socket: Socket,
        packet: &[u8],
        destination: SocketAddr,
        timeout: Duration,
        identifier: u16,
        sequence_number: u16,
    ) -> io::Result<(Option<Vec<u8>>, Option<SocketAddr>, Duration)> {
        let start_time = Instant::now();
        socket.send_to(packet, &SockAddr::from(destination))?;

        let mut buffer = [std::mem::MaybeUninit::uninit(); 1024];
        let (bytes_read, src_addr) = match socket.recv_from(&mut buffer) {
            Ok((n, addr)) => (n, addr),
            Err(e) => {
                return if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut {
                    Ok((None, None, timeout))
                } else {
                    Err(e)
                };
            }
        };

        let elapsed = start_time.elapsed();
        let source_addr = match src_addr.as_socket() {
            Some(addr) => addr,
            None => return Ok((None, None, elapsed)),
        };

        let data = unsafe {
            std::slice::from_raw_parts(buffer.as_ptr() as *const u8, bytes_read)
        };

        let is_ipv6 = source_addr.ip().is_ipv6();
        let result = if is_ipv6 {
            self.parse_icmpv6_response(data, bytes_read, source_addr, elapsed, identifier, sequence_number)
        } else {
            self.parse_icmpv4_response(data, bytes_read, source_addr, elapsed, identifier, sequence_number)
        };

        Ok(result)
    }

    fn parse_icmpv4_response(
        &self,
        data: &[u8],
        bytes_read: usize,
        source_addr: SocketAddr,
        elapsed: Duration,
        identifier: u16,
        sequence_number: u16,
    ) -> (Option<Vec<u8>>, Option<SocketAddr>, Duration) {
        if bytes_read < 28 {
            return (None, Some(source_addr), elapsed);
        }

        let icmp_start = 20;
        let icmp_packet = &data[icmp_start..bytes_read];

        if icmp_packet.len() < 8 {
            return (None, Some(source_addr), elapsed);
        }

        let icmp_type = icmp_packet[0];
        let icmp_code = icmp_packet[1];

        match icmp_type {
            ICMP_TIME_EXCEEDED if icmp_code == 0 => {
                // Time Exceeded: 提取原始ICMP包的标识符和序列号进行验证
                let original_icmp_start = 48; // IP头(20) + ICMP头(8) + 原始IP头(20)
                if bytes_read > original_icmp_start + 7 {
                    let orig_id = u16::from_be_bytes([data[original_icmp_start + 4], data[original_icmp_start + 5]]);
                    let orig_seq = u16::from_be_bytes([data[original_icmp_start + 6], data[original_icmp_start + 7]]);
                    let orig_type = data[original_icmp_start];

                    if orig_id == identifier && orig_seq == sequence_number && orig_type == ICMP_ECHO_REQUEST {
                        return (Some(data[..bytes_read].to_vec()), Some(source_addr), elapsed);
                    }
                }
                (None, Some(source_addr), elapsed)
            }
            3 => {
                // Destination Unreachable
                (None, Some(source_addr), elapsed)
            }
            ICMP_ECHO_REPLY => {
                let resp_id = u16::from_be_bytes([icmp_packet[4], icmp_packet[5]]);
                let resp_seq = u16::from_be_bytes([icmp_packet[6], icmp_packet[7]]);

                if resp_id == identifier && resp_seq == sequence_number {
                    (Some(data[..bytes_read].to_vec()), Some(source_addr), elapsed)
                } else {
                    (None, Some(source_addr), elapsed)
                }
            }
            _ => (None, Some(source_addr), elapsed),
        }
    }

    fn parse_icmpv6_response(
        &self,
        data: &[u8],
        bytes_read: usize,
        source_addr: SocketAddr,
        elapsed: Duration,
        identifier: u16,
        sequence_number: u16,
    ) -> (Option<Vec<u8>>, Option<SocketAddr>, Duration) {
        if bytes_read < 48 {
            return (None, None, elapsed);
        }

        let icmpv6_start = 40;
        let icmpv6_packet = &data[icmpv6_start..bytes_read];

        if icmpv6_packet.len() < 8 {
            return (None, None, elapsed);
        }

        let icmpv6_type = icmpv6_packet[0];

        match icmpv6_type {
            t if t == ICMPV6_TIME_EXCEEDED => {
                let original_icmp_start = 88; // IPv6头(40) + ICMPv6头(8) + 原始IPv6头(40)
                if bytes_read > original_icmp_start + 7 {
                    let orig_id = u16::from_be_bytes([data[original_icmp_start + 4], data[original_icmp_start + 5]]);
                    let orig_seq = u16::from_be_bytes([data[original_icmp_start + 6], data[original_icmp_start + 7]]);

                    if orig_id == identifier && orig_seq == sequence_number {
                        return (Some(data[..bytes_read].to_vec()), Some(source_addr), elapsed);
                    }
                }
                (None, Some(source_addr), elapsed)
            }
            t if t == ICMPV6_ECHO_REPLY => {
                let resp_id = u16::from_be_bytes([icmpv6_packet[4], icmpv6_packet[5]]);
                let resp_seq = u16::from_be_bytes([icmpv6_packet[6], icmpv6_packet[7]]);

                if resp_id == identifier && resp_seq == sequence_number {
                    (Some(data[..bytes_read].to_vec()), Some(source_addr), elapsed)
                } else {
                    (None, Some(source_addr), elapsed)
                }
            }
            _ => (None, Some(source_addr), elapsed),
        }
    }

    fn create_icmpv4_packet(
        &self,
        icmp_type: u8,
        code: u8,
        identifier: u16,
        sequence: u16,
        data: &[u8],
    ) -> Vec<u8> {
        let mut packet = Vec::with_capacity(8 + data.len());
        packet.push(icmp_type);
        packet.push(code);
        packet.extend_from_slice(&[0u8, 0u8]); // 校验和占位
        packet.extend_from_slice(&identifier.to_be_bytes());
        packet.extend_from_slice(&sequence.to_be_bytes());
        packet.extend_from_slice(data);

        let checksum = calculate_checksum(&packet);
        packet[2] = (checksum >> 8) as u8;
        packet[3] = checksum as u8;

        packet
    }

    fn create_icmpv6_packet(
        &self,
        icmp_type: u8,
        code: u8,
        identifier: u16,
        sequence: u16,
        data: &[u8],
    ) -> Vec<u8> {
        let mut packet = Vec::with_capacity(8 + data.len());
        packet.push(icmp_type);
        packet.push(code);
        packet.extend_from_slice(&[0u8, 0u8]); // 校验和占位
        packet.extend_from_slice(&identifier.to_be_bytes());
        packet.extend_from_slice(&sequence.to_be_bytes());
        packet.extend_from_slice(data);

        let checksum = calculate_checksum(&packet);
        packet[2] = (checksum >> 8) as u8;
        packet[3] = checksum as u8;

        packet
    }
}
