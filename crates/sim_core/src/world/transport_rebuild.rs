//! Rebuild transport lines after topology changes (merge, split, side-load remap).

use super::*;
use crate::topology::builder::BuiltPathTile;

pub(super) struct BuiltLineRecord {
    pub(super) line_id: LineId,
    pub(super) tiles: Vec<BuiltPathTile>,
    pub(super) closed: bool,
    pub(super) front_output: Option<TilePos>,
}

pub(super) struct OldTransportLine {
    pub(super) tiles: Vec<TilePos>,
    pub(super) speed: UnitsPerTick,
    pub(super) lanes: [PackedItemStream; 2],
}

pub(super) struct SplitterChannelGeometry {
    pub(super) channel_tile: TilePos,
    pub(super) input_pos: TilePos,
    pub(super) output_pos: TilePos,
}

pub(super) fn line_by_first_tile(
    records: &[BuiltLineRecord],
    tile: TilePos,
) -> Option<&BuiltLineRecord> {
    records
        .iter()
        .find(|record| record.tiles.first().is_some_and(|first| first.pos == tile))
}

pub(super) fn line_containing_tile(
    records: &[BuiltLineRecord],
    tile: TilePos,
) -> Option<&BuiltLineRecord> {
    records
        .iter()
        .find(|record| record.tiles.iter().any(|path_tile| path_tile.pos == tile))
}

pub(super) fn line_ending_from_tile_to_tile(
    records: &[BuiltLineRecord],
    source_tile: TilePos,
    target_tile: TilePos,
) -> Option<LineId> {
    records
        .iter()
        .find(|record| {
            !record.closed
                && record.front_output == Some(target_tile)
                && record
                    .tiles
                    .last()
                    .is_some_and(|tile| tile.pos == source_tile)
        })
        .map(|record| record.line_id)
}

pub(super) fn splitter_channel_geometry(
    footprint: &[TilePos],
    direction: Direction,
) -> Option<[SplitterChannelGeometry; 2]> {
    let mut channel_tiles = footprint.to_vec();
    channel_tiles.sort();
    let [first, second] = channel_tiles.as_slice() else {
        return None;
    };
    Some([
        splitter_channel_geometry_for_tile(*first, direction),
        splitter_channel_geometry_for_tile(*second, direction),
    ])
}

fn splitter_channel_geometry_for_tile(
    channel_tile: TilePos,
    direction: Direction,
) -> SplitterChannelGeometry {
    SplitterChannelGeometry {
        channel_tile,
        input_pos: direction.opposite().output_pos(channel_tile),
        output_pos: direction.output_pos(channel_tile),
    }
}

pub(super) fn source_side_for_target(
    source_front: TilePos,
    target_tile: TilePos,
) -> Option<Direction> {
    match (
        source_front.x - target_tile.x,
        source_front.y - target_tile.y,
    ) {
        (-1, 0) => Some(Direction::West),
        (1, 0) => Some(Direction::East),
        (0, -1) => Some(Direction::South),
        (0, 1) => Some(Direction::North),
        _ => None,
    }
}

pub(super) fn side_load_insert_distances(
    min_distance: DistanceUnits,
    max_distance: DistanceUnits,
    target_direction: Option<Direction>,
    source_side: Option<Direction>,
    source_lane: usize,
) -> Vec<DistanceUnits> {
    let span = max_distance.raw() - min_distance.raw();
    let use_exit_half =
        side_load_source_lane_uses_exit_half(target_direction, source_side, source_lane);
    let offset = if use_exit_half {
        span / 4
    } else {
        span * 3 / 4
    };
    let preferred = DistanceUnits::new(min_distance.raw() + offset);
    let mut candidates = vec![preferred];
    let step = (MIN_ITEM_SPACING.raw() / 4).max(1);
    let midpoint = min_distance.raw() + span / 2;
    let (scan_min, scan_max) = if use_exit_half {
        (min_distance.raw(), midpoint)
    } else {
        (midpoint, max_distance.raw() - 1)
    };
    let mut raw = scan_min;
    while raw <= scan_max {
        let candidate = DistanceUnits::new(raw);
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
        raw += step;
    }
    let edge_raw = scan_max;
    if !candidates.contains(&DistanceUnits::new(edge_raw)) {
        candidates.push(DistanceUnits::new(edge_raw));
    }
    let mut raw = scan_min + step / 2;
    while raw <= scan_max {
        let candidate = DistanceUnits::new(raw);
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
        raw += step;
    }
    candidates.sort_by_key(|distance| (distance.raw() - preferred.raw()).abs());
    candidates
}

