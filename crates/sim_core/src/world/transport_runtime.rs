//! Transport tick: line advance, interactions, metrics, and line segmentation rules.

use super::*;
use crate::transport::line::FRONT_WINDOW_LEN;
use crate::transport::stream::MIN_ITEM_SPACING;

const SPLITTER_BUFFER_CAPACITY: usize = 5;
const SPLITTER_STAGE_END: DistanceUnits = DistanceUnits::new(DistanceUnits::UNITS_PER_TILE / 2);

struct SpeedSegment {
    tiles: Vec<crate::topology::builder::BuiltPathTile>,
    closed: bool,
    front_output: Option<TilePos>,
}

struct SplitterTopologyConnection {
    input_tiles: [TilePos; 2],
    output_tiles: [TilePos; 2],
    input_lines: [Option<LineId>; 2],
    output_lines: [Option<LineId>; 2],
}

fn missing_splitter_input_line_id(building_id: BuildingId, channel: usize) -> LineId {
    LineId(u32::MAX / 2 - building_id.0.saturating_mul(2) - channel as u32)
}

fn missing_splitter_output_line_id(building_id: BuildingId, channel: usize) -> LineId {
    LineId(u32::MAX - building_id.0.saturating_mul(2) - channel as u32)
}

impl SimWorld {
    pub(super) fn refresh_transport_metrics(&self, metrics: &mut SimMetricsSnapshot) {
        metrics.simulated_items = 0;
        metrics.active_lines = 0;
        metrics.sleeping_lines = 0;
        for line_id in self.transport.line_ids_sorted() {
            let Some(line) = self.transport.line(line_id) else {
                continue;
            };
            metrics.simulated_items += line.lane(0).item_count() + line.lane(1).item_count();
            if line.sleeping() {
                metrics.sleeping_lines += 1;
            } else {
                metrics.active_lines += 1;
            }
        }
    }

    pub(super) fn refresh_connected_belt_inputs(&mut self) -> bool {
        let updates = self
            .topology_graph
            .belts_sorted()
            .filter_map(|(pos, belt)| {
                let input_direction = self.connected_belt_input_direction(pos, belt.direction);
                (input_direction != belt.input_direction).then_some((
                    pos,
                    BeltTile::turn(input_direction, belt.direction).on_surface(belt.surface_z),
                ))
            })
            .collect::<Vec<_>>();
        let changed = !updates.is_empty();
        for (pos, belt) in updates {
            self.topology_graph.set_belt(pos, belt);
        }
        changed
    }

    pub(super) fn connected_belt_input_direction(
        &self,
        pos: TilePos,
        output: Direction,
    ) -> Direction {
        let target_z = self
            .topology_graph
            .belt(pos)
            .map(|belt| belt.surface_z)
            .unwrap_or_else(|| self.surface_z_at(pos));
        let mut side_input = None;
        for input_direction in [output, output.left(), output.right()] {
            let input_pos = input_direction.opposite().output_pos(pos);
            if self.transport_source_outputs_to(input_pos, pos, input_direction, target_z) {
                if input_direction == output {
                    return output;
                }
                if side_input.is_some() {
                    return output;
                }
                side_input = Some(input_direction);
            }
        }

        side_input.unwrap_or(output)
    }

    fn transport_source_outputs_to(
        &self,
        source_pos: TilePos,
        target_pos: TilePos,
        direction: Direction,
        target_z: SurfaceZ,
    ) -> bool {
        if self
            .topology_graph
            .belt(source_pos)
            .is_some_and(|belt| belt.direction == direction && belt.surface_z == target_z)
        {
            return true;
        }

        let Some(building_id) = self.building_occupancy.get(&source_pos) else {
            return false;
        };
        let Some(building) = self.buildings.get(building_id) else {
            return false;
        };
        if building.direction != direction || direction.output_pos(source_pos) != target_pos {
            return false;
        }
        if building.surface_z != target_z {
            return false;
        }
        self.catalog
            .building_by_id(&building.def_id)
            .is_some_and(|def| matches!(def.behavior.driver, CoreBuildingDriver::Splitter { .. }))
    }

