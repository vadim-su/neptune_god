//! Inserter tick: pickup/drop candidates, cooldown, and inventory transfers.

use super::*;

impl SimWorld {
    fn inventory_can_accept_stack(
        inventory: &SimInventory,
        stack: CoreItemStack,
        item_rules: &InventoryItemRules,
    ) -> bool {
        inventory
            .clone()
            .insert_with_mode(stack, InsertMode::AtomicAllOrNothing, item_rules)
            .rejected
            .is_none()
    }

    pub(super) fn tick_inserters(
        &mut self,
        metrics: &mut SimMetricsSnapshot,
        diff: &mut SimDiff,
        behavior_host: &(impl BehaviorHost + ?Sized),
        behavior_catalog: &BehaviorCatalog,
    ) {
        for building_id in self.buildings.keys().copied().collect::<Vec<_>>() {
            let Some(building) = self.buildings.get(&building_id).cloned() else {
                continue;
            };
            let SimBuildingState::Inserter(mut inserter) = building.state.clone() else {
                continue;
            };

            metrics.active_inserters += 1;
            if inserter.cooldown_remaining_ticks > 0 {
                inserter.cooldown_remaining_ticks -= 1;
                self.replace_inserter_state(building_id, inserter);
                continue;
            }

            let cooldown_ticks = self.inserter_cooldown_ticks(building.def_id.as_str());
            if inserter.carried.is_none() {
                if let Some(stack) = self.try_inserter_pickup(&building, &inserter, diff) {
                    inserter.carried = Some(stack);
                    inserter.cooldown_remaining_ticks = cooldown_ticks;
                    metrics.inserter_pickups += 1;
                    metrics.inventory_transfers += 1;
                    diff.changed_chunks.push(building.origin.chunk_pos());
                }
            } else if let Some(stack) = inserter.carried
                && self.try_inserter_drop_with_behavior(
                    &building,
                    &inserter,
                    stack,
                    diff,
                    behavior_host,
                    behavior_catalog,
                )
            {
                inserter.carried = None;
                inserter.cooldown_remaining_ticks = cooldown_ticks;
                metrics.inserter_drops += 1;
                metrics.inventory_transfers += 1;
                diff.changed_chunks.push(building.origin.chunk_pos());
            }

            self.replace_inserter_state(building_id, inserter);
        }
    }

    pub(super) fn try_inserter_pickup(
        &mut self,
        building: &SimBuilding,
        inserter: &InserterRuntime,
        diff: &mut SimDiff,
    ) -> Option<CoreItemStack> {
        let tile = adjacent_tile(building.origin, inserter.pickup_direction);
        let mut candidates = self.inventory_pickup_candidates(tile, building.surface_z);
        candidates.extend(self.belt_pickup_candidates(tile, building.origin, building.surface_z));
        let candidate = choose_nearest_candidate(building.origin, candidates)?;
        match candidate.kind {
            InserterCandidateKind::Inventory { role, .. } => {
                let owner = candidate.owner;
                let mut inventories = self.take_building_inventories(owner);
                let inventory = inventories
                    .iter_mut()
                    .find(|inventory| inventory.role() == role)?;
                let stack = inventory.take_first_matching(|_| true);
                self.restore_building_inventories(owner, inventories);
                if stack.is_some() {
                    diff.changed_chunks.push(tile.chunk_pos());
                }
                stack
            }
            InserterCandidateKind::Belt { lane, .. } => {
                let line_id = LineId(candidate.owner.0);
                let line_tile = candidate.tile;
                let (_, min_distance, max_distance) = self.line_tile_window(line_id, line_tile)?;
                let line = self.transport.line_mut(line_id)?;
                let position = line.first_in_window(lane, min_distance, max_distance)?;
                let stack = carried_stack(position.item);
                let item = line.remove_one_at_distance(lane, position.distance)?;
                debug_assert_eq!(item, position.item);
                self.activation.wake_line(line_id);
                diff.changed_lines.push(line_id);
                diff.changed_chunks.push(line_tile.chunk_pos());
                Some(stack)
            }
            InserterCandidateKind::Splitter {
                node,
                phase,
                channel,
                lane,
            } => {
                let position = self
                    .splitter_item_positions_on_tile(candidate.tile)
                    .into_iter()
                    .filter(|position| {
                        position.node == node
                            && position.phase == phase
                            && position.channel == channel
                            && position.lane == lane
                    })
                    .max_by_key(|position| position.progress)?;
                let stack = carried_stack(position.item);
                let item = self.remove_splitter_item(node, phase, channel, lane)?;
                debug_assert_eq!(item, position.item);
                diff.changed_chunks.push(candidate.tile.chunk_pos());
                Some(stack)
            }
            InserterCandidateKind::Underground { node, lane } => {
                let position = self
                    .underground_item_positions_on_tile(candidate.tile)
                    .into_iter()
                    .filter(|position| position.node == node && position.lane == lane)
                    .max_by_key(|position| position.progress)?;
                let stack = carried_stack(position.item);
                let item = self.remove_underground_item(node, position.phase, lane)?;
                debug_assert_eq!(item, position.item);
                diff.changed_chunks.push(candidate.tile.chunk_pos());
                Some(stack)
            }
        }
    }

