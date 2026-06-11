//! Gap-based packed item stream (Factorio-style spacing along line distance).

use serde::{Deserialize, Serialize};

use crate::ids::ItemKindId;
use crate::units::DistanceUnits;

/// Minimum center-to-center spacing between items on a line (distance units).
pub const MIN_ITEM_SPACING: DistanceUnits = DistanceUnits::new(64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamAdvanceReport {
    pub items_scanned: usize,
    pub became_compressed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamItemPosition {
    pub item: ItemKindId,
    pub distance: DistanceUnits,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamRangeQuery {
    pub positions: Vec<StreamItemPosition>,
    pub items_scanned: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackedItemStream {
    items: Vec<ItemKindId>,
    gaps_after: Vec<DistanceUnits>,
    front_gap: DistanceUnits,
    back_gap: DistanceUnits,
    cached_frontmost_positive_gap: Option<usize>,
    cached_back_item_distance_from_front: Option<DistanceUnits>,
    revision: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PackedItemStreamSnapshot {
    pub items: Vec<ItemKindId>,
    pub front_gap: DistanceUnits,
    pub gaps_after: Vec<DistanceUnits>,
    pub back_gap: DistanceUnits,
}

impl PackedItemStream {
    pub fn from_gaps(
        items: Vec<ItemKindId>,
        front_gap: DistanceUnits,
        gaps_after: Vec<DistanceUnits>,
        back_gap: DistanceUnits,
    ) -> Self {
        assert_eq!(items.len().saturating_sub(1), gaps_after.len());
        let cached_frontmost_positive_gap = frontmost_compressible_gap(&gaps_after);
        let cached_back_item_distance_from_front = (!items.is_empty()).then(|| {
            let mut distance = front_gap;
            for gap in &gaps_after {
                distance += *gap;
            }
            distance
        });
        Self {
            items,
            gaps_after,
            front_gap,
            back_gap,
            cached_frontmost_positive_gap,
            cached_back_item_distance_from_front,
            revision: 0,
        }
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn items(&self) -> &[ItemKindId] {
        &self.items
    }

    pub fn front_gap(&self) -> DistanceUnits {
        self.front_gap
    }

    pub fn back_gap(&self) -> DistanceUnits {
        self.back_gap
    }

    pub fn gap_after(&self, index: usize) -> DistanceUnits {
        self.gaps_after[index]
    }

    pub fn gaps_after(&self) -> &[DistanceUnits] {
        &self.gaps_after
    }

    pub fn cached_frontmost_positive_gap(&self) -> Option<usize> {
        self.cached_frontmost_positive_gap
    }

    pub fn distance_span_from_front(&self) -> Option<(DistanceUnits, DistanceUnits)> {
        self.cached_back_item_distance_from_front
            .map(|back_distance| (self.front_gap, back_distance))
    }

    pub fn positions_in_range(
        &self,
        start: DistanceUnits,
        end: DistanceUnits,
    ) -> Vec<StreamItemPosition> {
        self.positions_in_range_with_report(start, end).positions
    }

    pub fn positions_in_range_with_report(
        &self,
        start: DistanceUnits,
        end: DistanceUnits,
    ) -> StreamRangeQuery {
        let Some((first_distance, last_distance)) = self.distance_span_from_front() else {
            return StreamRangeQuery::default();
        };
        if last_distance < start || first_distance > end {
            return StreamRangeQuery::default();
        }

        let mut query = StreamRangeQuery::default();
        let mut distance = self.front_gap;
        for (item_index, &item) in self.items.iter().enumerate() {
            query.items_scanned += 1;
            if distance > end {
                break;
            }
            if distance >= start {
                query.positions.push(StreamItemPosition { item, distance });
            }

            if item_index < self.gaps_after.len() {
                distance += self.gaps_after[item_index];
            }
        }
        query
    }

    pub fn remove_one_at_distance(&mut self, distance: DistanceUnits) -> Option<ItemKindId> {
        let mut positions =
            self.positions_in_range(DistanceUnits::ZERO, DistanceUnits::new(i32::MAX));
        let index = positions
            .iter()
            .position(|position| position.distance == distance)?;
        let removed = positions.remove(index).item;
        let terminal_end = self.terminal_end_distance().unwrap_or(DistanceUnits::ZERO);
        self.replace_positions(positions, terminal_end);
        Some(removed)
    }

    pub fn insert_one_at_distance_with_terminal_end(
        &mut self,
        item: ItemKindId,
        distance: DistanceUnits,
        empty_terminal_end: Option<DistanceUnits>,
    ) -> bool {
        let mut positions =
            self.positions_in_range(DistanceUnits::ZERO, DistanceUnits::new(i32::MAX));
        let terminal_end = if self.items.is_empty() {
            empty_terminal_end.unwrap_or(self.front_gap + self.back_gap)
        } else {
            self.terminal_end_distance().unwrap_or(DistanceUnits::ZERO)
        };
        if distance > terminal_end {
            return false;
        }
        if positions.iter().any(|position| {
            (position.distance.raw() - distance.raw()).abs() < MIN_ITEM_SPACING.raw()
        }) {
            return false;
        }
        positions.push(StreamItemPosition { item, distance });
        positions.sort_by_key(|position| position.distance);
        self.replace_positions(positions, terminal_end);
        true
    }

    pub fn insert_one_with_nudge_at_distance_with_terminal_end(
        &mut self,
        item: ItemKindId,
        distance: DistanceUnits,
        empty_terminal_end: Option<DistanceUnits>,
    ) -> bool {
        if self.insert_one_at_distance_with_terminal_end(item, distance, empty_terminal_end) {
            return true;
        }

        let positions = self.positions_in_range(DistanceUnits::ZERO, DistanceUnits::new(i32::MAX));
        let terminal_end = if self.items.is_empty() {
            empty_terminal_end.unwrap_or(self.front_gap + self.back_gap)
        } else {
            self.terminal_end_distance().unwrap_or(DistanceUnits::ZERO)
        };
        let max_distance = terminal_end - DistanceUnits::new(1);
        if distance > max_distance {
            return false;
        }

        let Some(nudged) = nudged_insert_positions(&positions, item, distance, max_distance) else {
            return false;
        };
        self.replace_positions(nudged, terminal_end);
        true
    }

    fn replace_positions(
        &mut self,
        positions: Vec<StreamItemPosition>,
        terminal_end: DistanceUnits,
    ) {
        let revision = self.revision + 1;
        if positions.is_empty() {
            *self = Self {
                back_gap: terminal_end,
                revision,
                ..Self::default()
            };
            return;
        }

        let front_gap = positions[0].distance;
        let back_distance = positions[positions.len() - 1].distance;
        let back_gap = terminal_end - back_distance;
        let items = positions
            .iter()
            .map(|position| position.item)
            .collect::<Vec<_>>();
        let gaps_after = positions
            .windows(2)
            .map(|window| window[1].distance - window[0].distance)
            .collect::<Vec<_>>();
        *self = Self::from_gaps(items, front_gap, gaps_after, back_gap);
        self.revision = revision;
    }

    fn terminal_end_distance(&self) -> Option<DistanceUnits> {
        if self.items.is_empty() {
            Some(self.front_gap + self.back_gap)
        } else {
            self.cached_back_item_distance_from_front
                .map(|distance| distance + self.back_gap)
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn snapshot(&self) -> PackedItemStreamSnapshot {
        PackedItemStreamSnapshot {
            items: self.items.clone(),
            front_gap: self.front_gap,
            gaps_after: self.gaps_after.clone(),
            back_gap: self.back_gap,
        }
    }

    pub fn from_snapshot(snapshot: PackedItemStreamSnapshot) -> Result<Self, String> {
        let expected_gaps = snapshot.items.len().saturating_sub(1);
        let actual_gaps = snapshot.gaps_after.len();
        if actual_gaps != expected_gaps {
            return Err(format!(
                "invalid packed item stream gaps_after count: expected {expected_gaps}, actual {actual_gaps}"
            ));
        }

        Ok(Self::from_gaps(
            snapshot.items,
            snapshot.front_gap,
            snapshot.gaps_after,
            snapshot.back_gap,
        ))
    }

    pub fn is_fully_compressed(&self) -> bool {
        self.front_gap == DistanceUnits::ZERO && self.cached_frontmost_positive_gap.is_none()
    }

    pub fn advance_unblocked(&mut self, distance: DistanceUnits) -> StreamAdvanceReport {
        if self.items.is_empty() || distance == DistanceUnits::ZERO {
            return StreamAdvanceReport::default();
        }
        self.front_gap -= distance;
        if let Some(back_distance) = self.cached_back_item_distance_from_front.as_mut() {
            *back_distance -= distance;
        }
        self.back_gap += distance;
        self.revision += 1;
        StreamAdvanceReport::default()
    }

    pub fn advance_wrapped(
        &mut self,
        distance: DistanceUnits,
        terminal_end: DistanceUnits,
    ) -> StreamAdvanceReport {
        if self.items.is_empty()
            || distance == DistanceUnits::ZERO
            || terminal_end <= DistanceUnits::ZERO
        {
            return StreamAdvanceReport::default();
        }
        let terminal = terminal_end.raw();
        let mut positions =
            self.positions_in_range(DistanceUnits::ZERO, terminal_end - DistanceUnits::new(1));
        for position in &mut positions {
            let wrapped = (position.distance.raw() - distance.raw()).rem_euclid(terminal);
            position.distance = DistanceUnits::new(wrapped);
        }
        positions.sort_by_key(|position| position.distance);
        self.replace_positions(positions, terminal_end);
        StreamAdvanceReport::default()
    }

    pub fn advance_blocked(&mut self, distance: DistanceUnits) -> StreamAdvanceReport {
        if self.items.is_empty() || distance == DistanceUnits::ZERO {
            return StreamAdvanceReport::default();
        }
        let mut remaining = distance;
        if self.front_gap > DistanceUnits::ZERO {
            let moved = if self.front_gap >= remaining {
                remaining
            } else {
                self.front_gap
            };
            self.front_gap -= moved;
            if let Some(back_distance) = self.cached_back_item_distance_from_front.as_mut() {
                *back_distance -= moved;
            }
            self.back_gap += moved;
            remaining -= moved;
            if remaining == DistanceUnits::ZERO {
                self.revision += 1;
                return StreamAdvanceReport {
                    items_scanned: 0,
                    became_compressed: self.is_fully_compressed(),
                };
            }
        }
        let Some(index) = self.cached_frontmost_positive_gap else {
            self.revision += 1;
            return StreamAdvanceReport {
                items_scanned: 0,
                became_compressed: true,
            };
        };
        let previous_gap = self.gaps_after[index];
        let compressed_gap = previous_gap.saturating_sub(remaining);
        self.gaps_after[index] = if compressed_gap < MIN_ITEM_SPACING {
            MIN_ITEM_SPACING
        } else {
            compressed_gap
        };
        let moved = previous_gap - self.gaps_after[index];
        if let Some(back_distance) = self.cached_back_item_distance_from_front.as_mut() {
            *back_distance = back_distance.saturating_sub(moved);
        }
        self.back_gap += moved;
        if self.gaps_after[index] <= MIN_ITEM_SPACING {
            self.cached_frontmost_positive_gap =
                frontmost_compressible_gap(&self.gaps_after[index + 1..])
                    .map(|next_index| index + 1 + next_index);
        }
        self.revision += 1;
        StreamAdvanceReport {
            items_scanned: 1,
            became_compressed: self.cached_frontmost_positive_gap.is_none(),
        }
    }
}

fn frontmost_compressible_gap(gaps: &[DistanceUnits]) -> Option<usize> {
    gaps.iter()
        .enumerate()
        .find_map(|(index, gap)| (*gap > MIN_ITEM_SPACING).then_some(index))
}

fn nudged_insert_positions(
    positions: &[StreamItemPosition],
    item: ItemKindId,
    distance: DistanceUnits,
    max_distance: DistanceUnits,
) -> Option<Vec<StreamItemPosition>> {
    let mut best: Option<(i32, Vec<StreamItemPosition>)> = None;
    for insert_index in 0..=positions.len() {
        let mut candidate = positions.to_vec();
        candidate.insert(insert_index, StreamItemPosition { item, distance });

        for index in (0..insert_index).rev() {
            let max_allowed = candidate[index + 1].distance - MIN_ITEM_SPACING;
            if candidate[index].distance > max_allowed {
                candidate[index].distance = max_allowed;
            }
        }
        for index in (insert_index + 1)..candidate.len() {
            let min_allowed = candidate[index - 1].distance + MIN_ITEM_SPACING;
            if candidate[index].distance < min_allowed {
                candidate[index].distance = min_allowed;
            }
        }

        if candidate[0].distance < DistanceUnits::ZERO
            || candidate[candidate.len() - 1].distance > max_distance
        {
            continue;
        }

        let displacement = positions
            .iter()
            .zip(
                candidate
                    .iter()
                    .enumerate()
                    .filter_map(|(index, position)| (index != insert_index).then_some(position)),
            )
            .map(|(before, after)| (before.distance.raw() - after.distance.raw()).abs())
            .sum();
        if best
            .as_ref()
            .is_none_or(|(best_displacement, _)| displacement < *best_displacement)
        {
            best = Some((displacement, candidate));
        }
    }
    best.map(|(_, candidate)| candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ItemKindId;
    use crate::units::DistanceUnits;

    const IRON: ItemKindId = ItemKindId(1);
    const COPPER: ItemKindId = ItemKindId(2);

    #[test]
    fn unblocked_advance_changes_terminal_gaps_without_touching_items() {
        let mut stream = PackedItemStream::from_gaps(
            vec![IRON, COPPER],
            DistanceUnits::new(64),
            vec![DistanceUnits::new(96)],
            DistanceUnits::new(256),
        );

        let report = stream.advance_unblocked(DistanceUnits::new(16));

        assert_eq!(stream.front_gap(), DistanceUnits::new(48));
        assert_eq!(stream.back_gap(), DistanceUnits::new(272));
        assert_eq!(
            stream.distance_span_from_front(),
            Some((DistanceUnits::new(48), DistanceUnits::new(144)))
        );
        assert_eq!(stream.item_count(), 2);
        assert_eq!(report.items_scanned, 0);
        assert_eq!(stream.revision(), 1);
    }

    #[test]
    fn unblocked_advance_continues_past_zero_front_gap() {
        let mut stream = PackedItemStream::from_gaps(
            vec![IRON, COPPER],
            DistanceUnits::new(4),
            vec![DistanceUnits::new(12)],
            DistanceUnits::new(256),
        );

        let report = stream.advance_unblocked(DistanceUnits::new(8));

        assert_eq!(stream.front_gap(), DistanceUnits::new(-4));
        assert_eq!(
            stream.distance_span_from_front(),
            Some((DistanceUnits::new(-4), DistanceUnits::new(8)))
        );
        assert_eq!(stream.back_gap(), DistanceUnits::new(264));
        assert_eq!(report.items_scanned, 0);
    }

    #[test]
    fn wrapped_advance_moves_items_across_terminal_boundary() {
        let mut stream = PackedItemStream::from_gaps(
            vec![IRON],
            DistanceUnits::new(4),
            vec![],
            DistanceUnits::new(252),
        );

        stream.advance_wrapped(DistanceUnits::new(8), DistanceUnits::new(256));

        assert_eq!(
            stream.positions_in_range(DistanceUnits::ZERO, DistanceUnits::new(255)),
            vec![StreamItemPosition {
                item: IRON,
                distance: DistanceUnits::new(252),
            }]
        );
    }

    #[test]
    fn blocked_advance_consumes_front_gap_before_internal_gaps() {
        let mut stream = PackedItemStream::from_gaps(
            vec![IRON, COPPER],
            DistanceUnits::new(16),
            vec![DistanceUnits::new(32)],
            DistanceUnits::new(80),
        );

        let report = stream.advance_blocked(DistanceUnits::new(8));

        assert_eq!(stream.front_gap(), DistanceUnits::new(8));
        assert_eq!(stream.gap_after(0), DistanceUnits::new(32));
        assert_eq!(
            stream.distance_span_from_front(),
            Some((DistanceUnits::new(8), DistanceUnits::new(40)))
        );
        assert_eq!(report.items_scanned, 0);
        assert!(!report.became_compressed);
    }

    #[test]
    fn blocked_advance_after_front_gap_updates_revision_without_internal_gaps() {
        let mut stream = PackedItemStream::from_gaps(
            vec![IRON],
            DistanceUnits::new(4),
            vec![],
            DistanceUnits::new(80),
        );

        let report = stream.advance_blocked(DistanceUnits::new(8));

        assert_eq!(stream.front_gap(), DistanceUnits::ZERO);
        assert_eq!(stream.revision(), 1);
        assert!(report.became_compressed);
    }

    #[test]
    fn blocked_advance_compresses_frontmost_positive_gap_amortized() {
        let mut stream = PackedItemStream::from_gaps(
            vec![IRON, COPPER, IRON],
            DistanceUnits::ZERO,
            vec![DistanceUnits::new(64), DistanceUnits::new(96)],
            DistanceUnits::new(80),
        );

        let first = stream.advance_blocked(DistanceUnits::new(5));
        let second = stream.advance_blocked(DistanceUnits::new(5));

        assert_eq!(stream.gap_after(1), DistanceUnits::new(86));
        assert_eq!(
            stream.distance_span_from_front(),
            Some((DistanceUnits::ZERO, DistanceUnits::new(150)))
        );
        assert_eq!(stream.back_gap(), DistanceUnits::new(90));
        assert_eq!(stream.cached_frontmost_positive_gap(), Some(1));
        assert_eq!(first.items_scanned, 1);
        assert_eq!(second.items_scanned, 1);
    }

    #[test]
    fn blocked_advance_compresses_frontmost_positive_gap_first() {
        let mut stream = PackedItemStream::from_gaps(
            vec![IRON, COPPER, IRON],
            DistanceUnits::ZERO,
            vec![DistanceUnits::new(96), DistanceUnits::new(96)],
            DistanceUnits::new(80),
        );

        stream.advance_blocked(DistanceUnits::new(8));

        assert_eq!(stream.gap_after(0), DistanceUnits::new(88));
        assert_eq!(stream.gap_after(1), DistanceUnits::new(96));
        assert_eq!(
            stream.distance_span_from_front(),
            Some((DistanceUnits::ZERO, DistanceUnits::new(184)))
        );
    }

    #[test]
    fn blocked_advance_preserves_minimum_item_spacing() {
        let mut stream = PackedItemStream::from_gaps(
            vec![IRON, COPPER, IRON],
            DistanceUnits::ZERO,
            vec![DistanceUnits::new(64), DistanceUnits::new(96)],
            DistanceUnits::new(80),
        );

        stream.advance_blocked(DistanceUnits::new(64));

        assert_eq!(
            stream.positions_in_range(DistanceUnits::ZERO, DistanceUnits::new(128)),
            vec![
                StreamItemPosition {
                    item: IRON,
                    distance: DistanceUnits::ZERO,
                },
                StreamItemPosition {
                    item: COPPER,
                    distance: DistanceUnits::new(64),
                },
                StreamItemPosition {
                    item: IRON,
                    distance: DistanceUnits::new(128),
                },
            ]
        );
        assert!(stream.is_fully_compressed());
    }

    #[test]
    fn blocked_stream_sleeps_when_no_positive_gaps_remain() {
        let mut stream = PackedItemStream::from_gaps(
            vec![IRON, COPPER],
            DistanceUnits::ZERO,
            vec![DistanceUnits::new(16)],
            DistanceUnits::new(0),
        );

        let report = stream.advance_blocked(DistanceUnits::new(8));

        assert_eq!(stream.gap_after(0), DistanceUnits::new(16));
        assert!(stream.is_fully_compressed());
        assert!(report.became_compressed);
    }

    #[test]
    fn empty_stream_has_no_distance_span() {
        let stream = PackedItemStream::default();

        assert_eq!(stream.distance_span_from_front(), None);
    }

    #[test]
    fn positions_in_range_returns_exact_item_distances() {
        let stream = PackedItemStream::from_gaps(
            vec![IRON, COPPER, IRON],
            DistanceUnits::new(16),
            vec![DistanceUnits::new(32), DistanceUnits::new(48)],
            DistanceUnits::new(256),
        );

        assert_eq!(
            stream.positions_in_range(DistanceUnits::new(16), DistanceUnits::new(48)),
            vec![
                StreamItemPosition {
                    item: IRON,
                    distance: DistanceUnits::new(16),
                },
                StreamItemPosition {
                    item: COPPER,
                    distance: DistanceUnits::new(48),
                },
            ]
        );
    }

    #[test]
    fn positions_in_offscreen_range_returns_empty() {
        let stream = PackedItemStream::from_gaps(
            vec![IRON, COPPER],
            DistanceUnits::new(64),
            vec![DistanceUnits::new(64)],
            DistanceUnits::new(256),
        );

        let before_span =
            stream.positions_in_range_with_report(DistanceUnits::new(0), DistanceUnits::new(32));
        let after_span =
            stream.positions_in_range_with_report(DistanceUnits::new(160), DistanceUnits::new(256));

        assert_eq!(before_span.positions, Vec::new());
        assert_eq!(before_span.items_scanned, 0);
        assert_eq!(after_span.positions, Vec::new());
        assert_eq!(after_span.items_scanned, 0);
    }

    #[test]
    fn range_query_reports_items_scanned_before_start() {
        let stream = PackedItemStream::from_gaps(
            vec![IRON, COPPER, IRON, COPPER],
            DistanceUnits::new(16),
            vec![
                DistanceUnits::new(16),
                DistanceUnits::new(16),
                DistanceUnits::new(16),
            ],
            DistanceUnits::new(256),
        );

        let query =
            stream.positions_in_range_with_report(DistanceUnits::new(48), DistanceUnits::new(48));

        assert_eq!(
            query.positions,
            vec![StreamItemPosition {
                item: IRON,
                distance: DistanceUnits::new(48),
            }]
        );
        assert_eq!(query.items_scanned, 4);
    }

    #[test]
    fn positions_remain_coherent_after_unblocked_and_blocked_advance() {
        let mut unblocked = PackedItemStream::from_gaps(
            vec![IRON, COPPER],
            DistanceUnits::new(64),
            vec![DistanceUnits::new(96)],
            DistanceUnits::new(256),
        );
        unblocked.advance_unblocked(DistanceUnits::new(16));

        assert_eq!(
            unblocked.distance_span_from_front(),
            Some((DistanceUnits::new(48), DistanceUnits::new(144)))
        );
        assert_eq!(
            unblocked.positions_in_range(DistanceUnits::new(0), DistanceUnits::new(160)),
            vec![
                StreamItemPosition {
                    item: IRON,
                    distance: DistanceUnits::new(48),
                },
                StreamItemPosition {
                    item: COPPER,
                    distance: DistanceUnits::new(144),
                },
            ]
        );

        let mut blocked = PackedItemStream::from_gaps(
            vec![IRON, COPPER, IRON],
            DistanceUnits::ZERO,
            vec![DistanceUnits::new(64), DistanceUnits::new(96)],
            DistanceUnits::new(80),
        );
        blocked.advance_blocked(DistanceUnits::new(5));

        assert_eq!(
            blocked.distance_span_from_front(),
            Some((DistanceUnits::ZERO, DistanceUnits::new(155)))
        );
        assert_eq!(
            blocked.positions_in_range(DistanceUnits::ZERO, DistanceUnits::new(155)),
            vec![
                StreamItemPosition {
                    item: IRON,
                    distance: DistanceUnits::ZERO,
                },
                StreamItemPosition {
                    item: COPPER,
                    distance: DistanceUnits::new(64),
                },
                StreamItemPosition {
                    item: IRON,
                    distance: DistanceUnits::new(155),
                },
            ]
        );
    }

    #[test]
    fn removing_back_item_preserves_terminal_back_gap() {
        let mut stream = PackedItemStream::from_gaps(
            vec![IRON, COPPER],
            DistanceUnits::new(64),
            vec![DistanceUnits::new(96)],
            DistanceUnits::new(256),
        );

        assert_eq!(
            stream.remove_one_at_distance(DistanceUnits::new(160)),
            Some(COPPER)
        );

        assert_eq!(stream.front_gap(), DistanceUnits::new(64));
        assert_eq!(stream.back_gap(), DistanceUnits::new(352));
        assert_eq!(
            stream.distance_span_from_front(),
            Some((DistanceUnits::new(64), DistanceUnits::new(64)))
        );
        assert_eq!(
            stream.positions_in_range(DistanceUnits::ZERO, DistanceUnits::new(512)),
            vec![StreamItemPosition {
                item: IRON,
                distance: DistanceUnits::new(64),
            }]
        );
    }

    #[test]
    fn inserting_behind_current_back_item_preserves_terminal_back_gap() {
        let mut stream = PackedItemStream::from_gaps(
            vec![IRON],
            DistanceUnits::new(64),
            vec![],
            DistanceUnits::new(256),
        );

        assert!(stream.insert_one_at_distance_with_terminal_end(
            COPPER,
            DistanceUnits::new(200),
            None,
        ));

        assert_eq!(stream.front_gap(), DistanceUnits::new(64));
        assert_eq!(stream.gap_after(0), DistanceUnits::new(136));
        assert_eq!(stream.back_gap(), DistanceUnits::new(120));
        assert_eq!(
            stream.distance_span_from_front(),
            Some((DistanceUnits::new(64), DistanceUnits::new(200)))
        );
        assert_eq!(
            stream.positions_in_range(DistanceUnits::ZERO, DistanceUnits::new(256)),
            vec![
                StreamItemPosition {
                    item: IRON,
                    distance: DistanceUnits::new(64),
                },
                StreamItemPosition {
                    item: COPPER,
                    distance: DistanceUnits::new(200),
                },
            ]
        );
    }

    #[test]
    fn nudged_insert_keeps_new_item_at_requested_distance_when_space_exists() {
        let mut stream = PackedItemStream::from_gaps(
            vec![IRON, IRON, IRON],
            DistanceUnits::new(64),
            vec![DistanceUnits::new(64), DistanceUnits::new(64)],
            DistanceUnits::new(64),
        );

        assert!(stream.insert_one_with_nudge_at_distance_with_terminal_end(
            COPPER,
            DistanceUnits::new(128),
            Some(DistanceUnits::new(256)),
        ));

        assert_eq!(
            stream.positions_in_range(DistanceUnits::ZERO, DistanceUnits::new(255)),
            vec![
                StreamItemPosition {
                    item: IRON,
                    distance: DistanceUnits::ZERO,
                },
                StreamItemPosition {
                    item: IRON,
                    distance: DistanceUnits::new(64),
                },
                StreamItemPosition {
                    item: COPPER,
                    distance: DistanceUnits::new(128),
                },
                StreamItemPosition {
                    item: IRON,
                    distance: DistanceUnits::new(192),
                },
            ]
        );
    }

    #[test]
    fn empty_stream_insert_preserves_terminal_end_and_allows_later_valid_insert() {
        let mut stream = PackedItemStream::from_gaps(
            Vec::new(),
            DistanceUnits::ZERO,
            Vec::new(),
            DistanceUnits::new(320),
        );

        assert!(stream.insert_one_at_distance_with_terminal_end(
            IRON,
            DistanceUnits::new(64),
            None,
        ));

        assert_eq!(stream.front_gap(), DistanceUnits::new(64));
        assert_eq!(stream.back_gap(), DistanceUnits::new(256));
        assert_eq!(
            stream.distance_span_from_front(),
            Some((DistanceUnits::new(64), DistanceUnits::new(64)))
        );

        assert!(stream.insert_one_at_distance_with_terminal_end(
            COPPER,
            DistanceUnits::new(200),
            None,
        ));

        assert_eq!(stream.front_gap(), DistanceUnits::new(64));
        assert_eq!(stream.gap_after(0), DistanceUnits::new(136));
        assert_eq!(stream.back_gap(), DistanceUnits::new(120));
        assert_eq!(
            stream.distance_span_from_front(),
            Some((DistanceUnits::new(64), DistanceUnits::new(200)))
        );
        assert_eq!(
            stream.positions_in_range(DistanceUnits::ZERO, DistanceUnits::new(320)),
            vec![
                StreamItemPosition {
                    item: IRON,
                    distance: DistanceUnits::new(64),
                },
                StreamItemPosition {
                    item: COPPER,
                    distance: DistanceUnits::new(200),
                },
            ]
        );
    }
}
