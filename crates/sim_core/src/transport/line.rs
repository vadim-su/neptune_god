//! One transport line: geometry, endpoints, and a packed item stream along distance.

use serde::{Deserialize, Serialize};

use crate::ids::{GroupId, ItemKindId, LineId, TilePos};
use crate::transport::stream::{
    PackedItemStream, PackedItemStreamSnapshot, StreamAdvanceReport, StreamItemPosition,
    StreamRangeQuery,
};
use crate::units::{DistanceUnits, UnitsPerTick};

/// Inclusive max distance from the line front used for transfers and pop_front.
pub const FRONT_WINDOW_LEN: DistanceUnits = DistanceUnits::new(31);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum LineEndpoint {
    Open,
    Blocked,
    Closed,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum LineTileKind {
    #[default]
    Surface,
    Underground,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct LineTile {
    pub pos: TilePos,
    #[serde(default)]
    pub kind: LineTileKind,
}
impl LineTile {
    pub const fn surface(pos: TilePos) -> Self {
        Self {
            pos,
            kind: LineTileKind::Surface,
        }
    }

    pub const fn underground(pos: TilePos) -> Self {
        Self {
            pos,
            kind: LineTileKind::Underground,
        }
    }

    pub const fn new(pos: TilePos) -> Self {
        Self::surface(pos)
    }

    pub const fn is_surface(self) -> bool {
        matches!(self.kind, LineTileKind::Surface)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinePath {
    tiles: Vec<LineTile>,
    total_len: DistanceUnits,
}
impl LinePath {
    pub fn new(tiles: Vec<LineTile>) -> Self {
        let total_len = DistanceUnits::from_tiles(tiles.len() as i32);
        Self { tiles, total_len }
    }
    pub fn tiles(&self) -> &[LineTile] {
        &self.tiles
    }
    pub fn total_len(&self) -> DistanceUnits {
        self.total_len
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineAdvanceReport {
    pub items_scanned: usize,
    pub became_compressed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Runtime belt line: path geometry, endpoints, speed, and item stream.
pub struct TransportLine {
    id: LineId,
    group_id: GroupId,
    path: LinePath,
    lanes: [PackedItemStream; 2],
    speed: UnitsPerTick,
    front: LineEndpoint,
    back: LineEndpoint,
    blocked_front: bool,
    sleeping: bool,
    revision: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct TransportLineSnapshot {
    pub id: LineId,
    pub group_id: GroupId,
    pub path: Vec<LineTile>,
    pub lanes: [PackedItemStreamSnapshot; 2],
    pub speed: UnitsPerTick,
    pub front: LineEndpoint,
    pub back: LineEndpoint,
    pub blocked_front: bool,
    pub sleeping: bool,
    pub revision: u64,
}

impl TransportLine {
    pub fn new(
        id: LineId,
        group_id: GroupId,
        path: LinePath,
        speed: UnitsPerTick,
        lanes: [PackedItemStream; 2],
        front: LineEndpoint,
        back: LineEndpoint,
    ) -> Self {
        Self {
            id,
            group_id,
            path,
            lanes,
            speed,
            front,
            back,
            blocked_front: front == LineEndpoint::Blocked,
            sleeping: false,
            revision: 0,
        }
    }
    pub fn id(&self) -> LineId {
        self.id
    }
    pub fn group_id(&self) -> GroupId {
        self.group_id
    }
    pub fn path(&self) -> &LinePath {
        &self.path
    }
    pub fn speed(&self) -> UnitsPerTick {
        self.speed
    }
    pub fn lane(&self, lane: usize) -> &PackedItemStream {
        &self.lanes[lane]
    }
    pub fn lane_mut(&mut self, lane: usize) -> &mut PackedItemStream {
        &mut self.lanes[lane]
    }
    pub fn lane_positions_in_range(
        &self,
        lane: usize,
        start: DistanceUnits,
        end: DistanceUnits,
    ) -> Vec<StreamItemPosition> {
        self.lanes[lane].positions_in_range(start, end)
    }
    pub fn lane_positions_in_range_with_report(
        &self,
        lane: usize,
        start: DistanceUnits,
        end: DistanceUnits,
    ) -> StreamRangeQuery {
        self.lanes[lane].positions_in_range_with_report(start, end)
    }
    pub fn set_front_endpoint(&mut self, front: LineEndpoint) {
        self.front = front;
        self.blocked_front = front == LineEndpoint::Blocked;
        self.sleeping = false;
        self.revision += 1;
    }
    pub fn front_window(&self) -> (DistanceUnits, DistanceUnits) {
        (DistanceUnits::ZERO, FRONT_WINDOW_LEN)
    }
    pub fn entry_boundary_insert_distance(&self) -> DistanceUnits {
        self.path.total_len() - DistanceUnits::new(1)
    }
    pub fn pop_front_item(&mut self, lane: usize) -> Option<ItemKindId> {
        let (min_distance, max_distance) = self.front_window();
        let position = self.first_in_window(lane, min_distance, max_distance)?;
        self.remove_one_at_distance(lane, position.distance)
    }
    pub fn insert_item_at_entry_boundary(&mut self, lane: usize, item: ItemKindId) -> bool {
        self.insert_one_in_window(lane, item, self.entry_boundary_insert_distance())
    }
    pub fn take_first_in_window(
        &mut self,
        lane: usize,
        min_distance: DistanceUnits,
        max_distance: DistanceUnits,
    ) -> Option<ItemKindId> {
        let first = self
            .lane(lane)
            .positions_in_range(min_distance, max_distance)
            .into_iter()
            .min_by_key(|position| position.distance)?;
        let item = self.lane_mut(lane).remove_one_at_distance(first.distance)?;
        self.sleeping = self.lanes.iter().all(PackedItemStream::is_empty);
        self.revision += 1;
        Some(item)
    }
    pub fn first_in_window(
        &self,
        lane: usize,
        min_distance: DistanceUnits,
        max_distance: DistanceUnits,
    ) -> Option<StreamItemPosition> {
        self.lane(lane)
            .positions_in_range(min_distance, max_distance)
            .into_iter()
            .min_by_key(|position| position.distance)
    }
    pub fn remove_one_at_distance(
        &mut self,
        lane: usize,
        distance: DistanceUnits,
    ) -> Option<ItemKindId> {
        let item = self.lane_mut(lane).remove_one_at_distance(distance)?;
        self.sleeping = self.lanes.iter().all(PackedItemStream::is_empty);
        self.revision += 1;
        Some(item)
    }
    pub fn insert_one_in_window(
        &mut self,
        lane: usize,
        item: ItemKindId,
        distance: DistanceUnits,
    ) -> bool {
        let terminal_end = self.path.total_len();
        let inserted = self
            .lane_mut(lane)
            .insert_one_at_distance_with_terminal_end(item, distance, Some(terminal_end));
        if inserted {
            self.sleeping = false;
            self.revision += 1;
        }
        inserted
    }
    pub fn insert_one_with_nudge_in_window(
        &mut self,
        lane: usize,
        item: ItemKindId,
        distance: DistanceUnits,
    ) -> bool {
        let terminal_end = self.path.total_len();
        let inserted = self
            .lane_mut(lane)
            .insert_one_with_nudge_at_distance_with_terminal_end(
                item,
                distance,
                Some(terminal_end),
            );
        if inserted {
            self.sleeping = false;
            self.revision += 1;
        }
        inserted
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn sleeping(&self) -> bool {
        self.sleeping
    }
    pub fn closed(&self) -> bool {
        self.front == LineEndpoint::Closed && self.back == LineEndpoint::Closed
    }

    pub fn snapshot(&self) -> TransportLineSnapshot {
        TransportLineSnapshot {
            id: self.id,
            group_id: self.group_id,
            path: self.path.tiles.clone(),
            lanes: [self.lanes[0].snapshot(), self.lanes[1].snapshot()],
            speed: self.speed,
            front: self.front,
            back: self.back,
            blocked_front: self.blocked_front,
            sleeping: self.sleeping,
            revision: self.revision,
        }
    }

    pub fn from_snapshot(snapshot: TransportLineSnapshot) -> Result<Self, String> {
        let [left_lane, right_lane] = snapshot.lanes;
        let lanes = [
            PackedItemStream::from_snapshot(left_lane)
                .map_err(|error| format!("line {:?} lane 0: {error}", snapshot.id))?,
            PackedItemStream::from_snapshot(right_lane)
                .map_err(|error| format!("line {:?} lane 1: {error}", snapshot.id))?,
        ];

        Ok(Self {
            id: snapshot.id,
            group_id: snapshot.group_id,
            path: LinePath::new(snapshot.path),
            lanes,
            speed: snapshot.speed,
            front: snapshot.front,
            back: snapshot.back,
            blocked_front: snapshot.blocked_front,
            sleeping: snapshot.sleeping,
            revision: snapshot.revision,
        })
    }

    pub fn advance(&mut self) -> LineAdvanceReport {
        if self.sleeping {
            return LineAdvanceReport::default();
        }
        if self.lanes.iter().all(PackedItemStream::is_empty) {
            self.sleeping = true;
            return LineAdvanceReport::default();
        }
        let distance = self.speed.distance_per_tick();
        let mut report = LineAdvanceReport::default();
        for lane in &mut self.lanes {
            let lane_report: StreamAdvanceReport = if self.front == LineEndpoint::Closed {
                lane.advance_wrapped(distance, self.path.total_len())
            } else if self.blocked_front {
                lane.advance_blocked(distance)
            } else {
                lane.advance_unblocked(distance)
            };
            report.items_scanned += lane_report.items_scanned;
            report.became_compressed |= lane_report.became_compressed;
        }
        if self.blocked_front && self.lanes.iter().all(PackedItemStream::is_fully_compressed) {
            self.sleeping = true;
        }
        self.revision += 1;
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{GroupId, ItemKindId, LineId, TilePos};

    const IRON: ItemKindId = ItemKindId(1);

    fn one_tile_line(front: LineEndpoint, lane: usize) -> TransportLine {
        let mut lanes = [PackedItemStream::default(), PackedItemStream::default()];
        lanes[lane] = PackedItemStream::from_gaps(
            vec![IRON],
            DistanceUnits::ZERO,
            vec![],
            DistanceUnits::new(128),
        );
        TransportLine::new(
            LineId(1),
            GroupId(1),
            LinePath::new(vec![LineTile::new(TilePos::new(0, 0))]),
            UnitsPerTick::new(8),
            lanes,
            front,
            LineEndpoint::Open,
        )
    }

    #[test]
    fn line_endpoint_variants_are_open_blocked_closed() {
        fn label(endpoint: LineEndpoint) -> &'static str {
            match endpoint {
                LineEndpoint::Open => "Open",
                LineEndpoint::Blocked => "Blocked",
                LineEndpoint::Closed => "Closed",
            }
        }

        assert_eq!(label(LineEndpoint::Open), "Open");
        assert_eq!(label(LineEndpoint::Blocked), "Blocked");
        assert_eq!(label(LineEndpoint::Closed), "Closed");
    }

    #[test]
    fn front_window_uses_named_constant() {
        assert_eq!(FRONT_WINDOW_LEN, DistanceUnits::new(31));
        let line = one_tile_line(LineEndpoint::Blocked, 0);
        assert_eq!(line.front_window(), (DistanceUnits::ZERO, FRONT_WINDOW_LEN));
    }

    #[test]
    fn pop_front_item_removes_item_at_front_distance() {
        let mut line = one_tile_line(LineEndpoint::Blocked, 0);

        let item = line.pop_front_item(0);

        assert_eq!(item, Some(IRON));
        assert_eq!(line.lane(0).item_count(), 0);
        assert_eq!(line.revision(), 1);
    }
}