    pub(super) fn try_inserter_drop_with_behavior(
        &mut self,
        building: &SimBuilding,
        inserter: &InserterRuntime,
        stack: CoreItemStack,
        diff: &mut SimDiff,
        behavior_host: &(impl BehaviorHost + ?Sized),
        behavior_catalog: &BehaviorCatalog,
    ) -> bool {
        let tile = adjacent_tile(building.origin, inserter.drop_direction);
        let mut candidates = self.inventory_drop_candidates(
            tile,
            building.surface_z,
            stack,
            behavior_host,
            behavior_catalog,
        );
        candidates.extend(self.belt_drop_candidates(
            tile,
            stack.kind,
            building.origin,
            building.surface_z,
        ));
        let item_rules = InventoryItemRules::from_catalog(&self.catalog);
        let Some(candidate) = choose_nearest_candidate(building.origin, candidates) else {
            if self.inventory_drop_target_exists(
                tile,
                building.surface_z,
                stack,
                behavior_host,
                behavior_catalog,
            ) {
                return false;
            }
            return self.try_inserter_drop_to_surface(tile, building.surface_z, stack, diff);
        };
        match candidate.kind {
            InserterCandidateKind::Inventory { role, .. } => {
                let owner = candidate.owner;
                let mut inventories = self.take_building_inventories(owner);
                let Some(inventory) = inventories
                    .iter_mut()
                    .find(|inventory| inventory.role() == role)
                else {
                    self.restore_building_inventories(owner, inventories);
                    return false;
                };
                let accepted = inventory
                    .insert_with_mode(stack, InsertMode::AtomicAllOrNothing, &item_rules)
                    .rejected
                    .is_none();
                self.restore_building_inventories(owner, inventories);
                if accepted {
                    diff.changed_chunks.push(tile.chunk_pos());
                }
                accepted
            }
            InserterCandidateKind::Belt { lane, .. } => {
                let line_id = LineId(candidate.owner.0);
                let line_tile = candidate.tile;
                let (_, min_distance, max_distance) =
                    match self.line_tile_window(line_id, line_tile) {
                        Some(window) => window,
                        None => return false,
                    };
                let Some(line) = self.transport.line_mut(line_id) else {
                    return false;
                };
                if !insert_belt_item_from_side(
                    line,
                    lane,
                    stack.kind,
                    min_distance,
                    max_distance,
                    self.topology_graph
                        .belt(line_tile)
                        .map(|belt| belt.direction),
                    direction_between_adjacent(line_tile, building.origin),
                ) {
                    return false;
                }
                self.activation.wake_line(line_id);
                diff.changed_lines.push(line_id);
                diff.changed_chunks.push(line_tile.chunk_pos());
                true
            }
            InserterCandidateKind::Splitter { .. } => false,
            InserterCandidateKind::Underground { .. } => false,
        }
    }

    #[cfg(test)]
    pub(super) fn try_inserter_drop(
        &mut self,
        building: &SimBuilding,
        inserter: &InserterRuntime,
        stack: CoreItemStack,
        diff: &mut SimDiff,
    ) -> bool {
        self.try_inserter_drop_with_behavior(
            building,
            inserter,
            stack,
            diff,
            &NOOP_BEHAVIOR_HOST,
            &BehaviorCatalog::default(),
        )
    }