    pub(super) fn rebuild_transport_lines(&mut self) {
        let splitter_runtimes = self.splitter_runtimes_by_identity();
        let underground_runtimes = self.underground_runtimes_by_identity();
        let old_lines = self
            .transport
            .line_ids_sorted()
            .filter_map(|line_id| self.transport.line(line_id))
            .map(|line| OldTransportLine {
                tiles: line
                    .path()
                    .tiles()
                    .iter()
                    .map(|tile| tile.pos)
                    .collect::<Vec<_>>(),
                speed: line.speed(),
                lanes: [line.lane(0).clone(), line.lane(1).clone()],
            })
            .collect::<Vec<_>>();
        let topology = TopologyBuilder.rebuild(&self.topology_graph);
        let all_new_tiles = topology
            .lines
            .iter()
            .flat_map(|line| line.tiles.iter().map(|tile| tile.pos))
            .collect::<BTreeSet<_>>();
        let mut transport = TransportStorage::default();
        let mut built_records = Vec::new();

        for built_line in topology.lines {
            let segments = self.split_built_line_by_speed(
                &built_line.tiles,
                built_line.closed,
                built_line.front_output,
            );
            for segment in segments {
                let line_id = self.ids.next_line();
                let closed = segment.closed;
                let front_output = segment.front_output;
                let tiles = segment.tiles;
                let speed = tiles
                    .iter()
                    .find(|tile| tile.kind == crate::topology::builder::BuiltPathTileKind::Surface)
                    .and_then(|tile| self.occupied_tiles.get(&tile.pos))
                    .copied()
                    .unwrap_or_else(|| UnitsPerTick::new(8));
                let tile_positions = tiles.iter().map(|tile| tile.pos).collect::<Vec<_>>();
                let lanes = remap_lanes_for_new_line(
                    &old_lines,
                    &tile_positions,
                    speed,
                    &mut self.removed_item_drops,
                );
                let path = LinePath::new(
                    tiles
                        .iter()
                        .map(|tile| match tile.kind {
                            crate::topology::builder::BuiltPathTileKind::Surface => {
                                LineTile::surface(tile.pos)
                            }
                            crate::topology::builder::BuiltPathTileKind::Underground => {
                                LineTile::underground(tile.pos)
                            }
                        })
                        .collect::<Vec<_>>(),
                );

                transport.insert_line(TransportLine::new(
                    line_id,
                    GroupId(line_id.0),
                    path,
                    speed,
                    lanes,
                    if closed {
                        LineEndpoint::Closed
                    } else {
                        LineEndpoint::Blocked
                    },
                    if closed {
                        LineEndpoint::Closed
                    } else {
                        LineEndpoint::Open
                    },
                ));
                built_records.push(BuiltLineRecord {
                    line_id,
                    tiles,
                    closed,
                    front_output,
                });
            }
        }

        drop_old_items_without_surviving_tile(
            &old_lines,
            &all_new_tiles,
            &mut self.removed_item_drops,
        );
        let splitter_owned_input_lines = self.splitter_owned_input_lines(&built_records);
        let underground_owned_input_lines = self.underground_owned_input_lines(&built_records);
        let node_owned_input_lines = splitter_owned_input_lines
            .union(&underground_owned_input_lines)
            .copied()
            .collect();
        self.insert_belt_interactions(&mut transport, &built_records, &node_owned_input_lines);
        self.insert_underground_nodes(&mut transport, &built_records, &underground_runtimes);
        self.insert_splitter_nodes(&mut transport, &built_records, &splitter_runtimes);

        self.topology_revision_seen = topology.source_revision;
        self.activation
            .replace_active_lines(transport.line_ids_sorted());
        self.transport = transport;
    }

    fn split_built_line_by_speed(
        &self,
        tiles: &[crate::topology::builder::BuiltPathTile],
        closed: bool,
        front_output: Option<TilePos>,
    ) -> Vec<SpeedSegment> {
        if tiles.is_empty() {
            return Vec::new();
        }

        if tiles
            .iter()
            .any(|tile| tile.kind == crate::topology::builder::BuiltPathTileKind::Underground)
        {
            return vec![SpeedSegment {
                tiles: tiles.to_vec(),
                closed,
                front_output,
            }];
        }

        let mut segments = Vec::new();
        let mut start = 0;
        for index in 1..tiles.len() {
            if self.should_split_line_segment(tiles[index - 1].pos, tiles[index].pos) {
                segments.push((start, index));
                start = index;
            }
        }
        segments.push((start, tiles.len()));

        if segments.len() == 1 {
            return vec![SpeedSegment {
                tiles: tiles.to_vec(),
                closed,
                front_output,
            }];
        }

        segments
            .iter()
            .enumerate()
            .map(|(segment_index, &(start, end))| {
                let segment_tiles = tiles[start..end].to_vec();
                let next_tile = if segment_index + 1 < segments.len() {
                    let (next_start, _) = segments[segment_index + 1];
                    Some(tiles[next_start].pos)
                } else if closed {
                    Some(tiles[segments[0].0].pos)
                } else {
                    front_output
                };

                SpeedSegment {
                    tiles: segment_tiles,
                    closed: false,
                    front_output: next_tile,
                }
            })
            .collect()
    }

    fn splitter_runtimes_by_identity(
        &self,
    ) -> BTreeMap<(TilePos, Direction), crate::transport::node::SplitterRuntime> {
        self.transport
            .nodes_sorted()
            .filter_map(|node| {
                let direction = node.direction?;
                match &node.runtime {
                    crate::transport::node::TransportNodeRuntime::Splitter(runtime) => {
                        Some(((node.sort_tile, direction), runtime.clone()))
                    }
                    crate::transport::node::TransportNodeRuntime::None
                    | crate::transport::node::TransportNodeRuntime::Underground(_) => None,
                }
            })
            .collect()
    }

    fn underground_runtimes_by_identity(
        &self,
    ) -> BTreeMap<
        (TilePos, TilePos, Direction, UnitsPerTick),
        crate::transport::node::UndergroundTransportRuntime,
    > {
        use crate::transport::node::{TransportNodeKind, TransportNodeRuntime};

        self.transport
            .nodes_sorted()
            .filter_map(|node| {
                if node.kind != TransportNodeKind::Underground {
                    return None;
                }
                let direction = node.direction?;
                let entrance = node.input_ports().next()?.tile;
                let exit = node.output_ports().next()?.tile;
                let runtime = match &node.runtime {
                    TransportNodeRuntime::Underground(runtime) => runtime.clone(),
                    _ => return None,
                };
                let speed = self
                    .topology_graph
                    .underground_link(entrance)
                    .map(|link| link.speed)?;
                Some(((entrance, exit, direction, speed), runtime))
            })
            .collect()
    }

