use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    default_route_packs, DataQuality, EvidenceFact, NormalizedSymptom, RoutePack, SymptomKind,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteCompileInput {
    pub symptom: NormalizedSymptom,
    pub available_facts: Vec<EvidenceFact>,
    pub max_selected_packs: usize,
    pub target_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledInvestigationRoute {
    pub schema_version: String,
    pub compiler_id: String,
    pub symptom: NormalizedSymptom,
    pub target_ids: Vec<String>,
    pub ordered_domains: Vec<String>,
    pub selected_packs: Vec<CompiledRoutePack>,
    pub rejected_packs: Vec<RejectedRoutePack>,
    pub available_fact_ids: Vec<String>,
    pub missing_fact_ids: Vec<String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledRoutePack {
    pub pack_id: String,
    pub domain: String,
    pub title: String,
    pub reason: String,
    pub required_facts: Vec<String>,
    pub missing_fact_ids: Vec<String>,
    pub suggested_refs: Vec<String>,
    pub expected_cost: String,
    pub cause_neutral: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedRoutePack {
    pub pack_id: String,
    pub domain: String,
    pub reason: String,
}

pub fn compile_route_for_symptom(input: RouteCompileInput) -> CompiledInvestigationRoute {
    let mut data_quality = input.symptom.data_quality.clone();
    let available_fact_ids = input
        .available_facts
        .iter()
        .map(|fact| fact.fact_id.clone())
        .collect::<BTreeSet<_>>();
    let max_selected = input.max_selected_packs.max(1);
    let pack_registry = default_route_packs();
    let priority = priority_domains(input.symptom.kind);
    let mut ordered_packs = Vec::new();
    for domain in &priority {
        if let Some(pack) = pack_registry.iter().find(|pack| pack.domain == *domain) {
            ordered_packs.push(pack.clone());
        }
    }
    for pack in &pack_registry {
        if !ordered_packs
            .iter()
            .any(|selected| selected.domain == pack.domain)
        {
            ordered_packs.push(pack.clone());
        }
    }

    let mut selected_packs = Vec::new();
    let mut rejected_packs = Vec::new();
    let mut missing_fact_ids = BTreeSet::new();
    for pack in ordered_packs {
        if selected_packs.len() < max_selected && priority.contains(&pack.domain.as_str()) {
            let missing = pack
                .required_facts
                .iter()
                .filter(|fact| !available_fact_ids.contains(*fact))
                .cloned()
                .collect::<Vec<_>>();
            for fact_id in &missing {
                missing_fact_ids.insert(fact_id.clone());
            }
            selected_packs.push(compiled_pack(
                &pack,
                missing,
                format!(
                    "{} route pack selected for symptom {}",
                    pack.domain, input.symptom.normalized
                ),
            ));
        } else {
            rejected_packs.push(RejectedRoutePack {
                pack_id: pack.pack_id,
                domain: pack.domain,
                reason: format!(
                    "lower priority for symptom {} or outside selected pack budget {}",
                    input.symptom.normalized, max_selected
                ),
            });
        }
    }

    if !missing_fact_ids.is_empty() {
        data_quality.notes.push(format!(
            "{} required facts are not available in the current context",
            missing_fact_ids.len()
        ));
    }

    CompiledInvestigationRoute {
        schema_version: "obs.compiled_route.v1".to_string(),
        compiler_id: "symptom_to_context.v1".to_string(),
        symptom: input.symptom,
        target_ids: input.target_ids,
        ordered_domains: selected_packs
            .iter()
            .map(|pack: &CompiledRoutePack| pack.domain.clone())
            .collect(),
        selected_packs,
        rejected_packs,
        available_fact_ids: available_fact_ids.into_iter().collect(),
        missing_fact_ids: missing_fact_ids.into_iter().collect(),
        data_quality,
    }
}

fn compiled_pack(pack: &RoutePack, missing: Vec<String>, reason: String) -> CompiledRoutePack {
    CompiledRoutePack {
        pack_id: pack.pack_id.clone(),
        domain: pack.domain.clone(),
        title: pack.title.clone(),
        reason,
        required_facts: pack.required_facts.clone(),
        missing_fact_ids: missing,
        suggested_refs: pack.suggested_refs.clone(),
        expected_cost: pack.budget_hint.expected_cost.clone(),
        cause_neutral: pack.cause_neutral,
    }
}

fn priority_domains(kind: SymptomKind) -> Vec<&'static str> {
    match kind {
        SymptomKind::ServiceUnhealthy => {
            vec!["service_health", "latency_timeouts", "config_deploy_drift"]
        }
        SymptomKind::LatencyTimeout => vec![
            "latency_timeouts",
            "service_health",
            "cpu_saturation",
            "network_degradation",
        ],
        SymptomKind::MemoryGrowth => {
            vec!["memory_growth", "service_health", "cpu_saturation"]
        }
        SymptomKind::CpuSaturation => {
            vec!["cpu_saturation", "service_health", "latency_timeouts"]
        }
        SymptomKind::NetworkDegradation => {
            vec!["network_degradation", "latency_timeouts", "service_health"]
        }
        SymptomKind::DiskIoPressure => {
            vec!["disk_io_pressure", "latency_timeouts", "service_health"]
        }
        SymptomKind::ConfigDrift => {
            vec!["config_deploy_drift", "service_health", "latency_timeouts"]
        }
        SymptomKind::ThermalPower => {
            vec!["thermal_power_edge", "cpu_saturation", "service_health"]
        }
        SymptomKind::SensorGap => {
            vec!["latency_timeouts", "network_degradation", "service_health"]
        }
        SymptomKind::Unknown => vec![
            "service_health",
            "latency_timeouts",
            "memory_growth",
            "cpu_saturation",
            "network_degradation",
        ],
    }
}
