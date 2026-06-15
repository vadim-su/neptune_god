//! Underground belt placement, linking, and corridor-aware line building.

use super::underground_corridor::{UndergroundCorridorRecord, underground_corridor_tiles};
use super::*;
use crate::catalog::CoreBuildingDef;
use crate::inserter::carried_stack;
use crate::topology::graph::{BeltTile, UndergroundEndpointRole, UndergroundLink};
use crate::transport::node::{TransportNodeKind, TransportNodeRuntime};
use crate::units::UnitsPerTick;

impl SimWorld {
    pub fn place_underground_belt_pair(
        &mut self,
        def_id: String,
        entrance: TilePos,
        exit: TilePos,
        direction: Direction,
    ) -> Result<(), SimCommandError> {
        let def = self
            .catalog
            .building_by_id(&def_id)
            .cloned()
            .ok_or(SimCommandError::UnknownBuildingKind)?;
        let CoreBuildingDriver::Underground {
            speed_units_per_tick,
            max_range_tiles,
        } = def.behavior.driver
        else {
            return Err(SimCommandError::InvalidPort);
        };

        if entrance == exit {
            return Err(SimCommandError::InvalidPosition { pos: exit });
        }

        let delta = direction.delta();
        let actual = (exit.x - entrance.x, exit.y - entrance.y);
        if actual.0 == 0 && actual.1 == 0 {
            return Err(SimCommandError::InvalidPosition { pos: exit });
        }
        if actual.0.signum() != delta.0.signum() || actual.1.signum() != delta.1.signum() {
            return Err(SimCommandError::InvalidPosition { pos: exit });
        }
        if actual.0 * delta.1 != actual.1 * delta.0 {
            return Err(SimCommandError::InvalidPosition { pos: exit });
        }
        let distance = actual.0.unsigned_abs().max(actual.1.unsigned_abs()) as u8;
        if distance == 0 || distance > max_range_tiles {
            return Err(SimCommandError::InvalidPosition { pos: exit });
        }

        for &pos in &[entrance, exit] {
            self.ensure_buildable(pos)?;
            if self.building_occupancy.contains_key(&pos) || self.occupied_tiles.contains_key(&pos)
            {
                return Err(SimCommandError::OccupiedTile { pos });
            }
        }

        let corridor_record = UndergroundCorridorRecord {
            speed: speed_units_per_tick,
            direction,
        };
        for tile in underground_corridor_tiles(entrance, exit) {
            if self
                .underground_corridors
                .get(&tile)
                .is_some_and(|records| records.contains(&corridor_record))
            {
                return Err(SimCommandError::TopologyConflict { pos: tile });
            }
        }

        let entrance_id = self.ids.next_building();
        let exit_id = self.ids.next_building();

        self.place_belt_tile(entrance, direction, direction, speed_units_per_tick)?;
        self.place_belt_tile(exit, direction, direction, speed_units_per_tick)?;

        if self.refresh_connected_belt_inputs() {
            self.rebuild_transport_lines();
        }

        self.install_underground_building(
            &def,
            entrance_id,
            entrance,
            direction,
            UndergroundRole::Entrance,
            exit_id,
        )?;
        self.install_underground_building(
            &def,
            exit_id,
            exit,
            direction,
            UndergroundRole::Exit,
            entrance_id,
        )?;

        self.register_underground_topology_links(entrance, exit, speed_units_per_tick);
        self.reserve_underground_corridor(entrance, exit, corridor_record);
        self.rebuild_transport_lines();

        Ok(())
    }

    fn reserve_underground_corridor(
        &mut self,
        entrance: TilePos,
        exit: TilePos,
        record: UndergroundCorridorRecord,
    ) {
        for tile in underground_corridor_tiles(entrance, exit) {
            self.underground_corridors
                .entry(tile)
                .or_default()
                .insert(record);
        }
    }

    fn clear_underground_corridor(
        &mut self,
        entrance: TilePos,
        exit: TilePos,
        record: UndergroundCorridorRecord,
    ) {
        for tile in underground_corridor_tiles(entrance, exit) {
            let Some(records) = self.underground_corridors.get_mut(&tile) else {
                continue;
            };
            records.remove(&record);
            if records.is_empty() {
                self.underground_corridors.remove(&tile);
            }
        }
    }

    fn register_underground_topology_links(
        &mut self,
        entrance: TilePos,
        exit: TilePos,
        speed: UnitsPerTick,
    ) {
        self.topology_graph.set_underground_link(
            entrance,
            UndergroundLink {
                partner: exit,
                role: UndergroundEndpointRole::Entrance,
                speed,
            },
        );
        self.topology_graph.set_underground_link(
            exit,
            UndergroundLink {
                partner: entrance,
                role: UndergroundEndpointRole::Exit,
                speed,
            },
        );
    }

