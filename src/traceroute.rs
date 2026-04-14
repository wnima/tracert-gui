use anyhow::{Result, anyhow};
use log::info;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::net::lookup_host;

use crate::icmp::Icmp;

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum IpVersion {
    IPv4,
    IPv6,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HopInfo {
    pub hop: u32,
    pub ip: String,
    pub rtt1: Option<f64>,
    pub rtt2: Option<f64>,
    pub rtt3: Option<f64>,
    pub avg_rtt: Option<f64>,
    pub hostnames: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracerouteResult {
    pub destination: String,
    pub hops: Vec<HopInfo>,
    pub total_time: f64,
}

pub struct ProductionTraceroute {
    max_hops: u32,
    timeout: Duration,
    ip_version: IpVersion,
}

impl ProductionTraceroute {
    pub fn with_config(max_hops: u32, timeout_ms: u64, ip_version: IpVersion) -> Self {
        Self {
            max_hops,
            timeout: Duration::from_millis(timeout_ms),
            ip_version,
        }
    }

    pub async fn trace_with_callback<F>(
        &mut self,
        target: &str,
        callback: F,
        cancel_flag: Arc<AtomicBool>,
    ) -> Result<TracerouteResult>
    where
        F: Fn(HopInfo) + Send + 'static,
    {
        let start_time = Instant::now();
        info!("开始追踪: {}", target);

        if cancel_flag.load(Ordering::Relaxed) {
            return Err(anyhow!("追踪已取消"));
        }

        // 解析目标地址
        let ip_addr = if let Ok(parsed) = target.parse::<IpAddr>() {
            parsed
        } else {
            self.resolve_hostname(target).await?
        };

        if cancel_flag.load(Ordering::Relaxed) {
            return Err(anyhow!("追踪已取消"));
        }

        let hops = self.run_icmp_traceroute(&ip_addr.to_string(), callback, &cancel_flag)?;
        let total_time = start_time.elapsed().as_secs_f64();

        Ok(TracerouteResult {
            destination: ip_addr.to_string(),
            hops,
            total_time,
        })
    }

    async fn resolve_hostname(&self, hostname: &str) -> Result<IpAddr> {
        // 尝试直接解析
        if let Ok(addr) = self.try_resolve(hostname, None).await {
            return Ok(addr);
        }

        // 尝试添加端口后解析
        let with_port = format!("{}:80", hostname);
        if let Ok(addr) = self.try_resolve(&with_port, None).await {
            return Ok(addr);
        }

        // 尝试作为IP地址解析
        if let Ok(ip_addr) = hostname.parse::<IpAddr>() {
            return Ok(ip_addr);
        }

        Err(anyhow!("无法解析主机名: {}", hostname))
    }

    async fn try_resolve(&self, addr_str: &str, _version: Option<IpVersion>) -> Result<IpAddr> {
        match lookup_host(addr_str).await {
            Ok(mut addrs) => {
                let target_version = _version.unwrap_or(self.ip_version);
                
                // 优先匹配指定IP版本
                for addr in &mut addrs {
                    match (target_version, addr.ip()) {
                        (IpVersion::IPv4, IpAddr::V4(_)) | (IpVersion::IPv6, IpAddr::V6(_)) => {
                            return Ok(addr.ip());
                        }
                        _ => continue,
                    }
                }

                // 使用第一个可用地址
                let mut addrs = lookup_host(addr_str).await?;
                if let Some(addr) = addrs.next() {
                    return Ok(addr.ip());
                }
            }
            Err(e) => return Err(anyhow!(e)),
        }

        Err(anyhow!("未找到匹配的地址"))
    }

    fn run_icmp_traceroute<F>(
        &self,
        target: &str,
        callback: F,
        cancel_flag: &Arc<AtomicBool>,
    ) -> Result<Vec<HopInfo>>
    where
        F: Fn(HopInfo) + Send + 'static,
    {
        let target_addr = target.parse::<IpAddr>()
            .map_err(|_| anyhow!("无效的目标地址"))?;
        
        let mut hops = Vec::new();

        for ttl in 1..=self.max_hops {
            if cancel_flag.load(Ordering::Relaxed) {
                info!("在跳数 {} 处取消追踪", ttl);
                break;
            }

            info!("探测 TTL: {}", ttl);

            let mut rtts = [None; 3];
            let mut valid_ips = Vec::new();

            for probe_idx in 0..3 {
                if cancel_flag.load(Ordering::Relaxed) {
                    break;
                }

                match self.send_probe(&target_addr, ttl, probe_idx) {
                    Ok((ip, rtt)) => {
                        rtts[probe_idx] = Some(rtt);
                        valid_ips.push(ip.clone());
                        info!("  探测 {}: IP={}, RTT={:.2}ms", probe_idx + 1, ip, rtt);
                    }
                    Err(e) => {
                        rtts[probe_idx] = None;
                        valid_ips.push(e.to_string());
                        info!("  探测 {}: 失败 - {}", probe_idx + 1, e);
                    }
                }

                std::thread::sleep(Duration::from_millis(50));
            }

            if cancel_flag.load(Ordering::Relaxed) {
                break;
            }

            // 确定显示的IP地址
            let display_ip = if !valid_ips.is_empty() {
                let valid_addrs: Vec<_> = valid_ips.iter()
                    .filter(|ip| ip.parse::<IpAddr>().is_ok())
                    .collect();

                if !valid_addrs.is_empty() {
                    valid_addrs[0].clone()
                } else {
                    valid_ips[0].clone()
                }
            } else {
                "*".to_string()
            };

            let hop_info = HopInfo {
                hop: ttl,
                ip: display_ip,
                rtt1: rtts[0],
                rtt2: rtts[1],
                rtt3: rtts[2],
                avg_rtt: self.calculate_avg_rtt(&rtts),
                hostnames: vec![],
            };

            callback(hop_info.clone());
            hops.push(hop_info);

            // 如果到达目标，停止探测
            if let Some(last_hop) = hops.last() {
                if last_hop.ip == target {
                    break;
                }
            }
        }

        Ok(hops)
    }

    fn send_probe(
        &self,
        target_addr: &IpAddr,
        ttl: u32,
        _probe_idx: usize,
    ) -> Result<(String, f64)> {
        let icmp = Icmp {};

        // 根据TTL动态调整超时时间
        let dynamic_timeout = if ttl <= 5 {
            self.timeout
        } else if ttl <= 10 {
            Duration::from_millis(self.timeout.as_millis() as u64 * 2)
        } else {
            Duration::from_millis(self.timeout.as_millis() as u64 * 3)
        };

        let result = icmp.send_icmp_probe_with_ttl(&target_addr.to_string(), ttl, dynamic_timeout);

        match result {
            Ok((_, Some(addr), duration)) => {
                let rtt_ms = duration.as_secs_f64() * 1000.0;
                Ok((addr.ip().to_string(), rtt_ms))
            }
            Ok((Some(_), None, duration)) => {
                // Echo Reply但没有源地址
                let rtt_ms = duration.as_secs_f64() * 1000.0;
                Ok((target_addr.to_string(), rtt_ms))
            }
            _ => Err(anyhow!("请求超时")),
        }
    }

    fn calculate_avg_rtt(&self, rtts: &[Option<f64>; 3]) -> Option<f64> {
        let valid_rtts: Vec<f64> = rtts.iter().filter_map(|&x| x).collect();
        if !valid_rtts.is_empty() {
            Some(valid_rtts.iter().sum::<f64>() / valid_rtts.len() as f64)
        } else {
            None
        }
    }
}
