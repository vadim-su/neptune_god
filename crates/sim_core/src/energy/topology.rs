//! Rebuild energy nodes and edges from placed buildings and catalog power defs.

use std::collections::BTreeMap;

use crate::building::SimBuilding;
use crate::catalog::CoreCatalog;
use crate::ids::{BuildingId, EnergyNodeId, TilePos};

use super::model::{
    EnergyConsumerRuntime, EnergyConsumerState, EnergyEdge, EnergyNetwork, EnergyNode,
    EnergySourceRuntime, EnergyStorageRuntime,
};
use super::units::{PowerUnits, SuppliedRatio};

pub fn rebuild_energy_topology(
    network: &mut EnergyNetwork,
    catalog: &CoreCatalog,
    buildings: &BTreeMap<BuildingId, SimBuilding>,
) {
    let existing_node_by_building = network.node_by_building.clone();
    let existing_storage = network
        .storages
        .iter()
        .map(|(building, storage)| (*building, storage.stored))
        .collect::<BTreeMap<_, _>>();

    network.clear_topology_preserving_allocators();

    let mut pole_nodes = Vec::new();
    let mut attachment_nodes = Vec::new();

    for (building_id, building) in buildings {
        let Some(def) = catalog.building_by_id(&building.def_id) else {
            continue;
        };
        if !def.power.participates_in_network() {
            continue;
        }

        let node_id = existing_node_by_building
            .get(building_id)
            .copied()
            .unwrap_or_else(|| network.next_node_id());
        network.nodes.insert(
            node_id,
            EnergyNode {
                id: node_id,
                pos: building.origin,
                building: Some(*building_id),
            },
        );
        network.node_by_building.insert(*building_id, node_id);
        network
            .building_def_by_id
            .insert(*building_id, building.def_id.clone());

        let Some(connection) = def.power.connection.as_ref() else {
            continue;
        };
        let power_class = connection.power_class.clone();
        let is_hub = connection
            .connection_range_tiles
            .max(connection.coverage_radius_tiles)
            > 0;
        if is_hub {
            pole_nodes.push((
                *building_id,
                node_id,
                building.origin,
                power_class.clone(),
                connection.input_power_classes.clone(),
            ));
        }

        if let Some(generator) = &def.power.generator {
            network.sources.insert(
                *building_id,
                EnergySourceRuntime {
                    building: *building_id,
                    def_id: building.def_id.clone(),
                    max_output: generator.initial_output,
                    used_output: PowerUnits::ZERO,
                },
            );
        }

        if let Some(storage) = &def.power.storage {
            network.storages.insert(
                *building_id,
                EnergyStorageRuntime {
                    building: *building_id,
                    def_id: building.def_id.clone(),
                    stored: existing_storage
                        .get(building_id)
                        .copied()
                        .unwrap_or(storage.initial_charge),
                },
            );
        }

        if let Some(consumer) = &def.power.consumer {
            network.consumers.insert(
                *building_id,
                EnergyConsumerRuntime {
                    building: *building_id,
                    def_id: building.def_id.clone(),
                    demand: consumer.demand,
                    supplied: PowerUnits::ZERO,
                    supplied_ratio: SuppliedRatio::ZERO,
                    effective_ratio: SuppliedRatio::ZERO,
                    state: EnergyConsumerState::Offline,
                },
            );
        }

        if !is_hub
            && (def.power.generator.is_some()
                || def.power.storage.is_some()
                || def.power.consumer.is_some())
        {
            attachment_nodes.push((*building_id, node_id, building.origin, power_class));
        }
    }

    for left in 0..pole_nodes.len() {
        for right in (left + 1)..pole_nodes.len() {
            let (_, left_node, left_pos, ref left_class, ref left_inputs) = pole_nodes[left];
            let (_, right_node, right_pos, ref right_class, ref right_inputs) = pole_nodes[right];
            let Some(direction) =
                power_class_direction(left_class, left_inputs, right_class, right_inputs)
            else {
                continue;
            };
            let distance = tile_range_distance(left_pos, right_pos);
            let Some(left_def) = pole_def(catalog, buildings, pole_nodes[left].0) else {
                continue;
            };
            let Some(right_def) = pole_def(catalog, buildings, pole_nodes[right].0) else {
                continue;
            };

            if distance <= left_def.connection_range.min(right_def.connection_range) {
                insert_edge(
                    network,
                    left_node,
                    right_node,
                    distance.max(1),
                    left_def.edge_capacity.min(right_def.edge_capacity),
                    max_power_units(left_def.loss_per_tile, right_def.loss_per_tile),
                    direction,
                );
            }
        }
    }

    for (building, node, pos, power_class) in attachment_nodes {
        let mut best = None;
        for (pole_building, pole_node, pole_pos, pole_class, pole_inputs) in &pole_nodes {
            if *pole_building == building {
                continue;
            }
            let Some(direction) = power_class_direction(
                &power_class,
                &[],
                pole_class.as_str(),
                pole_inputs.as_slice(),
            ) else {
                continue;
            };
            let Some(pole_def) = pole_def(catalog, buildings, *pole_building) else {
                continue;
            };
            let distance = tile_range_distance(pos, *pole_pos);
            if distance <= pole_def.coverage_radius {
                best = match best {
                    Some((best_distance, best_pole, _, _, _))
                        if (best_distance, best_pole) <= (distance, *pole_node) =>
                    {
                        best
                    }
                    _ => Some((
                        distance,
                        *pole_node,
                        pole_def.edge_capacity,
                        pole_def.loss_per_tile,
                        direction,
                    )),
                };
            }
        }

        if let Some((distance, pole_node, capacity, loss, direction)) = best {
            insert_edge(
                network,
                node,
                pole_node,
                distance.max(1),
                capacity,
                loss,
                direction,
            );
        } else if network.consumers.contains_key(&building) {
            network.unconnected_consumers.insert(building);
        }
    }

    network.topology_dirty = false;
}