    fn install_underground_building(
        &mut self,
        def: &CoreBuildingDef,
        id: BuildingId,
        origin: TilePos,
        direction: Direction,
        role: UndergroundRole,
        partner: BuildingId,
    ) -> Result<(), SimCommandError> {
        let footprint = footprint_tiles(origin, &def.footprint);
        let surface_z = self.surface_z_at(origin);
        let ports = building_ports(origin, &footprint, direction, surface_z, def);
        self.building_occupancy.insert(origin, id);
        self.building_by_origin.insert(origin, id);
        self.buildings.insert(
            id,
            SimBuilding {
                id,
                def_id: def.id.clone(),
                kind: def.kind,
                origin,
                surface_z,
                direction,
                footprint,
                ports,
                inventories: Vec::new(),
                state: SimBuildingState::Underground(UndergroundRuntime {
                    role,
                    partner,
                    direction,
                }),
            },
        );
        Ok(())
    }

    pub(super) fn remove_underground_building(
        &mut self,
        id: BuildingId,
    ) -> Result<(), SimCommandError> {
        let Some(building) = self.buildings.get(&id).cloned() else {
            return Ok(());
        };
        let SimBuildingState::Underground(runtime) = building.state.clone() else {
            return Ok(());
        };
        let partner = runtime.partner;
        let speed = self
            .topology_graph
            .underground_link(building.origin)
            .map(|link| link.speed)
            .unwrap_or_else(|| UnitsPerTick::new(4));
        let corridor_record = UndergroundCorridorRecord {
            speed,
            direction: runtime.direction,
        };
        let partner_origin = self
            .buildings
            .get(&partner)
            .map(|partner_building| partner_building.origin);
        let (entrance, exit) = match runtime.role {
            UndergroundRole::Entrance => (Some(building.origin), partner_origin),
            UndergroundRole::Exit => (partner_origin, Some(building.origin)),
        };
        if let (Some(entrance), Some(exit)) = (entrance, exit) {
            self.collect_underground_runtime_drops(entrance, exit);
            self.clear_underground_corridor(entrance, exit, corridor_record);
        }

        if partner != id {
            let _ = self.remove_underground_endpoint(partner);
        }
        self.remove_underground_endpoint(id)?;
        self.refresh_connected_belt_inputs();
        self.rebuild_transport_lines();
        Ok(())
    }

    pub fn place_underground(
        &mut self,
        def_id: String,
        pos: TilePos,
        direction: Direction,
    ) -> Result<(), SimCommandError> {
        let def = self
            .catalog
            .building_by_id(&def_id)
            .cloned()
            .ok_or(SimCommandError::UnknownBuildingKind)?;
        let CoreBuildingDriver::Underground {
            speed_units_per_tick,
            max_range_tiles,
        } = def.behavior.driver
        else {
            return Err(SimCommandError::InvalidPort);
        };

        self.ensure_buildable(pos)?;
        if self.building_occupancy.contains_key(&pos) || self.occupied_tiles.contains_key(&pos) {
            return Err(SimCommandError::OccupiedTile { pos });
        }

        if let Some(partner_id) =
            self.find_solo_underground_partner(pos, direction, &def_id, max_range_tiles)
        {
            let partner_origin = self
                .buildings
                .get(&partner_id)
                .ok_or(SimCommandError::MissingBuildingId {
                    building: partner_id,
                })?
                .origin;
            self.validate_underground_pair_positions(
                pos,
                partner_origin,
                direction,
                speed_units_per_tick,
                max_range_tiles,
            )?;
        }

        let id = self.ids.next_building();
        self.place_belt_tile(pos, direction, direction, speed_units_per_tick)?;
        self.install_underground_building(&def, id, pos, direction, UndergroundRole::Entrance, id)?;

        if let Some(partner_id) =
            self.find_solo_underground_partner(pos, direction, &def_id, max_range_tiles)
        {
            self.finalize_underground_pair(id, partner_id, direction, speed_units_per_tick)?;
        } else if self.refresh_connected_belt_inputs() {
            self.rebuild_transport_lines();
        }

        Ok(())
    }

