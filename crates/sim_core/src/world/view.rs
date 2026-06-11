//! Visible belt path interval queries for [`crate::view`] item enumeration.

use super::*;

pub(super) fn visible_path_index_interval(
    tiles: &[LineTile],
    bounds: VisibleTileBounds,
    mut path_tile_visits: Option<&mut usize>,
) -> Option<(usize, usize)> {
    if let Some(interval) = straight_visible_path_index_interval(tiles, bounds) {
        if let Some(path_tile_visits) = path_tile_visits.as_deref_mut() {
            *path_tile_visits += 1;
        }
        return interval;
    }

    let mut min_index = None;
    let mut max_index = 0;
    for (index, tile) in tiles.iter().enumerate() {
        if let Some(path_tile_visits) = path_tile_visits.as_deref_mut() {
            *path_tile_visits += 1;
        }
        if bounds.contains(tile.pos) {
            min_index.get_or_insert(index);
            max_index = index;
        }
    }
    min_index.map(|min_index| (min_index, max_index))
}

fn straight_visible_path_index_interval(
    tiles: &[LineTile],
    bounds: VisibleTileBounds,
) -> Option<Option<(usize, usize)>> {
    if let [tile] = tiles {
        return Some(bounds.contains(tile.pos).then_some((0, 0)));
    }
    let [first, .., last] = tiles else {
        return Some(None);
    };
    if first.pos.y == last.pos.y {
        if !is_contiguous_straight_axis(first.pos.x, last.pos.x, tiles.len()) {
            return None;
        }
        return Some(straight_axis_interval(
            first.pos.x,
            last.pos.x,
            bounds.min().x,
            bounds.max().x,
            bounds.min().y <= first.pos.y && first.pos.y <= bounds.max().y,
        ));
    }
    if first.pos.x == last.pos.x {
        if !is_contiguous_straight_axis(first.pos.y, last.pos.y, tiles.len()) {
            return None;
        }
        return Some(straight_axis_interval(
            first.pos.y,
            last.pos.y,
            bounds.min().y,
            bounds.max().y,
            bounds.min().x <= first.pos.x && first.pos.x <= bounds.max().x,
        ));
    }
    None
}

fn is_contiguous_straight_axis(start: i32, end: i32, tile_count: usize) -> bool {
    end.checked_sub(start)
        .is_some_and(|delta| delta.unsigned_abs() as usize + 1 == tile_count)
}

fn straight_axis_interval(
    start: i32,
    end: i32,
    bounds_min: i32,
    bounds_max: i32,
    orthogonal_axis_visible: bool,
) -> Option<(usize, usize)> {
    if !orthogonal_axis_visible {
        return None;
    }

    let line_min = start.min(end);
    let line_max = start.max(end);
    let visible_min = line_min.max(bounds_min);
    let visible_max = line_max.min(bounds_max);
    if visible_min > visible_max {
        return None;
    }

    if start <= end {
        Some((
            (visible_min - start) as usize,
            (visible_max - start) as usize,
        ))
    } else {
        Some((
            (start - visible_max) as usize,
            (start - visible_min) as usize,
        ))
    }
}