    fn insert_underground_nodes(
        &self,
        transport: &mut TransportStorage,
        records: &[BuiltLineRecord],
        old_underground_runtimes: &BTreeMap<
            (TilePos, TilePos, Direction, UnitsPerTick),
            crate::transport::node::UndergroundTransportRuntime,
        >,
    ) {
        use crate::transport::node::{TransportNode, TransportNodeId, TransportNodeRuntime};

        let mut next_node_id = transport
            .nodes_sorted()
            .map(|node| node.id.0)
            .max()
            .unwrap_or(0);
        let mut entrances = self
            .buildings
            .values()
            .filter_map(|building| {
                let SimBuildingState::Underground(runtime) = &building.state else {
                    return None;
                };
                if runtime.role != UndergroundRole::Entrance || runtime.partner == building.id {
                    return None;
                }
                Some((building, runtime))
            })
            .collect::<Vec<_>>();
        entrances.sort_by_key(|(building, _)| (building.origin, building.id));

        for (building, runtime) in entrances {
            let Some(exit_building) = self.buildings.get(&runtime.partner) else {
                continue;
            };
            let SimBuildingState::Underground(exit_runtime) = &exit_building.state else {
                continue;
            };
            if exit_runtime.role != UndergroundRole::Exit {
                continue;
            }

            let entrance = building.origin;
            let exit = exit_building.origin;
            let direction = runtime.direction;
            let Some(speed) = self
                .topology_graph
                .underground_link(entrance)
                .map(|link| link.speed)
            else {
                continue;
            };
            let Some(input_line) =
                line_ending_from_tile_to_tile(records, entrance, direction.output_pos(entrance))
            else {
                continue;
            };
            let output_tile = direction.output_pos(exit);
            let Some(output_line) = self
                .underground_output_line(records, exit, output_tile)
                .or_else(|| line_by_first_tile(records, exit).map(|record| record.line_id))
            else {
                continue;
            };
            let distance = DistanceUnits::new(
                ((exit.x - entrance.x)
                    .unsigned_abs()
                    .max((exit.y - entrance.y).unsigned_abs()) as i32)
                    * DistanceUnits::UNITS_PER_TILE,
            );

            next_node_id += 1;
            let mut node = TransportNode::underground(
                TransportNodeId(next_node_id),
                entrance,
                exit,
                direction,
                input_line,
                output_line,
                distance,
            );
            if let Some(old_runtime) =
                old_underground_runtimes.get(&(entrance, exit, direction, speed))
            {
                node.runtime = TransportNodeRuntime::Underground(old_runtime.clone());
            }
            transport.insert_node(node);
        }
    }

    fn insert_splitter_nodes(
        &self,
        transport: &mut TransportStorage,
        records: &[BuiltLineRecord],
        old_splitter_runtimes: &BTreeMap<
            (TilePos, Direction),
            crate::transport::node::SplitterRuntime,
        >,
    ) {
        use crate::transport::node::{TransportNode, TransportNodeId, TransportNodeRuntime};

        let mut next_node_id = transport
            .nodes_sorted()
            .map(|node| node.id.0)
            .max()
            .unwrap_or(0);
        let mut splitters = self
            .buildings
            .values()
            .filter(|building| {
                self.catalog
                    .building_by_id(&building.def_id)
                    .is_some_and(|def| {
                        matches!(def.behavior.driver, CoreBuildingDriver::Splitter { .. })
                    })
            })
            .collect::<Vec<_>>();
        splitters.sort_by_key(|building| (building.origin, building.id));

        for building in splitters {
            let Some(connection) = self.splitter_topology_connection(records, building) else {
                continue;
            };

            next_node_id += 1;
            let mut node = TransportNode::splitter_2x1_with_channel_tiles(
                TransportNodeId(next_node_id),
                building.origin,
                building.direction,
                connection.input_tiles,
                connection.output_tiles,
                [
                    connection.input_lines[0]
                        .unwrap_or_else(|| missing_splitter_input_line_id(building.id, 0)),
                    connection.input_lines[1]
                        .unwrap_or_else(|| missing_splitter_input_line_id(building.id, 1)),
                ],
                [
                    connection.output_lines[0]
                        .unwrap_or_else(|| missing_splitter_output_line_id(building.id, 0)),
                    connection.output_lines[1]
                        .unwrap_or_else(|| missing_splitter_output_line_id(building.id, 1)),
                ],
            );
            let runtime = match building.state {
                SimBuildingState::Splitter(ref runtime) => runtime.clone(),
                _ => old_splitter_runtimes
                    .get(&(building.origin, building.direction))
                    .cloned()
                    .unwrap_or_default(),
            };
            node.runtime = TransportNodeRuntime::Splitter(runtime);
            transport.insert_node(node);
        }
    }

    fn splitter_owned_input_lines(&self, records: &[BuiltLineRecord]) -> BTreeSet<LineId> {
        self.buildings
            .values()
            .filter_map(|building| self.splitter_topology_connection(records, building))
            .flat_map(|connection| connection.input_lines.into_iter().flatten())
            .collect()
    }

    fn underground_owned_input_lines(&self, records: &[BuiltLineRecord]) -> BTreeSet<LineId> {
        self.buildings
            .values()
            .filter_map(|building| {
                let SimBuildingState::Underground(runtime) = &building.state else {
                    return None;
                };
                if runtime.role != UndergroundRole::Entrance || runtime.partner == building.id {
                    return None;
                }
                line_ending_from_tile_to_tile(
                    records,
                    building.origin,
                    runtime.direction.output_pos(building.origin),
                )
            })
            .collect()
    }

    fn splitter_topology_connection(
        &self,
        records: &[BuiltLineRecord],
        building: &SimBuilding,
    ) -> Option<SplitterTopologyConnection> {
        let def = self.catalog.building_by_id(&building.def_id)?;
        if !matches!(def.behavior.driver, CoreBuildingDriver::Splitter { .. }) {
            return None;
        }

        let [first, second] = splitter_channel_geometry(&building.footprint, building.direction)?;
        let input_first =
            line_ending_from_tile_to_tile(records, first.input_pos, first.channel_tile);
        let input_second =
            line_ending_from_tile_to_tile(records, second.input_pos, second.channel_tile);
        let output_first =
            self.splitter_output_line(records, first.channel_tile, first.output_pos)?;
        let output_second =
            self.splitter_output_line(records, second.channel_tile, second.output_pos)?;
        if input_first.is_none()
            && input_second.is_none()
            && output_first.is_none()
            && output_second.is_none()
        {
            return None;
        }

        Some(SplitterTopologyConnection {
            input_tiles: [first.channel_tile, second.channel_tile],
            output_tiles: [first.output_pos, second.output_pos],
            input_lines: [input_first, input_second],
            output_lines: [output_first, output_second],
        })
    }