    pub fn rotate_underground(&mut self, pos: TilePos) -> Result<(), SimCommandError> {
        let Some(building_id) = self.building_occupancy.get(&pos).copied() else {
            return Err(SimCommandError::MissingBuilding { pos });
        };
        let Some(building) = self.buildings.get(&building_id).cloned() else {
            return Err(SimCommandError::MissingBuilding { pos });
        };
        let SimBuildingState::Underground(runtime) = building.state.clone() else {
            return Err(SimCommandError::InvalidPort);
        };
        let def_id = building.def_id.clone();
        let speed = self
            .topology_graph
            .underground_link(building.origin)
            .map(|link| link.speed)
            .or_else(|| {
                self.catalog.building_by_id(&def_id).and_then(|def| {
                    let CoreBuildingDriver::Underground {
                        speed_units_per_tick,
                        ..
                    } = def.behavior.driver
                    else {
                        return None;
                    };
                    Some(speed_units_per_tick)
                })
            })
            .unwrap_or_else(|| UnitsPerTick::new(4));

        if runtime.partner != building_id {
            self.unpair_underground(building_id)?;
        }

        let new_direction = runtime.direction.opposite();
        self.topology_graph.set_belt(
            pos,
            BeltTile::turn(new_direction, new_direction).on_surface(self.surface_z_at(pos)),
        );
        if let Some(speed_units) = self.occupied_tiles.get(&pos).copied() {
            self.occupied_tiles.insert(pos, speed_units);
        }
        if let Some(building) = self.buildings.get_mut(&building_id) {
            building.direction = new_direction;
            building.state = SimBuildingState::Underground(UndergroundRuntime {
                role: UndergroundRole::Entrance,
                partner: building_id,
                direction: new_direction,
            });
        }

        let max_range = self
            .catalog
            .building_by_id(&def_id)
            .and_then(|def| {
                let CoreBuildingDriver::Underground {
                    max_range_tiles, ..
                } = def.behavior.driver
                else {
                    return None;
                };
                Some(max_range_tiles)
            })
            .unwrap_or(4);

        if let Some(partner_id) =
            self.find_solo_underground_partner(pos, new_direction, &def_id, max_range)
        {
            self.finalize_underground_pair(building_id, partner_id, new_direction, speed)?;
        } else {
            self.topology_graph.clear_underground_link(pos);
            self.refresh_connected_belt_inputs();
            self.rebuild_transport_lines();
        }

        Ok(())
    }

    fn find_solo_underground_partner(
        &self,
        pos: TilePos,
        direction: Direction,
        def_id: &str,
        max_range_tiles: u8,
    ) -> Option<BuildingId> {
        let delta = direction.delta();
        for distance in 1..=max_range_tiles {
            for sign in [-1_i32, 1] {
                let candidate = TilePos::new(
                    pos.x + delta.0 * i32::from(distance) * sign,
                    pos.y + delta.1 * i32::from(distance) * sign,
                );
                let Some(partner_id) = self.building_occupancy.get(&candidate).copied() else {
                    continue;
                };
                if !self.is_solo_underground(partner_id, def_id, direction) {
                    continue;
                }
                return Some(partner_id);
            }
        }
        None
    }

    fn is_solo_underground(
        &self,
        building_id: BuildingId,
        def_id: &str,
        direction: Direction,
    ) -> bool {
        let Some(building) = self.buildings.get(&building_id) else {
            return false;
        };
        if building.def_id != def_id {
            return false;
        }
        let SimBuildingState::Underground(runtime) = &building.state else {
            return false;
        };
        runtime.partner == building_id && runtime.direction == direction
    }

    fn finalize_underground_pair(
        &mut self,
        a_id: BuildingId,
        b_id: BuildingId,
        direction: Direction,
        speed: UnitsPerTick,
    ) -> Result<(), SimCommandError> {
        let origin_a = self
            .buildings
            .get(&a_id)
            .ok_or(SimCommandError::MissingBuildingId { building: a_id })?
            .origin;
        let origin_b = self
            .buildings
            .get(&b_id)
            .ok_or(SimCommandError::MissingBuildingId { building: b_id })?
            .origin;
        let max_range = self
            .buildings
            .get(&a_id)
            .and_then(|building| self.catalog.building_by_id(&building.def_id))
            .and_then(|def| {
                let CoreBuildingDriver::Underground {
                    max_range_tiles, ..
                } = def.behavior.driver
                else {
                    return None;
                };
                Some(max_range_tiles)
            })
            .unwrap_or(4);
        let (entrance, exit, corridor_record) = self
            .validate_underground_pair_positions(origin_a, origin_b, direction, speed, max_range)?;
        let (entrance_id, exit_id) = if origin_a == entrance {
            (a_id, b_id)
        } else {
            (b_id, a_id)
        };

        self.set_underground_runtime(entrance_id, UndergroundRole::Entrance, exit_id, direction);
        self.set_underground_runtime(exit_id, UndergroundRole::Exit, entrance_id, direction);
        self.register_underground_topology_links(entrance, exit, speed);
        self.reserve_underground_corridor(entrance, exit, corridor_record);
        self.rebuild_transport_lines();
        Ok(())
    }

