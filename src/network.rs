use std::cmp::Reverse;
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4, TcpListener};

use ipconfig::OperStatus;
use rand::Rng;

use crate::errors::{LanQrError, Result};
use crate::models::NetworkCandidate;

const PORT_RANGE_START: u16 = 20_000;
const PORT_RANGE_END: u16 = 59_999;
const MAX_PORT_ATTEMPTS: usize = 64;

pub fn discover_ipv4_candidates() -> Result<Vec<NetworkCandidate>> {
    let adapters = ipconfig::get_adapters()
        .map_err(|error| LanQrError::Message(format!("读取网络适配器失败：{error}")))?;

    let mut scored = Vec::new();

    for adapter in adapters {
        if adapter.oper_status() != OperStatus::IfOperStatusUp {
            continue;
        }

        let friendly_name = adapter.friendly_name().to_string();
        let lowered_name = friendly_name.to_lowercase();

        for ip in adapter.ip_addresses() {
            let IpAddr::V4(ipv4) = ip else {
                continue;
            };

            if ipv4.is_loopback() || ipv4.is_link_local() {
                continue;
            }

            let score = candidate_score(*ipv4, &lowered_name);
            if score <= 0 {
                continue;
            }

            scored.push((
                Reverse(score),
                NetworkCandidate {
                    ip: *ipv4,
                    label: format!("{friendly_name} ({ipv4})"),
                },
            ));
        }
    }

    scored.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.label.cmp(&right.1.label)));

    let mut result = Vec::new();
    for (_, candidate) in scored {
        if !result.iter().any(|item: &NetworkCandidate| item.ip == candidate.ip) {
            result.push(candidate);
        }
    }

    if result.is_empty() {
        return Err(LanQrError::NoLanIpv4);
    }

    Ok(result)
}

pub fn bind_available_listener(bind_ip: Ipv4Addr) -> Result<TcpListener> {
    let mut rng = rand::thread_rng();

    for _ in 0..MAX_PORT_ATTEMPTS {
        let port = rng.gen_range(PORT_RANGE_START..=PORT_RANGE_END);
        let address = SocketAddrV4::new(bind_ip, port);
        if let Ok(listener) = TcpListener::bind(address) {
            return Ok(listener);
        }
    }

    Err(LanQrError::PortAllocationFailed)
}

fn candidate_score(ip: Ipv4Addr, lowered_name: &str) -> i32 {
    let mut score = 0;

    if is_private_ipv4(ip) {
        score += 100;
    }

    if lowered_name.contains("wi-fi") || lowered_name.contains("wlan") || lowered_name.contains("ethernet") {
        score += 20;
    }

    if lowered_name.contains("virtual")
        || lowered_name.contains("vmware")
        || lowered_name.contains("hyper-v")
        || lowered_name.contains("loopback")
        || lowered_name.contains("pseudo")
        || lowered_name.contains("wsl")
        || lowered_name.contains("tunnel")
        || lowered_name.contains("bluetooth")
        || lowered_name.contains("tailscale")
        || lowered_name.contains("zerotier")
        || lowered_name.contains("meta")
    {
        score -= 100;
    }

    score
}

fn is_private_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 10
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
}
