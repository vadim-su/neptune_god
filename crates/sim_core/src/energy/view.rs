//! Read-only energy network snapshot for UI overlays and debug views.

use crate::ids::BuildingId;

use super::model::{
    EnergyConsumerRuntime, EnergyEdge, EnergyNetwork, EnergyNode, EnergySourceRuntime,
    EnergyStorageRuntime,
};
use super::units::PowerUnits;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnergyView {
    pub nodes: Vec<EnergyNode>,
    pub edges: Vec<EnergyEdge>,
    pub sources: Vec<EnergySourceRuntime>,
    pub consumers: Vec<EnergyConsumerRuntime>,
    pub storages: Vec<EnergyStorageRuntime>,
    pub unconnected_consumers: Vec<BuildingId>,
    pub total_losses: PowerUnits,
}

impl EnergyView {
    pub fn from_network(network: &EnergyNetwork) -> Self {
        Self {
            nodes: network.nodes.values().cloned().collect(),
            edges: network.edges.values().cloned().collect(),
            sources: network.sources.values().cloned().collect(),
            consumers: network.consumers.values().cloned().collect(),
            storages: network.storages.values().cloned().collect(),
            unconnected_consumers: network.unconnected_consumers.iter().copied().collect(),
            total_losses: network.last_report.lost,
        }
    }

    pub fn consumer_for_def(&self, def_id: &str) -> Option<&EnergyConsumerRuntime> {
        self.consumers
            .iter()
            .find(|consumer| consumer.def_id == def_id)
    }

    pub fn source_for_def(&self, def_id: &str) -> Option<&EnergySourceRuntime> {
        self.sources.iter().find(|source| source.def_id == def_id)
    }

    pub fn battery_for_def(&self, def_id: &str) -> Option<&EnergyStorageRuntime> {
        self.storages
            .iter()
            .find(|storage| storage.def_id == def_id)
    }
}