    pub(super) fn try_inserter_drop_to_surface(
        &mut self,
        tile: TilePos,
        surface_z: SurfaceZ,
        stack: CoreItemStack,
        diff: &mut SimDiff,
    ) -> bool {
        if self.surface_z_at(tile) != surface_z {
            return false;
        }
        if self.building_occupancy.contains_key(&tile)
            || self.occupied_tiles.contains_key(&tile)
            || self.occupied_surface_tiles.contains(&tile)
        {
            return false;
        }
        self.push_surface_item_drop(tile, stack);
        diff.changed_chunks.push(tile.chunk_pos());
        true
    }

    pub(super) fn inventory_pickup_candidates(
        &self,
        tile: TilePos,
        surface_z: SurfaceZ,
    ) -> Vec<InserterCandidate> {
        let mut candidates = self
            .buildings
            .values()
            .flat_map(|building| {
                building
                    .ports
                    .iter()
                    .filter(move |port| port.tile == tile)
                    .filter(move |port| port.surface_z == surface_z)
                    .filter_map(move |port| port_inventory_role(port.role).map(|role| (port, role)))
                    .filter(|(_, role)| *role != CoreInventoryRole::Storage)
                    .flat_map(move |(port, role)| {
                        building.inventories.iter().enumerate().filter_map(
                            move |(slot, inventory_id)| {
                                let record = self.inventories.get(inventory_id)?;
                                if record.inventory.role() != role {
                                    return None;
                                }
                                let role_order = pickup_role_order(role)?;
                                let has_item = record
                                    .inventory
                                    .snapshot()
                                    .slots
                                    .iter()
                                    .any(Option::is_some);
                                has_item.then_some(InserterCandidate {
                                    tile: port.tile,
                                    owner: building.id,
                                    role_order,
                                    lane_or_slot: slot,
                                    kind: InserterCandidateKind::Inventory { role, slot },
                                })
                            },
                        )
                    })
            })
            .collect::<Vec<_>>();
        candidates.extend(self.footprint_inventory_pickup_candidates(tile, surface_z));
        candidates
    }

    pub(super) fn inventory_drop_candidates(
        &self,
        tile: TilePos,
        surface_z: SurfaceZ,
        stack: CoreItemStack,
        behavior_host: &(impl BehaviorHost + ?Sized),
        behavior_catalog: &BehaviorCatalog,
    ) -> Vec<InserterCandidate> {
        let item_rules = InventoryItemRules::from_catalog(&self.catalog);
        let mut candidates = Vec::new();
        for building in self.buildings.values() {
            for port in &building.ports {
                if port.tile != tile {
                    continue;
                }
                if port.surface_z != surface_z {
                    continue;
                }
                if !port.accepts.is_empty() && !port.accepts.contains(&stack.kind) {
                    continue;
                }
                if !self.behavior_input_port_accepts(
                    building,
                    port,
                    stack.kind,
                    behavior_host,
                    behavior_catalog,
                ) {
                    continue;
                }
                let Some(role) = port_inventory_role(port.role) else {
                    continue;
                };
                if role == CoreInventoryRole::Storage {
                    continue;
                }
                let Some(role_order) = drop_role_order(role) else {
                    continue;
                };
                for (slot, inventory_id) in building.inventories.iter().copied().enumerate() {
                    let Some(record) = self.inventories.get(&inventory_id) else {
                        continue;
                    };
                    if record.inventory.role() != role {
                        continue;
                    }
                    if !self.inserter_deposit_cap_allows(building, role, stack, &record.inventory) {
                        continue;
                    }
                    if !Self::inventory_can_accept_stack(&record.inventory, stack, &item_rules) {
                        continue;
                    }
                    candidates.push(InserterCandidate {
                        tile: port.tile,
                        owner: building.id,
                        role_order,
                        lane_or_slot: slot,
                        kind: InserterCandidateKind::Inventory { role, slot },
                    });
                }
            }
        }
        candidates.extend(self.footprint_inventory_drop_candidates(tile, surface_z, stack));
        candidates
    }