    fn splitter_output_line(
        &self,
        records: &[BuiltLineRecord],
        channel_tile: TilePos,
        output_pos: TilePos,
    ) -> Option<Option<LineId>> {
        let Some(output_belt) = self.topology_graph.belt(output_pos) else {
            return Some(None);
        };
        let source_side = source_side_for_target(channel_tile, output_pos)?;
        let accepts_splitter_side = source_side == output_belt.input_direction.opposite()
            || output_belt
                .direction
                .near_lane_for_source_side(source_side)
                .is_some();
        if !accepts_splitter_side {
            return None;
        }
        Some(line_by_first_tile(records, output_pos).map(|record| record.line_id))
    }

    fn underground_output_line(
        &self,
        records: &[BuiltLineRecord],
        exit: TilePos,
        output_pos: TilePos,
    ) -> Option<LineId> {
        let output_belt = self.topology_graph.belt(output_pos)?;
        let source_side = source_side_for_target(exit, output_pos)?;
        let accepts_underground_output = source_side == output_belt.input_direction.opposite()
            || output_belt
                .direction
                .near_lane_for_source_side(source_side)
                .is_some();
        if !accepts_underground_output {
            return None;
        }
        line_by_first_tile(records, output_pos).map(|record| record.line_id)
    }

    fn tile_speed(&self, pos: TilePos) -> UnitsPerTick {
        self.occupied_tiles
            .get(&pos)
            .copied()
            .unwrap_or_else(|| UnitsPerTick::new(8))
    }

    /// Split when transport speed changes.
    fn should_split_line_segment(&self, from: TilePos, to: TilePos) -> bool {
        self.tile_speed(from) != self.tile_speed(to)
    }

    fn underground_allows_side_load(&self, target: TilePos, source_side: Direction) -> bool {
        let Some(building_id) = self.building_occupancy.get(&target) else {
            return true;
        };
        let Some(building) = self.buildings.get(building_id) else {
            return true;
        };
        let SimBuildingState::Underground(runtime) = &building.state else {
            return true;
        };
        match runtime.role {
            UndergroundRole::Entrance => source_side == runtime.direction.opposite(),
            UndergroundRole::Exit => source_side == runtime.direction,
        }
    }

    pub(super) fn insert_belt_interactions(
        &self,
        transport: &mut TransportStorage,
        records: &[BuiltLineRecord],
        node_owned_input_lines: &BTreeSet<LineId>,
    ) {
        use crate::transport::interaction::{BeltInteraction, BeltInteractionKind};
        use crate::transport::node::{TransportNode, TransportNodeId};

        let mut next_node_id = 0_u64;
        let mut make_node_id = || {
            next_node_id += 1;
            TransportNodeId(next_node_id)
        };
        for record in records {
            if node_owned_input_lines.contains(&record.line_id) {
                continue;
            }
            if record.closed {
                continue;
            }
            let Some(front_output) = record.front_output else {
                continue;
            };
            let source_sort_tile = record
                .tiles
                .iter()
                .rev()
                .find(|tile| tile.kind == crate::topology::builder::BuiltPathTileKind::Surface)
                .map(|tile| tile.pos)
                .unwrap_or(front_output);
            let source_side = source_side_for_target(source_sort_tile, front_output);

            if let Some(target) = line_by_first_tile(records, front_output)
                && !target.closed
                && let Some(target_belt) = self.topology_graph.belt(front_output)
                && self.belt_surfaces_match(source_sort_tile, front_output)
                && source_side == Some(target_belt.input_direction.opposite())
            {
                let kind = BeltInteractionKind::EndTransfer;
                transport.insert_interaction(BeltInteraction::new(
                    kind,
                    record.line_id,
                    Some(target.line_id),
                    Some(front_output),
                    front_output,
                ));
                transport.insert_node(TransportNode::end_transfer(
                    make_node_id(),
                    front_output,
                    record.line_id,
                    target.line_id,
                ));
                continue;
            }

            if let Some(target) = line_containing_tile(records, front_output) {
                let Some(target_belt) = self.topology_graph.belt(front_output) else {
                    continue;
                };
                if !self.belt_surfaces_match(source_sort_tile, front_output) {
                    continue;
                }
                let Some(source_side) = source_side else {
                    continue;
                };
                if !self.underground_allows_side_load(front_output, source_side) {
                    let kind = BeltInteractionKind::BlockedFront;
                    transport.insert_interaction(BeltInteraction::new(
                        kind,
                        record.line_id,
                        None,
                        None,
                        source_sort_tile,
                    ));
                    transport.insert_node(TransportNode::blocked_front(
                        make_node_id(),
                        source_sort_tile,
                        record.line_id,
                    ));
                    continue;
                }
                if let Some(near_lane) =
                    target_belt.direction.near_lane_for_source_side(source_side)
                {
                    let kind = BeltInteractionKind::SideLoad { near_lane };
                    transport.insert_interaction(BeltInteraction::new(
                        kind,
                        record.line_id,
                        Some(target.line_id),
                        Some(front_output),
                        source_sort_tile,
                    ));
                    transport.insert_node(TransportNode::side_load_to(
                        make_node_id(),
                        source_sort_tile,
                        front_output,
                        record.line_id,
                        target.line_id,
                        near_lane,
                    ));
                    continue;
                }
            }

            let kind = BeltInteractionKind::BlockedFront;
            transport.insert_interaction(BeltInteraction::new(
                kind,
                record.line_id,
                None,
                None,
                source_sort_tile,
            ));
            transport.insert_node(TransportNode::blocked_front(
                make_node_id(),
                source_sort_tile,
                record.line_id,
            ));
        }
    }

