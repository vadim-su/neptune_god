//! [`SimWorld`] digest implementation delegating to `digesting` helpers.

use super::digesting::{
    combine_building_state_digest, combine_energy_consumer_state_digest,
    combine_splitter_runtime_digest, combine_string_digest, combine_tile_digest,
    combine_underground_transport_runtime_digest, core_building_kind_digest,
    core_inventory_role_digest, core_item_kind_digest, direction_digest,
};
use super::ports::core_port_role_order;
use super::*;

impl SimWorld {
    pub fn digest(&self) -> WorldDigest {
        let mut digest = WorldDigest::default();
        digest.combine_u64(self.tick.raw());
        digest.combine_u64(self.time_of_day.raw_tick() as u64);
        digest.combine_u64(self.day_night_settings.day_length_ticks as u64);
        match self.day_night_settings.solar_curve {
            SolarCurveSettings::GameLike {
                sunrise_start,
                full_day_start,
                full_day_end,
                sunset_end,
            } => {
                digest.combine_u64(1);
                digest.combine_u64(sunrise_start.to_bits() as u64);
                digest.combine_u64(full_day_start.to_bits() as u64);
                digest.combine_u64(full_day_end.to_bits() as u64);
                digest.combine_u64(sunset_end.to_bits() as u64);
            }
        }
        digest.combine_u64(self.topology_revision_seen);
        digest.combine_u64(self.topology_graph.revision());

        for line_id in self.transport.line_ids_sorted() {
            let Some(line) = self.transport.line(line_id) else {
                continue;
            };

            digest.combine_u64(line_id.0 as u64);
            digest.combine_u64(line.speed().raw() as i64 as u64);
            digest.combine_u64(line.path().tiles().len() as u64);
            for tile in line.path().tiles() {
                digest.combine_u64(tile.pos.x as i64 as u64);
                digest.combine_u64(tile.pos.y as i64 as u64);
            }
            digest.combine_u64(line.revision());
            for lane_index in 0..2 {
                let lane = line.lane(lane_index);
                digest.combine_u64(lane.item_count() as u64);
                digest.combine_u64(lane.front_gap().raw() as i64 as u64);
                digest.combine_u64(lane.back_gap().raw() as i64 as u64);
                digest.combine_u64(
                    lane.cached_frontmost_positive_gap()
                        .map(|index| index as u64)
                        .unwrap_or(u64::MAX),
                );
                digest.combine_u64(lane.items().len() as u64);
                for item in lane.items() {
                    digest.combine_u64(item.0 as u64);
                }
                digest.combine_u64(lane.gaps_after().len() as u64);
                for gap in lane.gaps_after() {
                    digest.combine_u64(gap.raw() as i64 as u64);
                }
            }
        }
        digest.combine_u64(self.transport.nodes_sorted().count() as u64);
        for node in self.transport.nodes_sorted() {
            digest.combine_u64(node.id.0);
            digest.combine_u64(node.kind.sort_order() as u64);
            match node.kind {
                crate::transport::node::TransportNodeKind::BlockedFront => {
                    digest.combine_u64(0);
                    digest.combine_u64(0);
                }
                crate::transport::node::TransportNodeKind::EndTransfer => {
                    digest.combine_u64(1);
                    digest.combine_u64(0);
                }
                crate::transport::node::TransportNodeKind::SideLoad { near_lane } => {
                    digest.combine_u64(2);
                    digest.combine_u64(near_lane as u64);
                }
                crate::transport::node::TransportNodeKind::Splitter2x1 => {
                    digest.combine_u64(3);
                    digest.combine_u64(0);
                }
                crate::transport::node::TransportNodeKind::Underground => {
                    digest.combine_u64(4);
                    digest.combine_u64(0);
                }
                crate::transport::node::TransportNodeKind::ConveyorLift => {
                    digest.combine_u64(5);
                    digest.combine_u64(0);
                }
            }
            combine_tile_digest(&mut digest, node.sort_tile);
            match node.direction {
                Some(direction) => {
                    digest.combine_u64(1);
                    digest.combine_u64(direction_digest(direction));
                }
                None => digest.combine_u64(0),
            }
            digest.combine_u64(node.ports.len() as u64);
            for port in &node.ports {
                digest.combine_u64(port.node.0);
                digest.combine_u64(match port.role {
                    crate::transport::node::TransportPortRole::Input => 0,
                    crate::transport::node::TransportPortRole::Output => 1,
                });
                combine_tile_digest(&mut digest, port.tile);
                match port.side {
                    Some(direction) => {
                        digest.combine_u64(1);
                        digest.combine_u64(direction_digest(direction));
                    }
                    None => digest.combine_u64(0),
                }
                digest.combine_u64(port.lane as u64);
                digest.combine_u64(port.line.0 as u64);
            }
            match &node.runtime {
                crate::transport::node::TransportNodeRuntime::None => {
                    digest.combine_u64(0);
                }
                crate::transport::node::TransportNodeRuntime::Splitter(runtime) => {
                    digest.combine_u64(1);
                    combine_splitter_runtime_digest(&mut digest, runtime);
                }
                crate::transport::node::TransportNodeRuntime::Underground(runtime) => {
                    digest.combine_u64(2);
                    combine_underground_transport_runtime_digest(&mut digest, runtime);
                }
            }
        }
        digest.combine_u64(self.transport.interactions_sorted().count() as u64);
        for interaction in self.transport.interactions_sorted() {
            digest.combine_u64(match interaction.kind() {
                crate::transport::interaction::BeltInteractionKind::BlockedFront => 0,
                crate::transport::interaction::BeltInteractionKind::EndTransfer => 1,
                crate::transport::interaction::BeltInteractionKind::SideLoad { near_lane } => {
                    10 + near_lane as u64
                }
            });
            digest.combine_u64(interaction.source_line().0 as u64);
            digest.combine_u64(
                interaction
                    .target_line()
                    .map(|line_id| line_id.0 as u64)
                    .unwrap_or(u64::MAX),
            );
            combine_tile_digest(&mut digest, interaction.target_sort_tile());
            match interaction.target_tile() {
                Some(tile) => {
                    digest.combine_u64(1);
                    combine_tile_digest(&mut digest, tile);
                }
                None => digest.combine_u64(0),
            }
        }
        self.combine_core_building_digest(&mut digest);
        self.combine_character_inventory_digest(&mut digest);
        self.combine_energy_digest(&mut digest, &self.canonical_energy_network());
        self.combine_resource_digest(&mut digest);

        digest
    }