    fn behavior_input_port_accepts(
        &self,
        building: &SimBuilding,
        port: &SimBuildingPort,
        kind: ItemKindId,
        behavior_host: &(impl BehaviorHost + ?Sized),
        behavior_catalog: &BehaviorCatalog,
    ) -> bool {
        if port.role != CorePortRole::Input {
            return true;
        }
        let SimBuildingState::Behavior(state) = &building.state else {
            return true;
        };
        let Some(def) = self.catalog.building_by_id(&building.def_id) else {
            return true;
        };
        if !def.behavior.requires_behavior_host() {
            return true;
        }
        behavior_host
            .behavior_accepts_input(
                behavior_catalog,
                &def.behavior.config,
                state,
                behavior_kind(kind),
            )
            .unwrap_or(false)
    }

    fn inventory_drop_target_exists(
        &self,
        tile: TilePos,
        surface_z: SurfaceZ,
        stack: CoreItemStack,
        behavior_host: &(impl BehaviorHost + ?Sized),
        behavior_catalog: &BehaviorCatalog,
    ) -> bool {
        let port_target_exists = self.buildings.values().any(|building| {
            building
                .ports
                .iter()
                .filter(|port| port.tile == tile)
                .filter(|port| port.surface_z == surface_z)
                .filter(|port| port.accepts.is_empty() || port.accepts.contains(&stack.kind))
                .filter(|port| {
                    self.behavior_input_port_accepts(
                        building,
                        port,
                        stack.kind,
                        behavior_host,
                        behavior_catalog,
                    )
                })
                .filter_map(|port| port_inventory_role(port.role))
                .filter(|role| *role != CoreInventoryRole::Storage)
                .any(|role| {
                    drop_role_order(role).is_some()
                        && building.inventories.iter().any(|inventory_id| {
                            self.inventories
                                .get(inventory_id)
                                .is_some_and(|record| record.inventory.role() == role)
                        })
                })
        });
        if port_target_exists {
            return true;
        }

        let Some(building_id) = self.building_occupancy.get(&tile).copied() else {
            return false;
        };
        let Some(building) = self.buildings.get(&building_id) else {
            return false;
        };
        if building.surface_z != surface_z {
            return false;
        }
        building.inventories.iter().any(|inventory_id| {
            let Some(record) = self.inventories.get(inventory_id) else {
                return false;
            };
            let role = record.inventory.role();
            matches!(role, CoreInventoryRole::Fuel | CoreInventoryRole::Storage)
                && drop_role_order(role).is_some()
                && record.inventory.accepts(stack.kind)
        })
    }

    pub(super) fn footprint_inventory_pickup_candidates(
        &self,
        tile: TilePos,
        surface_z: SurfaceZ,
    ) -> Vec<InserterCandidate> {
        let Some(building_id) = self.building_occupancy.get(&tile).copied() else {
            return Vec::new();
        };
        let Some(building) = self.buildings.get(&building_id) else {
            return Vec::new();
        };
        if building.surface_z != surface_z {
            return Vec::new();
        }
        building
            .inventories
            .iter()
            .enumerate()
            .filter_map(|(slot, inventory_id)| {
                let record = self.inventories.get(inventory_id)?;
                let role = record.inventory.role();
                let role_order = pickup_role_order(role)?;
                if role != CoreInventoryRole::Storage {
                    return None;
                }
                let has_item = record
                    .inventory
                    .snapshot()
                    .slots
                    .iter()
                    .any(Option::is_some);
                has_item.then_some(InserterCandidate {
                    tile,
                    owner: building.id,
                    role_order,
                    lane_or_slot: slot,
                    kind: InserterCandidateKind::Inventory { role, slot },
                })
            })
            .collect()
    }