    fn validate_underground_pair_positions(
        &self,
        a: TilePos,
        b: TilePos,
        direction: Direction,
        speed: UnitsPerTick,
        max_range: u8,
    ) -> Result<(TilePos, TilePos, UndergroundCorridorRecord), SimCommandError> {
        let (entrance, exit) = ordered_underground_endpoints(a, b, direction);
        let distance = (exit.x - entrance.x)
            .unsigned_abs()
            .max((exit.y - entrance.y).unsigned_abs()) as u8;
        if distance == 0 || distance > max_range {
            return Err(SimCommandError::InvalidPosition { pos: exit });
        }

        let corridor_record = UndergroundCorridorRecord { speed, direction };
        for tile in underground_corridor_tiles(entrance, exit) {
            if self
                .underground_corridors
                .get(&tile)
                .is_some_and(|records| records.contains(&corridor_record))
            {
                return Err(SimCommandError::TopologyConflict { pos: tile });
            }
        }

        Ok((entrance, exit, corridor_record))
    }

    fn set_underground_runtime(
        &mut self,
        id: BuildingId,
        role: UndergroundRole,
        partner: BuildingId,
        direction: Direction,
    ) {
        if let Some(building) = self.buildings.get_mut(&id) {
            building.state = SimBuildingState::Underground(UndergroundRuntime {
                role,
                partner,
                direction,
            });
        }
    }

    fn unpair_underground(&mut self, id: BuildingId) -> Result<(), SimCommandError> {
        let Some(building) = self.buildings.get(&id).cloned() else {
            return Ok(());
        };
        let SimBuildingState::Underground(runtime) = building.state.clone() else {
            return Ok(());
        };
        if runtime.partner == id {
            return Ok(());
        }
        let partner_id = runtime.partner;
        let speed = self
            .topology_graph
            .underground_link(building.origin)
            .map(|link| link.speed)
            .unwrap_or_else(|| UnitsPerTick::new(4));
        let corridor_record = UndergroundCorridorRecord {
            speed,
            direction: runtime.direction,
        };
        let partner_origin = self
            .buildings
            .get(&partner_id)
            .map(|partner| partner.origin);
        let (entrance, exit) = match runtime.role {
            UndergroundRole::Entrance => (Some(building.origin), partner_origin),
            UndergroundRole::Exit => (partner_origin, Some(building.origin)),
        };
        if let (Some(entrance), Some(exit)) = (entrance, exit) {
            self.collect_underground_runtime_drops(entrance, exit);
            self.clear_underground_corridor(entrance, exit, corridor_record);
            self.topology_graph.clear_underground_link(entrance);
            self.topology_graph.clear_underground_link(exit);
        }
        self.set_underground_runtime(id, UndergroundRole::Entrance, id, runtime.direction);
        if let Some(partner) = self.buildings.get(&partner_id).cloned() {
            let SimBuildingState::Underground(partner_runtime) = partner.state else {
                return Ok(());
            };
            self.set_underground_runtime(
                partner_id,
                UndergroundRole::Entrance,
                partner_id,
                partner_runtime.direction,
            );
        }
        Ok(())
    }

    fn collect_underground_runtime_drops(&mut self, entrance: TilePos, exit: TilePos) {
        let drops = self
            .transport
            .nodes_sorted()
            .filter(|node| node.kind == TransportNodeKind::Underground)
            .filter(|node| {
                node.input_ports()
                    .next()
                    .is_some_and(|port| port.tile == entrance)
                    && node
                        .output_ports()
                        .next()
                        .is_some_and(|port| port.tile == exit)
            })
            .filter_map(|node| match &node.runtime {
                TransportNodeRuntime::Underground(runtime) => Some(runtime.items.clone()),
                _ => None,
            })
            .flatten()
            .map(|item| carried_stack(item.item))
            .collect::<Vec<_>>();
        self.extend_removed_item_drops(entrance, drops);
    }

    fn remove_underground_endpoint(&mut self, id: BuildingId) -> Result<(), SimCommandError> {
        let Some(building) = self.buildings.remove(&id) else {
            return Ok(());
        };
        self.building_by_origin.remove(&building.origin);
        for pos in building.footprint {
            self.building_occupancy.remove(&pos);
        }
        self.occupied_tiles.remove(&building.origin);
        self.topology_graph.remove_belt(building.origin);
        self.topology_graph.clear_underground_link(building.origin);
        Ok(())
    }
}

fn ordered_underground_endpoints(
    a: TilePos,
    b: TilePos,
    direction: Direction,
) -> (TilePos, TilePos) {
    let delta = direction.delta();
    let along = (b.x - a.x) * delta.0 + (b.y - a.y) * delta.1;
    if along > 0 { (a, b) } else { (b, a) }
}