fn insert_edge(
    network: &mut EnergyNetwork,
    a: EnergyNodeId,
    b: EnergyNodeId,
    length_tiles: i32,
    capacity: PowerUnits,
    loss_per_unit: PowerUnits,
    direction: EdgeDirection,
) {
    let id = network.next_edge_id();
    network.edges.insert(
        id,
        EnergyEdge {
            id,
            a,
            b,
            length_tiles,
            capacity,
            loss_per_unit,
            allows_a_to_b: direction.allows_a_to_b,
            allows_b_to_a: direction.allows_b_to_a,
            current_flow: PowerUnits::ZERO,
            constrained: false,
        },
    );
}

fn pole_def(
    catalog: &CoreCatalog,
    buildings: &BTreeMap<BuildingId, SimBuilding>,
    building: BuildingId,
) -> Option<PoleTopologyDef> {
    let building = buildings.get(&building)?;
    let connection = catalog
        .building_by_id(&building.def_id)?
        .power
        .connection
        .as_ref()?;
    Some(PoleTopologyDef {
        edge_capacity: connection.edge_capacity,
        loss_per_tile: connection.loss_per_tile,
        coverage_radius: connection.coverage_radius_tiles,
        connection_range: connection.connection_range_tiles,
        power_class: connection.power_class.clone(),
    })
}

#[derive(Clone, Debug)]
struct PoleTopologyDef {
    edge_capacity: PowerUnits,
    loss_per_tile: PowerUnits,
    coverage_radius: i32,
    connection_range: i32,
    #[allow(dead_code)]
    power_class: String,
}

fn tile_range_distance(a: TilePos, b: TilePos) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

fn max_power_units(a: PowerUnits, b: PowerUnits) -> PowerUnits {
    if a >= b { a } else { b }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EdgeDirection {
    allows_a_to_b: bool,
    allows_b_to_a: bool,
}

fn power_class_direction(
    left_class: &str,
    left_inputs: &[String],
    right_class: &str,
    right_inputs: &[String],
) -> Option<EdgeDirection> {
    let same_class = left_class == right_class;
    let allows_left_to_right = same_class || right_inputs.iter().any(|class| class == left_class);
    let allows_right_to_left = same_class || left_inputs.iter().any(|class| class == right_class);
    (allows_left_to_right || allows_right_to_left).then_some(EdgeDirection {
        allows_a_to_b: allows_left_to_right,
        allows_b_to_a: allows_right_to_left,
    })
}