    pub(super) fn footprint_inventory_drop_candidates(
        &self,
        tile: TilePos,
        surface_z: SurfaceZ,
        stack: CoreItemStack,
    ) -> Vec<InserterCandidate> {
        let Some(building_id) = self.building_occupancy.get(&tile).copied() else {
            return Vec::new();
        };
        let Some(building) = self.buildings.get(&building_id) else {
            return Vec::new();
        };
        if building.surface_z != surface_z {
            return Vec::new();
        }
        let item_rules = InventoryItemRules::from_catalog(&self.catalog);
        building
            .inventories
            .iter()
            .enumerate()
            .filter_map(|(slot, inventory_id)| {
                let record = self.inventories.get(inventory_id)?;
                let role = record.inventory.role();
                let role_order = drop_role_order(role)?;
                if !matches!(role, CoreInventoryRole::Fuel | CoreInventoryRole::Storage) {
                    return None;
                }
                if !record.inventory.accepts(stack.kind)
                    || !Self::inventory_can_accept_stack(&record.inventory, stack, &item_rules)
                    || !self.inserter_deposit_cap_allows(building, role, stack, &record.inventory)
                {
                    return None;
                }
                Some(InserterCandidate {
                    tile,
                    owner: building.id,
                    role_order,
                    lane_or_slot: slot,
                    kind: InserterCandidateKind::Inventory { role, slot },
                })
            })
            .collect()
    }

    fn inserter_deposit_cap_allows(
        &self,
        building: &SimBuilding,
        role: CoreInventoryRole,
        stack: CoreItemStack,
        inventory: &SimInventory,
    ) -> bool {
        let Some(def) = self.catalog.building_by_id(&building.def_id) else {
            return true;
        };
        let Some(limit) = def
            .inserter_deposit_limits
            .iter()
            .find(|limit| limit.role == role && limit.item == stack.kind)
        else {
            return true;
        };

        inventory.count(stack.kind).saturating_add(stack.amount) <= limit.max_amount
    }

    pub(super) fn belt_pickup_candidates(
        &self,
        tile: TilePos,
        inserter_origin: TilePos,
        surface_z: SurfaceZ,
    ) -> Vec<InserterCandidate> {
        let mut candidates = Vec::new();
        for line_tile in self.belt_line_tiles_for_port(tile) {
            let Some(belt) = self.topology_graph.belt(line_tile) else {
                continue;
            };
            if belt.surface_z != surface_z {
                continue;
            }
            let Some((line_id, slot, min_distance, max_distance)) =
                self.line_window_for_tile(line_tile)
            else {
                continue;
            };
            let Some(line) = self.transport.line(line_id) else {
                continue;
            };
            let source_side = direction_between_adjacent(line_tile, inserter_origin);
            let preferred_lane =
                source_side.and_then(|side| belt.direction.near_lane_for_source_side(side));
            for lane in 0..2 {
                if line
                    .lane_positions_in_range(lane, min_distance, max_distance)
                    .is_empty()
                {
                    continue;
                }
                candidates.push(InserterCandidate {
                    tile: line_tile,
                    owner: BuildingId(line_id.0),
                    role_order: CandidateRoleOrder::Belt,
                    lane_or_slot: lane_rank(lane, preferred_lane),
                    kind: InserterCandidateKind::Belt { lane, slot },
                });
            }
        }
        for position in self.splitter_item_positions_on_tile(tile) {
            let preferred_lane = direction_between_adjacent(position.tile, inserter_origin)
                .and_then(|side| {
                    self.transport
                        .nodes_sorted()
                        .find(|node| node.id == position.node)
                        .and_then(|node| node.direction)
                        .and_then(|direction| direction.near_lane_for_source_side(side))
                });
            candidates.push(InserterCandidate {
                tile: position.tile,
                owner: BuildingId(position.node.0.min(u64::from(u32::MAX)) as u32),
                role_order: CandidateRoleOrder::SplitterBelt,
                lane_or_slot: lane_rank(position.lane, preferred_lane),
                kind: InserterCandidateKind::Splitter {
                    node: position.node,
                    phase: position.phase,
                    channel: position.channel,
                    lane: position.lane,
                },
            });
        }
        for position in self.underground_item_positions_on_tile(tile) {
            let preferred_lane = direction_between_adjacent(position.tile, inserter_origin)
                .and_then(|side| position.direction.near_lane_for_source_side(side));
            candidates.push(InserterCandidate {
                tile: position.tile,
                owner: BuildingId(position.node.0.min(u64::from(u32::MAX)) as u32),
                role_order: CandidateRoleOrder::UndergroundBelt,
                lane_or_slot: lane_rank(position.lane, preferred_lane),
                kind: InserterCandidateKind::Underground {
                    node: position.node,
                    lane: position.lane,
                },
            });
        }
        candidates
    }