    pub(super) fn process_belt_interactions(&mut self, diff: &mut SimDiff) {
        use crate::transport::node::TransportNodeKind;

        let nodes = self.transport.nodes_sorted().cloned().collect::<Vec<_>>();

        for node in nodes {
            match node.kind {
                TransportNodeKind::EndTransfer => {
                    let source = node.input_ports().next().copied();
                    let target = node.output_ports().next().copied();
                    if let Some(source) = source {
                        self.process_end_transfer(source, target, diff);
                    }
                }
                TransportNodeKind::SideLoad { near_lane } => {
                    let source = node.input_ports().next().copied();
                    let target = node.output_ports().next().copied();
                    if let Some(source) = source {
                        self.process_side_load(source, target, near_lane, diff);
                    }
                }
                TransportNodeKind::Splitter2x1 => {
                    self.process_splitter_node(node, diff);
                }
                TransportNodeKind::Underground => {
                    self.process_underground_node(node, diff);
                }
                TransportNodeKind::BlockedFront => {}
            }
        }
    }

    fn belt_surfaces_match(&self, source: TilePos, target: TilePos) -> bool {
        let Some(source_belt) = self.topology_graph.belt(source) else {
            return false;
        };
        let Some(target_belt) = self.topology_graph.belt(target) else {
            return false;
        };
        source_belt.surface_z == target_belt.surface_z
    }

    pub(super) fn process_underground_node(
        &mut self,
        node: crate::transport::node::TransportNode,
        diff: &mut SimDiff,
    ) {
        let input_ports = node.input_ports().copied().collect::<Vec<_>>();
        let output_ports = node.output_ports().copied().collect::<Vec<_>>();
        if input_ports.len() != 2 || output_ports.len() != 2 {
            return;
        }
        let crate::transport::node::TransportNodeRuntime::Underground(mut runtime) = node.runtime
        else {
            return;
        };
        if node.direction.is_none() {
            return;
        }
        let speed = self
            .topology_graph
            .underground_link(node.sort_tile)
            .map(|link| link.speed)
            .unwrap_or_else(|| UnitsPerTick::new(4));

        if egress_underground_items(self, &output_ports, &mut runtime) {
            mark_underground_node_lines_changed(self, &input_ports, &output_ports, diff);
        }
        if advance_underground_items(&mut runtime, speed.distance_per_tick()) {
            mark_underground_node_lines_changed(self, &input_ports, &output_ports, diff);
        }
        if ingress_underground_items(self, &input_ports, &mut runtime) {
            mark_underground_node_lines_changed(self, &input_ports, &output_ports, diff);
        }

        if let Some(stored) = self.transport.underground_runtime_mut(node.id) {
            *stored = runtime;
        }
    }

    pub(super) fn process_splitter_node(
        &mut self,
        node: crate::transport::node::TransportNode,
        diff: &mut SimDiff,
    ) {
        let input_ports = node.input_ports().copied().collect::<Vec<_>>();
        let output_ports = node.output_ports().copied().collect::<Vec<_>>();
        if input_ports.len() != 4 || output_ports.len() != 4 {
            return;
        }

        let mut runtime = match node.runtime {
            crate::transport::node::TransportNodeRuntime::Splitter(runtime) => runtime,
            crate::transport::node::TransportNodeRuntime::None
            | crate::transport::node::TransportNodeRuntime::Underground(_) => return,
        };

        let splitter_speed = self
            .building_by_origin
            .get(&node.sort_tile)
            .and_then(|building_id| self.buildings.get(building_id))
            .and_then(|building| self.catalog.building_by_id(&building.def_id))
            .and_then(|def| match def.behavior.driver {
                CoreBuildingDriver::Splitter {
                    speed_units_per_tick,
                } => Some(speed_units_per_tick),
                _ => None,
            })
            .unwrap_or_else(|| UnitsPerTick::new(4));
        let mut remaining_egress_items = Vec::with_capacity(runtime.egress_items.len());
        for item in runtime.egress_items {
            if item.progress < SPLITTER_STAGE_END {
                remaining_egress_items.push(item);
                continue;
            }

            let Some(target_port) = splitter_output_port_for_channel_lane(
                &output_ports,
                item.output_channel,
                item.lane,
            ) else {
                remaining_egress_items.push(item);
                continue;
            };
            let Some(target) = self.transport.line_mut(target_port.line) else {
                remaining_egress_items.push(item);
                continue;
            };
            if target.insert_item_at_entry_boundary(target_port.lane, item.item) {
                self.activation.wake_line(target_port.line);
                self.mark_line_changed(target_port.line, diff);
            } else {
                remaining_egress_items.push(item);
            }
        }
        runtime.egress_items = remaining_egress_items;

        advance_splitter_egress_items(&mut runtime, splitter_speed.distance_per_tick());
        advance_splitter_ingress_items(&mut runtime, splitter_speed.distance_per_tick());

        let mut remaining_ingress_items = Vec::with_capacity(runtime.ingress_items.len());
        for item in runtime.ingress_items {
            if item.progress >= SPLITTER_STAGE_END
                && runtime.buffered_items.len() < SPLITTER_BUFFER_CAPACITY
            {
                runtime
                    .buffered_items
                    .push(crate::transport::node::SplitterBufferedItem {
                        item: item.item,
                        source_channel: item.input_channel,
                        lane: item.lane,
                    });
            } else {
                remaining_ingress_items.push(item);
            }
        }
        runtime.ingress_items = remaining_ingress_items;

        while let Some(buffered_item) = runtime.buffered_items.first().cloned() {
            let Some(output_channel) = self.available_splitter_output_channel(
                &output_ports,
                &runtime,
                buffered_item.lane,
                buffered_item.item,
            ) else {
                break;
            };
            runtime.buffered_items.remove(0);
            runtime
                .egress_items
                .push(crate::transport::node::SplitterEgressItem {
                    item: buffered_item.item,
                    source_channel: buffered_item.source_channel,
                    output_channel,
                    lane: buffered_item.lane,
                    progress: DistanceUnits::ZERO,
                });
            runtime.set_next_output_for_lane(buffered_item.lane, output_channel + 1);
        }

        for (input_index, source_port) in input_ports.into_iter().enumerate() {
            let input_channel = input_index / 2;
            let source_line = source_port.line;
            let source_lane = source_port.lane;
            if !splitter_internal_ingress_lane_can_accept(&runtime, input_channel, source_lane) {
                continue;
            }
            let Some(_) = self
                .transport
                .line(source_line)
                .and_then(|line| {
                    line.first_in_window(source_lane, DistanceUnits::ZERO, DistanceUnits::ZERO)
                })
                .map(|position| position.item)
            else {
                continue;
            };
            let Some(moved_item) = self
                .transport
                .line_mut(source_line)
                .and_then(|source| source.pop_front_item(source_lane))
            else {
                continue;
            };
            runtime
                .ingress_items
                .push(crate::transport::node::SplitterIngressItem {
                    item: moved_item,
                    input_channel,
                    lane: source_lane,
                    progress: DistanceUnits::ZERO,
                });
            self.activation.wake_line(source_line);
            self.mark_line_changed(source_line, diff);
        }

        if let Some(stored) = self.transport.splitter_runtime_mut(node.id) {
            *stored = runtime.clone();
        }
        if let Some(building_id) = self.building_by_origin.get(&node.sort_tile).copied()
            && let Some(building) = self.buildings.get_mut(&building_id)
            && self
                .catalog
                .building_by_id(&building.def_id)
                .is_some_and(|def| {
                    matches!(def.behavior.driver, CoreBuildingDriver::Splitter { .. })
                })
        {
            building.state = SimBuildingState::Splitter(runtime);
        }
    }

