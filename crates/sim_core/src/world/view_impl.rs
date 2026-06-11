//! [`SimWorld::visible_items_for_bounds`] and related render-facing queries.

use super::view::visible_path_index_interval;
use super::*;
use crate::view::{VisibleSplitterItem, VisibleSplitterItemPhase};

impl SimWorld {
    pub fn visible_items_for_bounds(
        &self,
        bounds: VisibleTileBounds,
    ) -> impl Iterator<Item = VisibleItem> + '_ {
        self.collect_visible_items_for_bounds(bounds, None)
            .into_iter()
    }
    pub fn visible_item_query_item_visits_for_tests(&self, bounds: VisibleTileBounds) -> usize {
        let mut stats = VisibleItemQueryStatsForTests::default();
        self.collect_visible_items_for_bounds(bounds, Some(&mut stats));
        stats.item_visits
    }

    pub fn visible_splitter_items_for_bounds(
        &self,
        bounds: VisibleTileBounds,
    ) -> impl Iterator<Item = VisibleSplitterItem> + '_ {
        self.collect_visible_splitter_items_for_bounds(bounds)
            .into_iter()
    }

    pub fn visible_items_for_bounds_with_stats_for_tests(
        &self,
        bounds: VisibleTileBounds,
    ) -> (Vec<VisibleItem>, VisibleItemQueryStatsForTests) {
        let mut stats = VisibleItemQueryStatsForTests::default();
        let items = self.collect_visible_items_for_bounds(bounds, Some(&mut stats));
        (items, stats)
    }

    pub(super) fn collect_visible_items_for_bounds(
        &self,
        bounds: VisibleTileBounds,
        mut stats: Option<&mut VisibleItemQueryStatsForTests>,
    ) -> Vec<VisibleItem> {
        let mut items = Vec::new();
        for line_id in self.transport.line_ids_sorted() {
            let Some(line) = self.transport.line(line_id) else {
                continue;
            };

            let total_len = line.path().total_len().raw();
            let tiles = line.path().tiles();
            let Some((min_index, max_index)) = visible_path_index_interval(
                tiles,
                bounds,
                stats
                    .as_deref_mut()
                    .map(|stats| &mut stats.path_tile_visits),
            ) else {
                continue;
            };
            let min_distance = total_len - ((max_index as i32 + 1) * DistanceUnits::UNITS_PER_TILE);
            let max_distance = total_len - (min_index as i32 * DistanceUnits::UNITS_PER_TILE) - 1;

            for lane_index in 0..2 {
                let query = line.lane_positions_in_range_with_report(
                    lane_index,
                    DistanceUnits::new(min_distance),
                    DistanceUnits::new(max_distance),
                );
                if let Some(stats) = stats.as_deref_mut() {
                    stats.item_visits += query.items_scanned;
                }

                for position in query.positions {
                    let raw_distance = position.distance.raw();
                    if raw_distance >= min_distance && raw_distance >= 0 && raw_distance < total_len
                    {
                        let index_from_start =
                            (total_len - 1 - raw_distance) / DistanceUnits::UNITS_PER_TILE;
                        if let Some(tile) = tiles.get(index_from_start as usize)
                            && bounds.contains(tile.pos)
                        {
                            let tile_min_distance = total_len
                                - ((index_from_start + 1) * DistanceUnits::UNITS_PER_TILE);
                            let progress_denominator = 128_u16;
                            let tile_distance_span = DistanceUnits::UNITS_PER_TILE - 1;
                            let local_distance =
                                (raw_distance - tile_min_distance).clamp(0, tile_distance_span);
                            let progress_numerator = ((((tile_distance_span - local_distance)
                                * i32::from(progress_denominator))
                                + (tile_distance_span / 2))
                                / tile_distance_span)
                                as u16;
                            let current_belt = self.topology_graph.belt(tile.pos);
                            let direction = current_belt
                                .map(|belt| belt.direction)
                                .unwrap_or(Direction::East);
                            let previous_index = if index_from_start > 0 {
                                Some(index_from_start as usize - 1)
                            } else if line.closed() {
                                tiles.len().checked_sub(1)
                            } else {
                                None
                            };
                            let entry_direction = previous_index
                                .and_then(|index| tiles.get(index))
                                .and_then(|previous_tile| {
                                    self.topology_graph
                                        .belt(previous_tile.pos)
                                        .map(|belt| belt.direction)
                                })
                                .unwrap_or_else(|| {
                                    current_belt
                                        .map(|belt| belt.input_direction)
                                        .unwrap_or(direction)
                                });
                            items.push(VisibleItem {
                                tile: tile.pos,
                                item: position.item,
                                lane: lane_index,
                                entry_direction,
                                direction,
                                progress_numerator,
                                progress_denominator,
                                route_hint: None,
                                splitter_route_hint: None,
                            });
                        }
                    }
                }
            }
        }
        for position in self.underground_item_positions_for_bounds(bounds) {
            let current_belt = self.topology_graph.belt(position.tile);
            let direction = current_belt
                .map(|belt| belt.direction)
                .unwrap_or(position.direction);
            let entry_direction = current_belt
                .map(|belt| belt.input_direction)
                .unwrap_or(position.direction);
            items.push(VisibleItem {
                tile: position.tile,
                item: position.item,
                lane: position.lane,
                entry_direction,
                direction,
                progress_numerator: underground_endpoint_progress_numerator(
                    position.phase,
                    position.progress,
                    position.distance,
                    128,
                ),
                progress_denominator: 128,
                route_hint: None,
                splitter_route_hint: None,
            });
        }
        items
    }

    fn collect_visible_splitter_items_for_bounds(
        &self,
        bounds: VisibleTileBounds,
    ) -> Vec<VisibleSplitterItem> {
        let mut items = Vec::new();
        for node in self.transport.nodes_sorted() {
            if node.kind != crate::transport::node::TransportNodeKind::Splitter2x1 {
                continue;
            }
            let crate::transport::node::TransportNodeRuntime::Splitter(runtime) = &node.runtime
            else {
                continue;
            };
            let Some(direction) = node.direction else {
                continue;
            };
            let input_tiles =
                splitter_channel_tiles(node, crate::transport::node::TransportPortRole::Input);
            let output_tiles =
                splitter_channel_tiles(node, crate::transport::node::TransportPortRole::Output);
            if !input_tiles
                .iter()
                .chain(output_tiles.iter())
                .any(|tile| bounds.contains(*tile))
                && !bounds.contains(node.sort_tile)
            {
                continue;
            }

            for item in &runtime.ingress_items {
                let progress_denominator = 128_u16;
                let progress_numerator =
                    splitter_stage_progress_numerator(item.progress, progress_denominator);
                items.push(VisibleSplitterItem {
                    origin: node.sort_tile,
                    item: item.item,
                    direction,
                    input_channel: item.input_channel,
                    output_channel: item.input_channel,
                    lane: item.lane,
                    phase: VisibleSplitterItemPhase::Ingress,
                    progress_numerator,
                    progress_denominator,
                });
            }
            for item in &runtime.egress_items {
                let progress_denominator = 128_u16;
                let progress_numerator =
                    splitter_stage_progress_numerator(item.progress, progress_denominator);
                items.push(VisibleSplitterItem {
                    origin: node.sort_tile,
                    item: item.item,
                    direction,
                    input_channel: item.source_channel,
                    output_channel: item.output_channel,
                    lane: item.lane,
                    phase: VisibleSplitterItemPhase::Egress,
                    progress_numerator,
                    progress_denominator,
                });
            }
        }
        items
    }
}