    pub(super) fn belt_drop_candidates(
        &self,
        tile: TilePos,
        item: ItemKindId,
        inserter_origin: TilePos,
        surface_z: SurfaceZ,
    ) -> Vec<InserterCandidate> {
        let mut candidates = Vec::new();
        for line_tile in self.belt_line_tiles_for_port(tile) {
            let Some(belt) = self.topology_graph.belt(line_tile) else {
                continue;
            };
            if belt.surface_z != surface_z {
                continue;
            }
            let Some((line_id, slot, min_distance, max_distance)) =
                self.line_window_for_tile(line_tile)
            else {
                continue;
            };
            let Some(line) = self.transport.line(line_id) else {
                continue;
            };
            let source_side = direction_between_adjacent(line_tile, inserter_origin);
            let belt_direction = Some(belt.direction);
            let preferred_lane =
                source_side.and_then(|side| belt.direction.near_lane_for_source_side(side));
            for lane in 0..2 {
                let mut line = line.clone();
                if !insert_belt_item_from_side(
                    &mut line,
                    lane,
                    item,
                    min_distance,
                    max_distance,
                    belt_direction,
                    source_side,
                ) {
                    continue;
                }
                candidates.push(InserterCandidate {
                    tile: line_tile,
                    owner: BuildingId(line_id.0),
                    role_order: CandidateRoleOrder::Belt,
                    lane_or_slot: lane_rank(lane, preferred_lane),
                    kind: InserterCandidateKind::Belt { lane, slot },
                });
            }
        }
        candidates
    }

    pub(super) fn belt_line_tiles_for_port(&self, port_tile: TilePos) -> Vec<TilePos> {
        self.buildings
            .values()
            .filter(|building| building.kind == CoreBuildingKind::Transport)
            .filter(|building| building.origin == port_tile)
            .map(|building| building.origin)
            .collect()
    }

    pub(super) fn line_window_for_tile(
        &self,
        tile: TilePos,
    ) -> Option<(LineId, usize, DistanceUnits, DistanceUnits)> {
        let mut fallback = None;
        for line_id in self.transport.line_ids_sorted() {
            let Some(window) = self.line_tile_window(line_id, tile) else {
                continue;
            };
            if self.occupied_tiles.contains_key(&tile) {
                return Some((line_id, window.0, window.1, window.2));
            }
            fallback = Some((line_id, window.0, window.1, window.2));
        }
        fallback
    }

    pub(super) fn line_tile_window(
        &self,
        line_id: LineId,
        tile: TilePos,
    ) -> Option<(usize, DistanceUnits, DistanceUnits)> {
        let line = self.transport.line(line_id)?;
        let index = line
            .path()
            .tiles()
            .iter()
            .position(|line_tile| line_tile.pos == tile && line_tile.is_surface())?;
        let total_len = line.path().total_len().raw();
        let min_distance = total_len - ((index as i32 + 1) * DistanceUnits::UNITS_PER_TILE);
        let max_distance = total_len - (index as i32 * DistanceUnits::UNITS_PER_TILE) - 1;
        Some((
            index,
            DistanceUnits::new(min_distance),
            DistanceUnits::new(max_distance),
        ))
    }

    pub(super) fn inserter_cooldown_ticks(&self, def_id: &str) -> u32 {
        self.catalog
            .building_by_id(def_id)
            .and_then(|def| match &def.behavior.driver {
                CoreBuildingDriver::Inserter { cooldown_ticks } => Some(*cooldown_ticks),
                _ => None,
            })
            .unwrap_or(0)
    }

    pub(super) fn replace_inserter_state(
        &mut self,
        building: BuildingId,
        inserter: InserterRuntime,
    ) {
        let Some(building) = self.buildings.get_mut(&building) else {
            return;
        };
        building.state = SimBuildingState::Inserter(inserter);
    }
}