    pub(super) fn combine_character_inventory_digest(&self, digest: &mut WorldDigest) {
        digest.combine_u64(self.character_inventory.equipment.len() as u64);
        for (slot, item) in &self.character_inventory.equipment {
            combine_string_digest(digest, slot.0.as_str());
            digest.combine_u64(core_item_kind_digest(item.item));
        }

        digest.combine_u64(self.character_inventory.containers.len() as u64);
        for container in &self.character_inventory.containers {
            combine_string_digest(digest, container.id.as_str());
            combine_string_digest(digest, &container.name);
            combine_string_digest(digest, container.source_slot.0.as_str());
            digest.combine_u64(core_item_kind_digest(container.source_item));
            digest.combine_u64(container.pickup_priority as u64);
            digest.combine_u64(u64::from(container.quick_access));
            let snapshot = container.inventory.snapshot();
            digest.combine_u64(core_inventory_role_digest(snapshot.role));
            digest.combine_u64(snapshot.slots.len() as u64);
            for (slot, instance) in snapshot.slots.into_iter().zip(snapshot.slot_instances) {
                match slot {
                    Some(stack) => {
                        digest.combine_u64(1);
                        digest.combine_u64(core_item_kind_digest(stack.kind));
                        digest.combine_u64(stack.amount as u64);
                        digest.combine_u64(
                            instance
                                .map(|instance| instance.0 as u64)
                                .unwrap_or(u64::MAX),
                        );
                    }
                    None => digest.combine_u64(0),
                }
            }
        }

        digest.combine_u64(self.loaded_containers.len() as u64);
        for (instance, container) in &self.loaded_containers {
            digest.combine_u64(instance.0 as u64);
            digest.combine_u64(core_item_kind_digest(container.item));
            digest.combine_u64(container.containers.len() as u64);
            for section in &container.containers {
                combine_string_digest(digest, section.container_id.as_str());
                let snapshot = section.inventory.snapshot();
                digest.combine_u64(core_inventory_role_digest(snapshot.role));
                digest.combine_u64(snapshot.slots.len() as u64);
                for (slot, nested_instance) in
                    snapshot.slots.into_iter().zip(snapshot.slot_instances)
                {
                    match slot {
                        Some(stack) => {
                            digest.combine_u64(1);
                            digest.combine_u64(core_item_kind_digest(stack.kind));
                            digest.combine_u64(stack.amount as u64);
                            digest.combine_u64(
                                nested_instance
                                    .map(|instance| instance.0 as u64)
                                    .unwrap_or(u64::MAX),
                            );
                        }
                        None => digest.combine_u64(0),
                    }
                }
            }
        }
    }

