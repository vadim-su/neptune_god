//! Energy network graph (nodes, edges, consumers, generators, storage).

use std::collections::{BTreeMap, BTreeSet};

use behavior_api::BehaviorPowerInput;
use serde::{Deserialize, Serialize};

use crate::ids::{BuildingId, EnergyEdgeId, EnergyNodeId, TilePos};

use super::units::{EnergyAmount, PowerUnits, SuppliedRatio};

pub const DEFAULT_POWER_CLASS: &str = "lv";

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct PowerDef {
    pub connection: Option<PowerConnectionDef>,
    pub generator: Option<GeneratorPowerDef>,
    pub storage: Option<StoragePowerDef>,
    pub consumer: Option<ConsumerPowerDef>,
}

impl PowerDef {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn is_consumer(&self) -> bool {
        self.consumer.is_some()
    }

    pub fn is_electric(&self) -> bool {
        self.connection.is_some()
            || self.generator.is_some()
            || self.storage.is_some()
            || self.consumer.is_some()
    }

    pub fn participates_in_network(&self) -> bool {
        self.connection.is_some()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PowerConnectionDef {
    pub coverage_radius_tiles: i32,
    pub connection_range_tiles: i32,
    pub edge_capacity: PowerUnits,
    pub loss_per_tile: PowerUnits,
    /// Local/output class for this connection. Same-class links are bidirectional.
    #[serde(default = "default_power_class")]
    pub power_class: String,
    /// Classes this connection can transform from into `power_class`.
    #[serde(default)]
    pub input_power_classes: Vec<String>,
}

fn default_power_class() -> String {
    DEFAULT_POWER_CLASS.to_string()
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct GeneratorPowerDef {
    pub max_output: PowerUnits,
    pub initial_output: PowerUnits,
    #[serde(default)]
    pub mode: GeneratorMode,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum GeneratorMode {
    #[default]
    Constant,
    Solar,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct StoragePowerDef {
    pub capacity: EnergyAmount,
    pub max_charge: PowerUnits,
    pub max_discharge: PowerUnits,
    pub initial_charge: EnergyAmount,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ConsumerPowerDef {
    pub demand: PowerUnits,
    pub priority: u8,
    pub offline_below: SuppliedRatio,
    #[serde(default)]
    pub power_sensitivity: PowerSensitivity,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum PowerSensitivity {
    #[default]
    Linear,
    Threshold,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum EnergyConsumerState {
    Powered,
    Degraded,
    Offline,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct EnergyConsumerRuntime {
    pub building: BuildingId,
    pub def_id: String,
    pub demand: PowerUnits,
    pub supplied: PowerUnits,
    pub supplied_ratio: SuppliedRatio,
    #[serde(default)]
    pub effective_ratio: SuppliedRatio,
    pub state: EnergyConsumerState,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct EnergyStorageRuntime {
    pub building: BuildingId,
    pub def_id: String,
    pub stored: EnergyAmount,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct EnergyNetworkSnapshot {
    pub storages: BTreeMap<BuildingId, EnergyStorageSnapshot>,
    pub consumers: BTreeMap<BuildingId, EnergyConsumerSnapshot>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct EnergyStorageSnapshot {
    pub building: BuildingId,
    pub def_id: String,
    pub stored: EnergyAmount,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct EnergyConsumerSnapshot {
    pub building: BuildingId,
    pub def_id: String,
    pub supplied: PowerUnits,
    pub supplied_ratio: SuppliedRatio,
    #[serde(default)]
    pub effective_ratio: SuppliedRatio,
    pub state: EnergyConsumerState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnergySourceRuntime {
    pub building: BuildingId,
    pub def_id: String,
    pub max_output: PowerUnits,
    pub used_output: PowerUnits,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnergySolveReport {
    pub delivered: PowerUnits,
    pub lost: PowerUnits,
    pub constrained_edges: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnergyNode {
    pub id: EnergyNodeId,
    pub pos: TilePos,
    pub building: Option<BuildingId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnergyEdge {
    pub id: EnergyEdgeId,
    pub a: EnergyNodeId,
    pub b: EnergyNodeId,
    pub length_tiles: i32,
    pub capacity: PowerUnits,
    pub loss_per_unit: PowerUnits,
    pub allows_a_to_b: bool,
    pub allows_b_to_a: bool,
    pub current_flow: PowerUnits,
    pub constrained: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnergyNetwork {
    pub topology_dirty: bool,
    pub nodes: BTreeMap<EnergyNodeId, EnergyNode>,
    pub node_by_building: BTreeMap<BuildingId, EnergyNodeId>,
    pub edges: BTreeMap<EnergyEdgeId, EnergyEdge>,
    pub sources: BTreeMap<BuildingId, EnergySourceRuntime>,
    pub consumers: BTreeMap<BuildingId, EnergyConsumerRuntime>,
    pub storages: BTreeMap<BuildingId, EnergyStorageRuntime>,
    pub unconnected_consumers: BTreeSet<BuildingId>,
    pub last_report: EnergySolveReport,
    pub building_def_by_id: BTreeMap<BuildingId, String>,
    next_node: u32,
    next_edge: u32,
}

impl EnergyNetwork {
    pub fn mark_dirty(&mut self) {
        self.topology_dirty = true;
    }

    pub fn clear_topology_preserving_allocators(&mut self) {
        self.nodes.clear();
        self.node_by_building.clear();
        self.edges.clear();
        self.sources.clear();
        self.consumers.clear();
        self.storages.clear();
        self.unconnected_consumers.clear();
        self.last_report = EnergySolveReport::default();
        self.building_def_by_id.clear();
    }

    pub fn next_node_id(&mut self) -> EnergyNodeId {
        let id = EnergyNodeId(self.next_node);
        self.next_node += 1;
        id
    }

    pub fn next_edge_id(&mut self) -> EnergyEdgeId {
        let id = EnergyEdgeId(self.next_edge);
        self.next_edge += 1;
        id
    }

    pub fn snapshot(&self) -> EnergyNetworkSnapshot {
        EnergyNetworkSnapshot {
            storages: self
                .storages
                .iter()
                .map(|(building, storage)| {
                    (
                        *building,
                        EnergyStorageSnapshot {
                            building: storage.building,
                            def_id: storage.def_id.clone(),
                            stored: storage.stored,
                        },
                    )
                })
                .collect(),
            consumers: self
                .consumers
                .iter()
                .map(|(building, consumer)| {
                    (
                        *building,
                        EnergyConsumerSnapshot {
                            building: consumer.building,
                            def_id: consumer.def_id.clone(),
                            supplied: consumer.supplied,
                            supplied_ratio: consumer.supplied_ratio,
                            effective_ratio: consumer.effective_ratio,
                            state: consumer.state,
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn apply_snapshot(&mut self, snapshot: EnergyNetworkSnapshot) {
        for (building, saved) in snapshot.storages {
            let Some(storage) = self.storages.get_mut(&building) else {
                continue;
            };
            if saved.building == building && saved.def_id == storage.def_id {
                storage.stored = saved.stored;
            }
        }

        for (building, saved) in snapshot.consumers {
            let Some(consumer) = self.consumers.get_mut(&building) else {
                continue;
            };
            if saved.building == building && saved.def_id == consumer.def_id {
                consumer.supplied = saved.supplied;
                consumer.supplied_ratio = saved.supplied_ratio;
                consumer.effective_ratio = saved.effective_ratio;
                consumer.state = saved.state;
            }
        }
    }

    pub fn set_source_output(&mut self, building: BuildingId, max_output: PowerUnits) -> bool {
        let Some(source) = self.sources.get_mut(&building) else {
            return false;
        };
        source.max_output = max_output;
        true
    }

    pub fn behavior_power_input(&self, building: BuildingId) -> BehaviorPowerInput {
        let Some(consumer) = self.consumers.get(&building) else {
            return BehaviorPowerInput::default();
        };
        BehaviorPowerInput {
            required: power_units_to_u32(consumer.demand),
            supplied: power_units_to_u32(consumer.supplied),
            supplied_ratio_ppm: consumer.effective_ratio.ppm(),
            offline: consumer.state == EnergyConsumerState::Offline,
        }
    }
}

fn power_units_to_u32(value: PowerUnits) -> u32 {
    value.raw().clamp(0, u32::MAX as i64) as u32
}
