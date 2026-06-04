use std::{collections::BTreeSet, net::Ipv4Addr, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{AdcError, AdcResult, DataQuality};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetDiscoveryResult {
    pub schema_version: String,
    pub network_cidr: String,
    pub candidate_count: usize,
    pub candidates: Vec<DiscoveredTarget>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredTarget {
    pub target_id: String,
    pub host: String,
    pub transport: String,
    pub confidence: String,
    pub source: String,
    pub data_quality: DataQuality,
}

pub fn discover_same_network_targets_from_neighbors(
    network_cidr: &str,
    neighbor_text: &str,
) -> AdcResult<TargetDiscoveryResult> {
    let network = Ipv4Network::parse(network_cidr)?;
    let mut data_quality = DataQuality {
        clock_confidence: crate::ClockConfidence::Medium,
        ..Default::default()
    };
    let mut seen = BTreeSet::new();
    let mut candidates = Vec::new();

    for line in neighbor_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some(entry) = NeighborEntry::parse(line) else {
            data_quality
                .notes
                .push("skipped unrecognized neighbor table line".to_string());
            continue;
        };
        if !network.contains(entry.addr) {
            continue;
        }
        if entry.is_failed() {
            data_quality.missing.push(format!(
                "neighbor {} is not currently reachable",
                entry.addr
            ));
            continue;
        }
        if seen.insert(entry.addr) {
            let host = entry.addr.to_string();
            candidates.push(DiscoveredTarget {
                target_id: format!("target-{}", host.replace('.', "-")),
                host,
                transport: "mcp_stdio_over_ssh".to_string(),
                confidence: "medium".to_string(),
                source: "neighbor_table".to_string(),
                data_quality: DataQuality {
                    clock_confidence: crate::ClockConfidence::Medium,
                    notes: vec![format!("neighbor_state={}", entry.state)],
                    ..Default::default()
                },
            });
        }
    }

    Ok(TargetDiscoveryResult {
        schema_version: "obs.discovery.v2".to_string(),
        network_cidr: network.to_string(),
        candidate_count: candidates.len(),
        candidates,
        data_quality,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ipv4Network {
    base: Ipv4Addr,
    prefix: u8,
}

impl Ipv4Network {
    fn parse(value: &str) -> AdcResult<Self> {
        let (base, prefix) = value
            .split_once('/')
            .ok_or_else(|| AdcError::ProfileValidation("network CIDR is required".to_string()))?;
        let base = Ipv4Addr::from_str(base).map_err(|err| {
            AdcError::ProfileValidation(format!("invalid network CIDR address: {err}"))
        })?;
        let prefix = prefix.parse::<u8>().map_err(|err| {
            AdcError::ProfileValidation(format!("invalid network CIDR prefix: {err}"))
        })?;
        if prefix > 32 {
            return Err(AdcError::ProfileValidation(
                "network CIDR prefix must be between 0 and 32".to_string(),
            ));
        }
        if prefix < 24 {
            return Err(AdcError::ProfileValidation(
                "network discovery requires /24 or narrower CIDR".to_string(),
            ));
        }
        Ok(Self { base, prefix })
    }

    fn contains(&self, addr: Ipv4Addr) -> bool {
        let mask = if self.prefix == 0 {
            0
        } else {
            u32::MAX << (32 - self.prefix)
        };
        u32::from(self.base) & mask == u32::from(addr) & mask
    }
}

impl std::fmt::Display for Ipv4Network {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}/{}", self.base, self.prefix)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NeighborEntry {
    addr: Ipv4Addr,
    state: String,
}

impl NeighborEntry {
    fn parse(line: &str) -> Option<Self> {
        parse_ip_neigh_line(line).or_else(|| parse_proc_net_arp_line(line))
    }

    fn is_failed(&self) -> bool {
        matches!(
            self.state.as_str(),
            "FAILED" | "INCOMPLETE" | "0x0" | "incomplete"
        )
    }
}

fn parse_ip_neigh_line(line: &str) -> Option<NeighborEntry> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    let addr = fields.first()?.parse::<Ipv4Addr>().ok()?;
    let state = fields.last()?.to_string();
    Some(NeighborEntry { addr, state })
}

fn parse_proc_net_arp_line(line: &str) -> Option<NeighborEntry> {
    if line.starts_with("IP address") {
        return None;
    }
    let fields = line.split_whitespace().collect::<Vec<_>>();
    let addr = fields.first()?.parse::<Ipv4Addr>().ok()?;
    let flags = fields.get(2)?.to_string();
    Some(NeighborEntry { addr, state: flags })
}