    pub(super) fn combine_energy_digest(
        &self,
        digest: &mut WorldDigest,
        energy: &crate::energy::EnergyNetwork,
    ) {
        digest.combine_u64(energy.storages.len() as u64);
        for (building, storage) in &energy.storages {
            digest.combine_u64(building.0 as u64);
            combine_string_digest(digest, &storage.def_id);
            digest.combine_u64(storage.stored.raw() as u64);
        }

        digest.combine_u64(energy.consumers.len() as u64);
        for (building, consumer) in &energy.consumers {
            digest.combine_u64(building.0 as u64);
            combine_string_digest(digest, &consumer.def_id);
            digest.combine_u64(consumer.supplied.raw() as u64);
            digest.combine_u64(consumer.supplied_ratio.ppm() as u64);
            digest.combine_u64(consumer.effective_ratio.ppm() as u64);
            combine_energy_consumer_state_digest(digest, consumer.state);
        }
    }

    pub(super) fn combine_core_building_digest(&self, digest: &mut WorldDigest) {
        digest.combine_u64(self.buildings.len() as u64);
        for (id, building) in &self.buildings {
            digest.combine_u64(id.0 as u64);
            digest.combine_u64(core_building_kind_digest(building.kind));
            combine_string_digest(digest, &building.def_id);
            combine_tile_digest(digest, building.origin);
            digest.combine_u64(direction_digest(building.direction));
            digest.combine_u64(building.footprint.len() as u64);
            for &pos in &building.footprint {
                combine_tile_digest(digest, pos);
            }
            digest.combine_u64(building.ports.len() as u64);
            for port in &building.ports {
                digest.combine_u64(core_port_role_order(port.role) as u64);
                combine_tile_digest(digest, port.tile);
                digest.combine_u64(port.accepts.len() as u64);
                for kind in &port.accepts {
                    digest.combine_u64(core_item_kind_digest(*kind));
                }
            }
            digest.combine_u64(building.inventories.len() as u64);
            for inventory in &building.inventories {
                digest.combine_u64(inventory.0 as u64);
            }
            combine_building_state_digest(digest, &building.state);
        }

        digest.combine_u64(self.building_by_origin.len() as u64);
        for (pos, id) in &self.building_by_origin {
            combine_tile_digest(digest, *pos);
            digest.combine_u64(id.0 as u64);
        }

        digest.combine_u64(self.building_occupancy.len() as u64);
        for (pos, id) in &self.building_occupancy {
            combine_tile_digest(digest, *pos);
            digest.combine_u64(id.0 as u64);
        }

        digest.combine_u64(self.inventories.len() as u64);
        for (id, record) in &self.inventories {
            digest.combine_u64(id.0 as u64);
            digest.combine_u64(record.owner.0 as u64);
            let snapshot = record.inventory.snapshot();
            digest.combine_u64(core_inventory_role_digest(snapshot.role));
            digest.combine_u64(snapshot.slots.len() as u64);
            for (slot, instance) in snapshot.slots.into_iter().zip(snapshot.slot_instances) {
                match slot {
                    Some(stack) => {
                        digest.combine_u64(1);
                        digest.combine_u64(core_item_kind_digest(stack.kind));
                        digest.combine_u64(stack.amount as u64);
                        digest.combine_u64(
                            instance
                                .map(|instance| instance.0 as u64)
                                .unwrap_or(u64::MAX),
                        );
                    }
                    None => digest.combine_u64(0),
                }
            }
        }

        digest.combine_u64(self.removed_item_drops.len() as u64);
        for drop in &self.removed_item_drops {
            combine_tile_digest(digest, drop.origin);
            digest.combine_u64(core_item_kind_digest(drop.stack.kind));
            digest.combine_u64(drop.stack.amount as u64);
            digest.combine_u64(
                drop.instance
                    .map(|instance| instance.0 as u64)
                    .unwrap_or(u64::MAX),
            );
        }

        digest.combine_u64(self.surface_item_drops.len() as u64);
        for drop in &self.surface_item_drops {
            combine_tile_digest(digest, drop.origin);
            digest.combine_u64(core_item_kind_digest(drop.stack.kind));
            digest.combine_u64(drop.stack.amount as u64);
            digest.combine_u64(
                drop.instance
                    .map(|instance| instance.0 as u64)
                    .unwrap_or(u64::MAX),
            );
        }

        digest.combine_u64(self.occupied_surface_tiles.len() as u64);
        for tile in &self.occupied_surface_tiles {
            combine_tile_digest(digest, *tile);
        }
    }