    fn available_splitter_output_channel(
        &self,
        output_ports: &[crate::transport::node::TransportPort],
        runtime: &crate::transport::node::SplitterRuntime,
        lane: usize,
        item: ItemKindId,
    ) -> Option<usize> {
        for offset in 0..2 {
            let channel = (runtime.next_output_for_lane(lane) + offset) % 2;
            if !splitter_internal_egress_lane_can_accept(runtime, channel, lane) {
                continue;
            }
            let Some(port) = splitter_output_port_for_channel_lane(output_ports, channel, lane)
            else {
                continue;
            };
            if self
                .transport
                .line(port.line)
                .map(|line| {
                    let mut clone = line.clone();
                    clone.insert_item_at_entry_boundary(lane, item)
                })
                .unwrap_or(false)
            {
                return Some(channel);
            }
        }

        for offset in 0..2 {
            let channel = (runtime.next_output_for_lane(lane) + offset) % 2;
            if !splitter_internal_egress_lane_can_accept(runtime, channel, lane) {
                continue;
            }
            let Some(_) = splitter_output_port_for_channel_lane(output_ports, channel, lane) else {
                continue;
            };
            return Some(channel);
        }
        None
    }

    pub(super) fn process_end_transfer(
        &mut self,
        source_port: crate::transport::node::TransportPort,
        target_port: Option<crate::transport::node::TransportPort>,
        diff: &mut SimDiff,
    ) {
        let source_line = source_port.line;
        let Some(target_port) = target_port else {
            return;
        };
        let target_line = target_port.line;
        let Some((_, target_tile_min_distance, _)) =
            self.line_tile_window(target_line, target_port.tile)
        else {
            return;
        };
        let Some(target_insert_distance) = self
            .transport
            .line(target_line)
            .map(|line| line.entry_boundary_insert_distance())
        else {
            return;
        };
        let (progress_numerator, progress_denominator) =
            render_progress_for_distance(target_tile_min_distance, target_insert_distance);

        for lane in 0..2 {
            let Some(item) = self
                .transport
                .line(source_line)
                .and_then(|line| line.first_in_window(lane, DistanceUnits::ZERO, FRONT_WINDOW_LEN))
                .map(|position| position.item)
            else {
                continue;
            };

            let target_can_accept = self
                .transport
                .line(target_line)
                .map(|line| {
                    let mut clone = line.clone();
                    clone.insert_item_at_entry_boundary(lane, item)
                })
                .unwrap_or(false);
            if !target_can_accept {
                continue;
            }

            let Some(moved_item) = self
                .transport
                .line_mut(source_line)
                .and_then(|source| source.pop_front_item(lane))
            else {
                continue;
            };
            let Some(target) = self.transport.line_mut(target_line) else {
                continue;
            };
            if target.insert_item_at_entry_boundary(lane, moved_item) {
                self.activation.wake_line(source_line);
                self.activation.wake_line(target_line);
                self.mark_line_changed(source_line, diff);
                self.mark_line_changed(target_line, diff);
                self.emit_route_hint(
                    diff,
                    moved_item,
                    source_port,
                    target_port,
                    lane,
                    lane,
                    progress_numerator,
                    progress_denominator,
                );
            }
        }
    }