pub(super) fn side_load_source_lane_uses_exit_half(
    target_direction: Option<Direction>,
    source_side: Option<Direction>,
    source_lane: usize,
) -> bool {
    let Some(target_direction) = target_direction else {
        return source_lane == 1;
    };
    let Some(source_side) = source_side else {
        return source_lane == 1;
    };
    let source_forward = source_side.opposite();
    let source_lane_side = if source_lane == 0 {
        source_forward.left()
    } else {
        source_forward.right()
    };
    source_lane_side == target_direction
}

pub(super) fn insert_side_load_item(
    line: &mut TransportLine,
    lane: usize,
    item: ItemKindId,
    distances: &[DistanceUnits],
) -> Option<DistanceUnits> {
    distances
        .iter()
        .copied()
        .find(|&distance| line.insert_one_in_window(lane, item, distance))
}

pub(super) fn remap_lanes_for_new_line(
    old_lines: &[OldTransportLine],
    new_tiles: &[TilePos],
    speed: UnitsPerTick,
    removed_item_drops: &mut Vec<CoreRemovalDrop>,
) -> [PackedItemStream; 2] {
    let mut lanes = [PackedItemStream::default(), PackedItemStream::default()];
    let new_total_len = (new_tiles.len() as i32) * DistanceUnits::UNITS_PER_TILE;
    for old_line in old_lines.iter().filter(|line| line.speed == speed) {
        let old_total_len = (old_line.tiles.len() as i32) * DistanceUnits::UNITS_PER_TILE;
        for (lane_index, lane) in lanes.iter_mut().enumerate() {
            for position in old_line.lanes[lane_index].positions_in_range(
                DistanceUnits::ZERO,
                DistanceUnits::new(old_total_len.saturating_sub(1)),
            ) {
                let Some((old_tile, local_offset)) =
                    line_tile_and_local_offset(&old_line.tiles, position.distance)
                else {
                    continue;
                };
                let Some(new_index) = new_tiles.iter().position(|tile| *tile == old_tile) else {
                    continue;
                };
                let new_min_distance =
                    new_total_len - ((new_index as i32 + 1) * DistanceUnits::UNITS_PER_TILE);
                let distance = DistanceUnits::new(new_min_distance + local_offset);
                if !lane.insert_one_at_distance_with_terminal_end(
                    position.item,
                    distance,
                    Some(DistanceUnits::new(new_total_len)),
                ) {
                    push_removed_belt_item_drop(removed_item_drops, old_tile, position.item);
                }
            }
        }
    }
    lanes
}

pub(super) fn drop_old_items_without_surviving_tile(
    old_lines: &[OldTransportLine],
    all_new_tiles: &BTreeSet<TilePos>,
    removed_item_drops: &mut Vec<CoreRemovalDrop>,
) {
    for old_line in old_lines {
        let old_total_len = (old_line.tiles.len() as i32) * DistanceUnits::UNITS_PER_TILE;
        for lane in &old_line.lanes {
            for position in lane.positions_in_range(
                DistanceUnits::ZERO,
                DistanceUnits::new(old_total_len.saturating_sub(1)),
            ) {
                let Some((old_tile, _local_offset)) =
                    line_tile_and_local_offset(&old_line.tiles, position.distance)
                else {
                    continue;
                };
                if !all_new_tiles.contains(&old_tile) {
                    push_removed_belt_item_drop(removed_item_drops, old_tile, position.item);
                }
            }
        }
    }
}

pub(super) fn line_tile_and_local_offset(
    tiles: &[TilePos],
    distance: DistanceUnits,
) -> Option<(TilePos, i32)> {
    let total_len = (tiles.len() as i32) * DistanceUnits::UNITS_PER_TILE;
    if distance.raw() < 0 || distance.raw() >= total_len {
        return None;
    }
    let index_from_start = (total_len - 1 - distance.raw()) / DistanceUnits::UNITS_PER_TILE;
    let tile = *tiles.get(index_from_start as usize)?;
    let tile_min_distance = total_len - ((index_from_start + 1) * DistanceUnits::UNITS_PER_TILE);
    Some((tile, distance.raw() - tile_min_distance))
}

pub(super) fn push_removed_belt_item_drop(
    removed_item_drops: &mut Vec<CoreRemovalDrop>,
    origin: TilePos,
    item: ItemKindId,
) {
    removed_item_drops.push(CoreRemovalDrop {
        origin,
        stack: CoreItemStack {
            kind: item,
            amount: 1,
        },
        instance: None,
    });
}
