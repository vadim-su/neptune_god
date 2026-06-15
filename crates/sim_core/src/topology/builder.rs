//! Incremental topology updates when belts and underground segments are placed.

use std::collections::BTreeSet;

use crate::ids::TilePos;
use crate::topology::graph::{TopologyGraph, UndergroundEndpointRole};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltPathTileKind {
    Surface,
    Underground,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltPathTile {
    pub pos: TilePos,
    pub kind: BuiltPathTileKind,
}

impl BuiltPathTile {
    pub const fn surface(pos: TilePos) -> Self {
        Self {
            pos,
            kind: BuiltPathTileKind::Surface,
        }
    }

    pub const fn underground(pos: TilePos) -> Self {
        Self {
            pos,
            kind: BuiltPathTileKind::Underground,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltLine {
    pub tiles: Vec<BuiltPathTile>,
    pub closed: bool,
    pub front_output: Option<TilePos>,
}

impl BuiltLine {
    pub fn surface_positions(&self) -> Vec<TilePos> {
        self.tiles
            .iter()
            .filter(|tile| tile.kind == BuiltPathTileKind::Surface)
            .map(|tile| tile.pos)
            .collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuiltTopology {
    pub lines: Vec<BuiltLine>,
    pub source_revision: u64,
}

#[derive(Clone, Debug, Default)]
pub struct TopologyBuilder;

impl TopologyBuilder {
    pub fn rebuild(&self, graph: &TopologyGraph) -> BuiltTopology {
        let mut visited = BTreeSet::new();
        let mut lines = Vec::new();

        for (pos, _) in graph.belts_sorted() {
            if visited.contains(&pos) || line_predecessor_count(graph, pos) > 0 {
                continue;
            }

            let tiles = follow_line(graph, pos, &mut visited);
            if !tiles.is_empty() {
                let closed = is_closed_line(graph, &tiles);
                let front_output = line_front_output(graph, &tiles, closed);
                lines.push(BuiltLine {
                    tiles,
                    closed,
                    front_output,
                });
            }
        }

        for (pos, _) in graph.belts_sorted() {
            if visited.contains(&pos) {
                continue;
            }
            let tiles = follow_line(graph, pos, &mut visited);
            if !tiles.is_empty() {
                let closed = is_closed_line(graph, &tiles);
                let front_output = line_front_output(graph, &tiles, closed);
                lines.push(BuiltLine {
                    tiles,
                    closed,
                    front_output,
                });
            }
        }

        lines.sort_by_key(|line| {
            let first = line
                .surface_positions()
                .first()
                .copied()
                .unwrap_or(line.tiles[0].pos);
            (first.y, first.x)
        });

        BuiltTopology {
            lines,
            source_revision: graph.revision(),
        }
    }
}

fn follow_line(
    graph: &TopologyGraph,
    start: TilePos,
    visited: &mut BTreeSet<TilePos>,
) -> Vec<BuiltPathTile> {
    let mut tiles = Vec::new();
    let mut current = start;
    while let Some(current_belt) = graph.belt(current) {
        if visited.contains(&current) {
            break;
        }

        visited.insert(current);
        tiles.push(BuiltPathTile::surface(current));

        if let Some(link) = graph.underground_link(current)
            && matches!(
                link.role,
                UndergroundEndpointRole::Entrance | UndergroundEndpointRole::Exit
            )
        {
            let _ = link;
            break;
        }

        let next = current_belt.direction.output_pos(current);
        if should_stop_before_next(graph, current, next, visited) {
            break;
        }
        current = next;
    }
    tiles
}

fn line_predecessor_count(graph: &TopologyGraph, pos: TilePos) -> usize {
    let Some(target_belt) = graph.belt(pos) else {
        return 0;
    };

    graph
        .belts_sorted()
        .filter(|(candidate, belt)| {
            belt.direction.output_pos(*candidate) == pos
                && belt.direction == target_belt.input_direction
                && belt.surface_z == target_belt.surface_z
        })
        .count()
}

fn should_stop_before_next(
    graph: &TopologyGraph,
    current: TilePos,
    next: TilePos,
    _visited: &BTreeSet<TilePos>,
) -> bool {
    let Some(current_belt) = graph.belt(current) else {
        return true;
    };
    let Some(next_belt) = graph.belt(next) else {
        return false;
    };

    if next_belt.surface_z != current_belt.surface_z {
        return true;
    }

    if next_belt.direction.output_pos(next) == current {
        return true;
    }

    next_belt.input_direction != current_belt.direction
}

fn is_closed_line(graph: &TopologyGraph, tiles: &[BuiltPathTile]) -> bool {
    let surfaces = tiles
        .iter()
        .filter(|tile| tile.kind == BuiltPathTileKind::Surface)
        .map(|tile| tile.pos)
        .collect::<Vec<_>>();
    if surfaces.len() < 3 {
        return false;
    }
    let Some(first) = surfaces.first() else {
        return false;
    };
    let Some(last) = surfaces.last() else {
        return false;
    };
    let Some(first_belt) = graph.belt(*first) else {
        return false;
    };
    let Some(last_belt) = graph.belt(*last) else {
        return false;
    };
    if first_belt.surface_z != last_belt.surface_z {
        return false;
    }
    last_belt.direction.output_pos(*last) == *first
        && first_belt.input_direction == last_belt.direction
}

fn line_front_output(
    graph: &TopologyGraph,
    tiles: &[BuiltPathTile],
    closed: bool,
) -> Option<TilePos> {
    if closed {
        return None;
    }
    let last = tiles
        .iter()
        .rev()
        .find(|tile| tile.kind == BuiltPathTileKind::Surface)
        .map(|tile| tile.pos)?;
    let belt = graph.belt(last)?;
    Some(belt.direction.output_pos(last))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::TilePos;
    use crate::topology::graph::{BeltTile, Direction, TopologyGraph};

    #[test]
    fn builds_one_line_from_contiguous_east_belts() {
        let mut graph = TopologyGraph::default();
        graph.set_belt(TilePos::new(0, 0), BeltTile::new(Direction::East));
        graph.set_belt(TilePos::new(1, 0), BeltTile::new(Direction::East));
        graph.set_belt(TilePos::new(2, 0), BeltTile::new(Direction::East));

        let topology = TopologyBuilder.rebuild(&graph);

        assert_eq!(topology.lines.len(), 1);
        assert!(!topology.lines[0].closed);
        assert_eq!(
            topology.lines[0].surface_positions(),
            vec![TilePos::new(0, 0), TilePos::new(1, 0), TilePos::new(2, 0)]
        );
    }

    #[test]
    fn break_in_belts_creates_two_lines() {
        let mut graph = TopologyGraph::default();
        graph.set_belt(TilePos::new(0, 0), BeltTile::new(Direction::East));
        graph.set_belt(TilePos::new(2, 0), BeltTile::new(Direction::East));

        let topology = TopologyBuilder.rebuild(&graph);

        assert_eq!(topology.lines.len(), 2);
        assert_eq!(
            topology.lines[0].surface_positions(),
            vec![TilePos::new(0, 0)]
        );
        assert_eq!(
            topology.lines[1].surface_positions(),
            vec![TilePos::new(2, 0)]
        );
    }

    #[test]
    fn adjacent_belts_on_different_surface_levels_are_separate_lines() {
        let mut graph = TopologyGraph::default();
        graph.set_belt(TilePos::new(0, 0), BeltTile::new(Direction::East));
        graph.set_belt(
            TilePos::new(1, 0),
            BeltTile::new(Direction::East).on_surface(1),
        );

        let topology = TopologyBuilder.rebuild(&graph);

        assert_eq!(topology.lines.len(), 2);
        assert_eq!(
            topology.lines[0].surface_positions(),
            vec![TilePos::new(0, 0)]
        );
        assert_eq!(
            topology.lines[1].surface_positions(),
            vec![TilePos::new(1, 0)]
        );
    }

    #[test]
    fn connected_turn_belts_stay_in_one_transport_line() {
        let mut graph = TopologyGraph::default();
        graph.set_belt(TilePos::new(0, 0), BeltTile::new(Direction::East));
        graph.set_belt(TilePos::new(1, 0), BeltTile::new(Direction::East));
        graph.set_belt(
            TilePos::new(2, 0),
            BeltTile::turn(Direction::East, Direction::South),
        );
        graph.set_belt(TilePos::new(2, -1), BeltTile::new(Direction::South));

        let topology = TopologyBuilder.rebuild(&graph);

        assert_eq!(topology.lines.len(), 1);
        assert!(!topology.lines[0].closed);
        assert_eq!(
            topology.lines[0].surface_positions(),
            vec![
                TilePos::new(0, 0),
                TilePos::new(1, 0),
                TilePos::new(2, 0),
                TilePos::new(2, -1),
            ]
        );
    }

    #[test]
    fn same_geometry_without_explicit_turn_stays_separate() {
        let mut graph = TopologyGraph::default();
        graph.set_belt(TilePos::new(0, 0), BeltTile::new(Direction::East));
        graph.set_belt(TilePos::new(1, 0), BeltTile::new(Direction::East));
        graph.set_belt(TilePos::new(2, 0), BeltTile::new(Direction::South));
        graph.set_belt(TilePos::new(2, -1), BeltTile::new(Direction::South));

        let topology = TopologyBuilder.rebuild(&graph);

        assert_eq!(topology.lines.len(), 2, "{:?}", topology.lines);
        assert!(topology.lines.iter().any(|line| {
            line.surface_positions() == vec![TilePos::new(0, 0), TilePos::new(1, 0)]
                && line.front_output == Some(TilePos::new(2, 0))
        }));
        assert!(topology.lines.iter().any(|line| {
            line.surface_positions() == vec![TilePos::new(2, 0), TilePos::new(2, -1)]
        }));
    }

    #[test]
    fn multi_tile_side_input_to_target_first_tile_stays_separate() {
        let mut graph = TopologyGraph::default();
        graph.set_belt(TilePos::new(-2, 0), BeltTile::new(Direction::East));
        graph.set_belt(TilePos::new(-1, 0), BeltTile::new(Direction::East));
        graph.set_belt(TilePos::new(0, 0), BeltTile::new(Direction::North));
        graph.set_belt(TilePos::new(0, 1), BeltTile::new(Direction::North));

        let topology = TopologyBuilder.rebuild(&graph);

        assert_eq!(topology.lines.len(), 2, "{:?}", topology.lines);
        assert!(topology.lines.iter().any(|line| {
            line.surface_positions() == vec![TilePos::new(-2, 0), TilePos::new(-1, 0)]
                && line.front_output == Some(TilePos::new(0, 0))
        }));
        assert!(topology.lines.iter().any(|line| {
            line.surface_positions() == vec![TilePos::new(0, 0), TilePos::new(0, 1)]
        }));
    }

    #[test]
    fn rectangular_cycle_is_one_closed_transport_line() {
        let mut graph = TopologyGraph::default();
        for (pos, belt) in rectangular_cycle() {
            graph.set_belt(pos, belt);
        }

        let topology = TopologyBuilder.rebuild(&graph);

        assert_eq!(topology.lines.len(), 1);
        assert!(topology.lines[0].closed);
        assert_eq!(
            topology.lines[0].surface_positions(),
            vec![
                TilePos::new(0, -2),
                TilePos::new(0, -1),
                TilePos::new(0, 0),
                TilePos::new(1, 0),
                TilePos::new(2, 0),
                TilePos::new(2, -1),
                TilePos::new(2, -2),
                TilePos::new(1, -2),
            ]
        );
    }

    #[test]
    fn merge_into_existing_belt_starts_a_separate_transport_line() {
        let mut graph = TopologyGraph::default();
        graph.set_belt(TilePos::new(0, 0), BeltTile::new(Direction::East));
        graph.set_belt(TilePos::new(1, 0), BeltTile::new(Direction::East));
        graph.set_belt(TilePos::new(1, 1), BeltTile::new(Direction::South));

        let topology = TopologyBuilder.rebuild(&graph);

        assert_eq!(topology.lines.len(), 2);
        assert!(
            topology
                .lines
                .iter()
                .any(|line| line.surface_positions()
                    == vec![TilePos::new(0, 0), TilePos::new(1, 0),])
        );
        assert!(
            topology
                .lines
                .iter()
                .any(|line| line.surface_positions() == vec![TilePos::new(1, 1)])
        );
    }

    #[test]
    fn opposing_belts_are_two_open_lines_not_one_closed_loop() {
        let mut graph = TopologyGraph::default();
        graph.set_belt(TilePos::new(0, 0), BeltTile::new(Direction::East));
        graph.set_belt(TilePos::new(1, 0), BeltTile::new(Direction::West));

        let topology = TopologyBuilder.rebuild(&graph);

        assert_eq!(topology.lines.len(), 2);
        assert!(topology.lines.iter().all(|line| !line.closed));
        assert!(
            topology
                .lines
                .iter()
                .any(|line| line.surface_positions() == vec![TilePos::new(0, 0)])
        );
        assert!(
            topology
                .lines
                .iter()
                .any(|line| line.surface_positions() == vec![TilePos::new(1, 0)])
        );
    }

    #[test]
    fn t_junction_keeps_target_line_straight() {
        let mut graph = TopologyGraph::default();
        graph.set_belt(TilePos::new(0, -1), BeltTile::new(Direction::North));
        graph.set_belt(TilePos::new(0, 0), BeltTile::new(Direction::North));
        graph.set_belt(TilePos::new(0, 1), BeltTile::new(Direction::North));
        graph.set_belt(TilePos::new(-1, 0), BeltTile::new(Direction::East));
        graph.set_belt(TilePos::new(1, 0), BeltTile::new(Direction::West));

        let topology = TopologyBuilder.rebuild(&graph);

        assert_eq!(topology.lines.len(), 3);
        assert!(
            topology.lines.iter().any(|line| {
                line.surface_positions()
                    == vec![TilePos::new(0, -1), TilePos::new(0, 0), TilePos::new(0, 1)]
            }),
            "vertical target line should remain straight: {:?}",
            topology.lines
        );
        assert!(
            topology
                .lines
                .iter()
                .any(|line| line.surface_positions() == vec![TilePos::new(-1, 0)])
        );
        assert!(
            topology
                .lines
                .iter()
                .any(|line| line.surface_positions() == vec![TilePos::new(1, 0)])
        );
    }

    #[test]
    fn side_input_to_target_first_tile_without_straight_predecessor_stays_separate() {
        let mut graph = TopologyGraph::default();
        graph.set_belt(TilePos::new(-1, 0), BeltTile::new(Direction::East));
        graph.set_belt(TilePos::new(0, 0), BeltTile::new(Direction::North));
        graph.set_belt(TilePos::new(0, 1), BeltTile::new(Direction::North));

        let topology = TopologyBuilder.rebuild(&graph);

        assert_eq!(topology.lines.len(), 2, "{:?}", topology.lines);
        assert!(
            topology.lines.iter().any(
                |line| line.surface_positions() == vec![TilePos::new(0, 0), TilePos::new(0, 1)]
            ),
            "target line should start at target first tile: {:?}",
            topology.lines
        );
        assert!(
            topology
                .lines
                .iter()
                .any(|line| line.surface_positions() == vec![TilePos::new(-1, 0)]),
            "side source should remain a single-tile line: {:?}",
            topology.lines
        );
    }

    #[test]
    fn side_merge_into_loop_corner_keeps_loop_closed() {
        let mut graph = TopologyGraph::default();
        for (pos, belt) in rectangular_cycle() {
            graph.set_belt(pos, belt);
        }
        graph.set_belt(TilePos::new(-1, 0), BeltTile::new(Direction::East));

        let topology = TopologyBuilder.rebuild(&graph);

        assert_eq!(topology.lines.len(), 2);
        assert!(topology.lines.iter().any(|line| {
            line.closed
                && line.surface_positions()
                    == vec![
                        TilePos::new(0, -2),
                        TilePos::new(0, -1),
                        TilePos::new(0, 0),
                        TilePos::new(1, 0),
                        TilePos::new(2, 0),
                        TilePos::new(2, -1),
                        TilePos::new(2, -2),
                        TilePos::new(1, -2),
                    ]
        }));
        assert!(
            topology.lines.iter().any(|line| {
                !line.closed && line.surface_positions() == vec![TilePos::new(-1, 0)]
            }),
            "side input should remain an open single-tile line: {:?}",
            topology.lines
        );
    }

    #[test]
    fn underground_pair_builds_separate_surface_lines() {
        use crate::topology::graph::{BeltTile, Direction, UndergroundLink};

        let mut graph = TopologyGraph::default();
        let entrance = TilePos::new(0, 0);
        let exit = TilePos::new(4, 0);
        let speed = crate::units::UnitsPerTick::new(4);
        graph.set_belt(entrance, BeltTile::new(Direction::East));
        graph.set_belt(exit, BeltTile::new(Direction::East));
        graph.set_underground_link(
            entrance,
            UndergroundLink {
                partner: exit,
                role: UndergroundEndpointRole::Entrance,
                speed,
            },
        );
        graph.set_underground_link(
            exit,
            UndergroundLink {
                partner: entrance,
                role: UndergroundEndpointRole::Exit,
                speed,
            },
        );

        let topology = TopologyBuilder.rebuild(&graph);

        assert_eq!(topology.lines.len(), 2);
        assert!(topology.lines.iter().any(|line| {
            !line.closed
                && line.tiles == vec![BuiltPathTile::surface(entrance)]
                && line.front_output == Some(TilePos::new(1, 0))
        }));
        assert!(topology.lines.iter().any(|line| {
            !line.closed
                && line.tiles == vec![BuiltPathTile::surface(exit)]
                && line.front_output == Some(TilePos::new(5, 0))
        }));
    }

    fn rectangular_cycle() -> [(TilePos, BeltTile); 8] {
        [
            (
                TilePos::new(0, 0),
                BeltTile::turn(Direction::North, Direction::East),
            ),
            (TilePos::new(1, 0), BeltTile::new(Direction::East)),
            (
                TilePos::new(2, 0),
                BeltTile::turn(Direction::East, Direction::South),
            ),
            (TilePos::new(2, -1), BeltTile::new(Direction::South)),
            (
                TilePos::new(2, -2),
                BeltTile::turn(Direction::South, Direction::West),
            ),
            (TilePos::new(1, -2), BeltTile::new(Direction::West)),
            (
                TilePos::new(0, -2),
                BeltTile::turn(Direction::West, Direction::North),
            ),
            (TilePos::new(0, -1), BeltTile::new(Direction::North)),
        ]
    }
}
