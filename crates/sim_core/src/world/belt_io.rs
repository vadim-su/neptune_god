//! Belt lane insert helpers (side load, distance snap, lane ranking).

use super::*;

pub(super) fn insert_belt_item_from_side(
    line: &mut TransportLine,
    lane: usize,
    item: ItemKindId,
    min_distance: DistanceUnits,
    max_distance: DistanceUnits,
    belt_direction: Option<Direction>,
    source_side: Option<Direction>,
) -> bool {
    belt_insert_distance_candidates(min_distance, max_distance, belt_direction, source_side)
        .into_iter()
        .any(|distance| line.insert_one_in_window(lane, item, distance))
}

pub(super) fn belt_insert_distance_candidates(
    min_distance: DistanceUnits,
    max_distance: DistanceUnits,
    belt_direction: Option<Direction>,
    source_side: Option<Direction>,
) -> Vec<DistanceUnits> {
    let min = min_distance.raw();
    let max = max_distance.raw();
    if max < min {
        return Vec::new();
    }

    let center = preferred_belt_insert_distance(min, max, belt_direction, source_side);
    let step = (MIN_ITEM_SPACING.raw() / 4).max(1);
    let mut distances = vec![center, min, max];
    let mut offset = step;
    while center - offset >= min || center + offset <= max {
        if center - offset >= min {
            distances.push(center - offset);
        }
        if center + offset <= max {
            distances.push(center + offset);
        }
        offset += step;
    }
    distances.sort_by_key(|distance| ((distance - center).abs(), *distance));
    distances.dedup();
    distances.into_iter().map(DistanceUnits::new).collect()
}

pub(super) fn preferred_belt_insert_distance(
    min: i32,
    max: i32,
    belt_direction: Option<Direction>,
    source_side: Option<Direction>,
) -> i32 {
    match (belt_direction, source_side) {
        (Some(direction), Some(side)) if side == direction => min,
        (Some(direction), Some(side)) if side == direction.opposite() => max,
        _ => min + (max - min) / 2,
    }
}

pub(super) fn lane_rank(lane: usize, preferred_lane: Option<usize>) -> usize {
    if preferred_lane == Some(lane) {
        0
    } else {
        lane + 1
    }
}

pub(super) fn snap_belt_insert_distance(
    distance: DistanceUnits,
    terminal_end: DistanceUnits,
) -> DistanceUnits {
    let step = MIN_ITEM_SPACING.raw();
    let raw = distance.raw();
    let snapped = ((raw + (step / 2)) / step) * step;
    let max_aligned = ((terminal_end.raw() - 1) / step) * step;
    DistanceUnits::new(snapped.clamp(0, max_aligned))
}