    pub(super) fn process_side_load(
        &mut self,
        source_port: crate::transport::node::TransportPort,
        target_port: Option<crate::transport::node::TransportPort>,
        near_lane: usize,
        diff: &mut SimDiff,
    ) {
        let source_line = source_port.line;
        let Some(target_port) = target_port else {
            return;
        };
        let target_line = target_port.line;
        let target_tile = target_port.tile;
        let Some((_, min_distance, max_distance)) = self.line_tile_window(target_line, target_tile)
        else {
            return;
        };
        let source_side = source_side_for_target(source_port.tile, target_tile);
        let target_direction = self
            .topology_graph
            .belt(target_tile)
            .map(|belt| belt.direction);
        let target_item_count = self
            .transport
            .line(target_line)
            .map(|line| line.lane(near_lane).item_count())
            .unwrap_or_default();
        let mut source_lanes = if target_item_count.is_multiple_of(2) {
            [0, 1]
        } else {
            [1, 0]
        };
        if target_direction
            .zip(source_side)
            .is_some_and(|(direction, side)| side == direction.right())
        {
            source_lanes = [0, 1];
        }

        for source_lane in source_lanes {
            let Some(source_position) = self.transport.line(source_line).and_then(|line| {
                line.first_in_window(source_lane, DistanceUnits::ZERO, FRONT_WINDOW_LEN)
            }) else {
                continue;
            };
            let item = source_position.item;
            let insert_distances = side_load_insert_distances(
                min_distance,
                max_distance,
                target_direction,
                source_side,
                source_lane,
            );

            let target_can_accept = self
                .transport
                .line(target_line)
                .map(|line| {
                    let mut clone = line.clone();
                    if source_line == target_line {
                        clone.remove_one_at_distance(source_lane, source_position.distance);
                    }
                    insert_side_load_item(&mut clone, near_lane, item, &insert_distances).is_some()
                })
                .unwrap_or(false);
            if !target_can_accept {
                continue;
            }

            let Some(source) = self.transport.line_mut(source_line) else {
                continue;
            };
            let Some(moved_item) = source.pop_front_item(source_lane) else {
                continue;
            };
            let Some(target) = self.transport.line_mut(target_line) else {
                continue;
            };
            if let Some(inserted_distance) =
                insert_side_load_item(target, near_lane, moved_item, &insert_distances)
            {
                let (progress_numerator, progress_denominator) =
                    render_progress_for_distance(min_distance, inserted_distance);
                self.activation.wake_line(source_line);
                self.activation.wake_line(target_line);
                self.mark_line_changed(source_line, diff);
                self.mark_line_changed(target_line, diff);
                self.emit_route_hint(
                    diff,
                    moved_item,
                    source_port,
                    target_port,
                    source_lane,
                    near_lane,
                    progress_numerator,
                    progress_denominator,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_route_hint(
        &self,
        diff: &mut SimDiff,
        item: ItemKindId,
        source_port: crate::transport::node::TransportPort,
        target_port: crate::transport::node::TransportPort,
        source_lane: usize,
        target_lane: usize,
        progress_numerator: u16,
        progress_denominator: u16,
    ) {
        self.emit_route_hint_at_center(
            diff,
            item,
            source_port,
            target_port,
            source_lane,
            target_lane,
            target_port.tile,
            progress_numerator,
            progress_denominator,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_route_hint_at_center(
        &self,
        diff: &mut SimDiff,
        item: ItemKindId,
        source_port: crate::transport::node::TransportPort,
        target_port: crate::transport::node::TransportPort,
        source_lane: usize,
        target_lane: usize,
        center: TilePos,
        progress_numerator: u16,
        progress_denominator: u16,
    ) {
        let mut from = source_port.as_ref();
        from.lane = source_lane;
        let mut to = target_port.as_ref();
        to.lane = target_lane;
        diff.route_hints
            .push(crate::transport::node::VisualRouteHint {
                item,
                from,
                to,
                center,
                target_tile: target_port.tile,
                progress_numerator,
                progress_denominator,
                start_tick: self.tick.raw(),
                end_tick: self.tick.raw() + 1,
            });
    }

    pub(super) fn mark_line_changed(&self, line_id: LineId, diff: &mut SimDiff) {
        diff.changed_lines.push(line_id);
        if let Some(line) = self.transport.line(line_id) {
            for tile in line.path().tiles() {
                diff.changed_chunks.push(tile.pos.chunk_pos());
            }
        }
    }
}

fn render_progress_for_distance(
    tile_min_distance: DistanceUnits,
    distance: DistanceUnits,
) -> (u16, u16) {
    let progress_denominator = 128_u16;
    let tile_distance_span = DistanceUnits::UNITS_PER_TILE - 1;
    let local_distance = (distance.raw() - tile_min_distance.raw()).clamp(0, tile_distance_span);
    let progress_numerator = ((((tile_distance_span - local_distance)
        * i32::from(progress_denominator))
        + (tile_distance_span / 2))
        / tile_distance_span) as u16;
    (progress_numerator, progress_denominator)
}

fn underground_lane_can_accept(
    runtime: &crate::transport::node::UndergroundTransportRuntime,
    lane: usize,
) -> bool {
    let ingress_progress = underground_ingress_progress();
    !runtime
        .items
        .iter()
        .any(|item| item.lane == lane && item.progress < ingress_progress + MIN_ITEM_SPACING)
}

fn underground_ingress_progress() -> DistanceUnits {
    DistanceUnits::new(DistanceUnits::UNITS_PER_TILE / 2)
}

fn advance_underground_items(
    runtime: &mut crate::transport::node::UndergroundTransportRuntime,
    delta: DistanceUnits,
) -> bool {
    let mut changed = false;
    for lane in 0..2 {
        let mut indices = runtime
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| (item.lane == lane).then_some(index))
            .collect::<Vec<_>>();
        indices.sort_by(|&left, &right| {
            runtime.items[right]
                .progress
                .cmp(&runtime.items[left].progress)
                .then_with(|| left.cmp(&right))
        });

        let mut ahead_progress = None;
        for index in indices {
            let item = &mut runtime.items[index];
            let lane_limit = ahead_progress
                .map(|progress: DistanceUnits| progress.saturating_sub(MIN_ITEM_SPACING))
                .unwrap_or(runtime.distance);
            let next = (item.progress + delta).min(lane_limit);
            if next != item.progress {
                item.progress = next;
                changed = true;
            }
            ahead_progress = Some(item.progress);
        }
    }
    changed
}

fn mark_underground_node_lines_changed(
    world: &mut SimWorld,
    input_ports: &[crate::transport::node::TransportPort],
    output_ports: &[crate::transport::node::TransportPort],
    diff: &mut SimDiff,
) {
    let mut marked_lines = Vec::new();
    for port in input_ports.iter().chain(output_ports.iter()) {
        if marked_lines.contains(&port.line) {
            continue;
        }
        marked_lines.push(port.line);
        world.activation.wake_line(port.line);
        world.mark_line_changed(port.line, diff);
    }
}

fn egress_underground_items(
    world: &mut SimWorld,
    output_ports: &[crate::transport::node::TransportPort],
    runtime: &mut crate::transport::node::UndergroundTransportRuntime,
) -> bool {
    let mut changed = false;
    let mut remaining_items = Vec::with_capacity(runtime.items.len());
    for item in runtime.items.drain(..) {
        if item.progress < runtime.distance {
            remaining_items.push(item);
            continue;
        }

        let Some(target_port) = output_ports
            .iter()
            .find(|port| port.lane == item.lane)
            .copied()
        else {
            remaining_items.push(item);
            continue;
        };
        let Some(target) = world.transport.line_mut(target_port.line) else {
            remaining_items.push(item);
            continue;
        };
        if target.insert_item_at_entry_boundary(target_port.lane, item.item) {
            changed = true;
        } else {
            remaining_items.push(item);
        }
    }
    runtime.items = remaining_items;
    changed
}

fn ingress_underground_items(
    world: &mut SimWorld,
    input_ports: &[crate::transport::node::TransportPort],
    runtime: &mut crate::transport::node::UndergroundTransportRuntime,
) -> bool {
    let mut changed = false;
    for source_port in input_ports {
        let source_line = source_port.line;
        let source_lane = source_port.lane;
        if !underground_lane_can_accept(runtime, source_lane) {
            continue;
        }
        let Some(_) = world.transport.line(source_line).and_then(|line| {
            line.first_in_window(source_lane, DistanceUnits::ZERO, DistanceUnits::ZERO)
        }) else {
            continue;
        };
        let Some(moved_item) = world
            .transport
            .line_mut(source_line)
            .and_then(|source| source.pop_front_item(source_lane))
        else {
            continue;
        };
        runtime
            .items
            .push(crate::transport::node::UndergroundTransportItem {
                item: moved_item,
                lane: source_lane,
                progress: underground_ingress_progress(),
            });
        changed = true;
    }
    changed
}

fn splitter_output_port_for_channel_lane(
    output_ports: &[crate::transport::node::TransportPort],
    channel: usize,
    lane: usize,
) -> Option<crate::transport::node::TransportPort> {
    output_ports
        .iter()
        .filter(|port| port.lane == lane)
        .nth(channel)
        .copied()
}

fn advance_splitter_egress_items(
    runtime: &mut crate::transport::node::SplitterRuntime,
    distance: DistanceUnits,
) {
    let mut indices = (0..runtime.egress_items.len()).collect::<Vec<_>>();
    indices.sort_by(|&left, &right| {
        let left_item = &runtime.egress_items[left];
        let right_item = &runtime.egress_items[right];
        (left_item.output_channel, left_item.lane)
            .cmp(&(right_item.output_channel, right_item.lane))
            .then_with(|| right_item.progress.cmp(&left_item.progress))
            .then_with(|| left.cmp(&right))
    });

    let mut front_progress_by_lane: [[Option<DistanceUnits>; 2]; 2] = [[None; 2]; 2];
    for index in indices {
        let item = &mut runtime.egress_items[index];
        let limit = front_progress_by_lane[item.output_channel][item.lane]
            .map(|front| front.saturating_sub(MIN_ITEM_SPACING))
            .unwrap_or(SPLITTER_STAGE_END);
        if item.progress < limit {
            item.progress = (item.progress + distance).min(limit);
        }
        front_progress_by_lane[item.output_channel][item.lane] = Some(item.progress);
    }
}

fn advance_splitter_ingress_items(
    runtime: &mut crate::transport::node::SplitterRuntime,
    distance: DistanceUnits,
) {
    let mut indices = (0..runtime.ingress_items.len()).collect::<Vec<_>>();
    indices.sort_by(|&left, &right| {
        let left_item = &runtime.ingress_items[left];
        let right_item = &runtime.ingress_items[right];
        (left_item.input_channel, left_item.lane)
            .cmp(&(right_item.input_channel, right_item.lane))
            .then_with(|| right_item.progress.cmp(&left_item.progress))
            .then_with(|| left.cmp(&right))
    });

    let mut front_progress_by_lane: [[Option<DistanceUnits>; 2]; 2] = [[None; 2]; 2];
    for index in indices {
        let item = &mut runtime.ingress_items[index];
        let limit = front_progress_by_lane[item.input_channel][item.lane]
            .map(|front| front.saturating_sub(MIN_ITEM_SPACING))
            .unwrap_or(SPLITTER_STAGE_END);
        if item.progress < limit {
            item.progress = (item.progress + distance).min(limit);
        }
        front_progress_by_lane[item.input_channel][item.lane] = Some(item.progress);
    }
}

fn splitter_internal_egress_lane_can_accept(
    runtime: &crate::transport::node::SplitterRuntime,
    channel: usize,
    lane: usize,
) -> bool {
    !runtime.egress_items.iter().any(|item| {
        item.output_channel == channel && item.lane == lane && item.progress < MIN_ITEM_SPACING
    })
}

fn splitter_internal_ingress_lane_can_accept(
    runtime: &crate::transport::node::SplitterRuntime,
    input_channel: usize,
    lane: usize,
) -> bool {
    !runtime.ingress_items.iter().any(|item| {
        item.input_channel == input_channel && item.lane == lane && item.progress < MIN_ITEM_SPACING
    })
}
