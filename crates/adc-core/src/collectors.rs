use serde::{Deserialize, Serialize};

use crate::{AdcError, AdcResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CpuSample {
    pub cpu_count: usize,
    pub total_jiffies: u64,
    pub idle_jiffies: u64,
}

impl CpuSample {
    pub fn usage_percent_between(before: &Self, after: &Self) -> Option<f64> {
        let total_delta = after.total_jiffies.checked_sub(before.total_jiffies)?;
        let idle_delta = after.idle_jiffies.checked_sub(before.idle_jiffies)?;
        if total_delta == 0 {
            return None;
        }
        let busy_delta = total_delta.saturating_sub(idle_delta);
        Some((busy_delta as f64 / total_delta as f64) * 100.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySample {
    pub mem_total_kb: u64,
    pub mem_free_kb: u64,
    pub mem_available_kb: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkDeviceSample {
    pub interfaces: Vec<NetworkSample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkSample {
    pub interface: String,
    pub rx_bytes: u64,
    pub rx_packets: u64,
    pub rx_errors: u64,
    pub rx_drops: u64,
    pub tx_bytes: u64,
    pub tx_packets: u64,
    pub tx_errors: u64,
    pub tx_drops: u64,
}

pub fn parse_proc_stat(contents: &str) -> AdcResult<CpuSample> {
    let mut aggregate = None;
    let mut cpu_count = 0;

    for line in contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let Some(label) = fields.first() else {
            continue;
        };
        if *label == "cpu" {
            let values = parse_u64_fields(&fields[1..], "cpu aggregate")?;
            let total = values.iter().sum();
            let idle = values.get(3).copied().unwrap_or(0) + values.get(4).copied().unwrap_or(0);
            aggregate = Some((total, idle));
        } else if label.strip_prefix("cpu").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
        }) {
            cpu_count += 1;
        }
    }

    let (total_jiffies, idle_jiffies) = aggregate
        .ok_or_else(|| AdcError::Artifact("failed to parse /proc/stat: missing cpu line".into()))?;

    Ok(CpuSample {
        cpu_count,
        total_jiffies,
        idle_jiffies,
    })
}

pub fn parse_meminfo(contents: &str) -> AdcResult<MemorySample> {
    Ok(MemorySample {
        mem_total_kb: find_meminfo_kb(contents, "MemTotal")?,
        mem_free_kb: find_meminfo_kb(contents, "MemFree")?,
        mem_available_kb: find_meminfo_kb(contents, "MemAvailable")?,
    })
}

pub fn parse_net_dev(contents: &str) -> AdcResult<NetworkDeviceSample> {
    let mut interfaces = Vec::new();

    for line in contents.lines() {
        let Some((name, counters)) = line.split_once(':') else {
            continue;
        };
        let values = parse_u64_fields(
            &counters.split_whitespace().collect::<Vec<_>>(),
            "/proc/net/dev",
        )?;
        if values.len() < 16 {
            return Err(AdcError::Artifact(format!(
                "failed to parse /proc/net/dev: interface {} has {} counters",
                name.trim(),
                values.len()
            )));
        }

        interfaces.push(NetworkSample {
            interface: name.trim().to_string(),
            rx_bytes: values[0],
            rx_packets: values[1],
            rx_errors: values[2],
            rx_drops: values[3],
            tx_bytes: values[8],
            tx_packets: values[9],
            tx_errors: values[10],
            tx_drops: values[11],
        });
    }

    Ok(NetworkDeviceSample { interfaces })
}

fn find_meminfo_kb(contents: &str, key: &str) -> AdcResult<u64> {
    for line in contents.lines() {
        let Some((line_key, value)) = line.split_once(':') else {
            continue;
        };
        if line_key == key {
            let Some(number) = value.split_whitespace().next() else {
                break;
            };
            return number.parse::<u64>().map_err(|err| {
                AdcError::Artifact(format!("failed to parse {key} from /proc/meminfo: {err}"))
            });
        }
    }
    Err(AdcError::Artifact(format!(
        "failed to parse /proc/meminfo: missing {key}"
    )))
}

fn parse_u64_fields(fields: &[&str], context: &str) -> AdcResult<Vec<u64>> {
    fields
        .iter()
        .map(|field| {
            field.parse::<u64>().map_err(|err| {
                AdcError::Artifact(format!("failed to parse integer in {context}: {err}"))
            })
        })
        .collect()
}