fn splitter_stage_progress_numerator(progress: DistanceUnits, denominator: u16) -> u16 {
    let stage_end = DistanceUnits::UNITS_PER_TILE / 2;
    ((progress.raw().clamp(0, stage_end) * i32::from(denominator) + (stage_end / 2)) / stage_end)
        as u16
}

fn underground_endpoint_progress_numerator(
    phase: UndergroundEndpointPhase,
    progress: DistanceUnits,
    distance: DistanceUnits,
    denominator: u16,
) -> u16 {
    let stage_end = DistanceUnits::UNITS_PER_TILE / 2;
    let local_progress = match phase {
        UndergroundEndpointPhase::Entrance => progress.raw(),
        UndergroundEndpointPhase::Exit => {
            (progress - (distance - DistanceUnits::new(stage_end))).raw()
        }
    };
    ((local_progress.clamp(0, stage_end) * i32::from(denominator) + (stage_end / 2)) / stage_end)
        as u16
}

fn splitter_channel_tiles(
    node: &crate::transport::node::TransportNode,
    role: crate::transport::node::TransportPortRole,
) -> [TilePos; 2] {
    let mut tiles = [node.sort_tile; 2];
    let ports = match role {
        crate::transport::node::TransportPortRole::Input => node.input_ports().collect::<Vec<_>>(),
        crate::transport::node::TransportPortRole::Output => {
            node.output_ports().collect::<Vec<_>>()
        }
    };
    for (index, port) in ports.into_iter().enumerate() {
        if port.lane == 0 {
            let channel = index / 2;
            if channel < tiles.len() {
                tiles[channel] = port.tile;
            }
        }
    }
    tiles
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VisibleItemQueryStatsForTests {
    pub item_visits: usize,
    pub path_tile_visits: usize,
}