    pub(super) fn combine_resource_digest(&self, digest: &mut WorldDigest) {
        digest.combine_u64(self.resources.len() as u64);
        for (pos, (kind, amount)) in &self.resources {
            combine_tile_digest(digest, *pos);
            digest.combine_u64(core_item_kind_digest(*kind));
            digest.combine_u64(*amount as u64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{LineId, TilePos};
    use crate::topology::graph::Direction;
    use crate::transport::node::{TransportNode, TransportNodeId, TransportNodeKind};

    #[test]
    fn digest_includes_transport_node_direction_and_ports() {
        fn world_with_splitter(direction: Direction) -> SimWorld {
            let mut world = SimWorld::default();
            world.transport.insert_node(TransportNode::splitter_2x1(
                TransportNodeId(1),
                TilePos::new(0, 0),
                direction,
                LineId(1),
                LineId(2),
                LineId(3),
                LineId(4),
            ));
            world
        }

        let east = world_with_splitter(Direction::East);
        let north = world_with_splitter(Direction::North);

        assert_ne!(east.digest(), north.digest());
    }

    #[test]
    fn digest_includes_transport_node_kind_payload() {
        fn world_with_side_load_kind(kind: TransportNodeKind) -> SimWorld {
            let mut world = SimWorld::default();
            let mut node = TransportNode::side_load(
                TransportNodeId(2),
                TilePos::new(0, 0),
                LineId(1),
                LineId(2),
                0,
            );
            node.kind = kind;
            world.transport.insert_node(node);
            world
        }

        let near_lane_zero =
            world_with_side_load_kind(TransportNodeKind::SideLoad { near_lane: 0 });
        let near_lane_one = world_with_side_load_kind(TransportNodeKind::SideLoad { near_lane: 1 });

        assert_ne!(near_lane_zero.digest(), near_lane_one.digest());
    }

    #[test]
    fn digest_includes_transport_port_node_identity() {
        fn world_with_port_node(port_node: TransportNodeId) -> SimWorld {
            let mut world = SimWorld::default();
            let mut node = TransportNode::side_load(
                TransportNodeId(3),
                TilePos::new(0, 0),
                LineId(1),
                LineId(2),
                0,
            );
            node.ports[0].node = port_node;
            world.transport.insert_node(node);
            world
        }

        let matching_port_node = world_with_port_node(TransportNodeId(3));
        let mismatched_port_node = world_with_port_node(TransportNodeId(99));

        assert_ne!(matching_port_node.digest(), mismatched_port_node.digest());
    }
}
