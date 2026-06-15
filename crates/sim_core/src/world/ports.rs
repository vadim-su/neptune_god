//! Building port geometry: map catalog port defs to footprint edge tiles.
//!
//! Used by inserters and belt I/O to resolve pickup/drop roles relative to rotation.

use super::*;

pub(super) fn adjacent_tile(origin: TilePos, direction: Direction) -> TilePos {
    let (dx, dy) = direction.delta();
    TilePos::new(origin.x + dx, origin.y + dy)
}

pub(super) fn building_ports(
    origin: TilePos,
    footprint: &[TilePos],
    direction: Direction,
    surface_z: SurfaceZ,
    def: &crate::catalog::CoreBuildingDef,
) -> Vec<SimBuildingPort> {
    let mut ports = Vec::new();
    for port in &def.inputs {
        ports.extend(
            port_tiles(origin, footprint, direction, port)
                .into_iter()
                .map(|tile| SimBuildingPort {
                    role: port.role,
                    tile,
                    surface_z,
                    accepts: port.accepts.clone(),
                }),
        );
    }
    if def.inputs.is_empty() {
        for inventory in def
            .inventories
            .iter()
            .filter(|inventory| inventory.role == CoreInventoryRole::Input)
        {
            for tile in all_footprint_edge_tiles(origin, footprint) {
                ports.push(SimBuildingPort {
                    role: CorePortRole::Input,
                    tile,
                    surface_z,
                    accepts: inventory.accepts.clone(),
                });
            }
        }
    }
    for port in &def.outputs {
        ports.extend(
            port_tiles(origin, footprint, direction, port)
                .into_iter()
                .map(|tile| SimBuildingPort {
                    role: port.role,
                    tile,
                    surface_z,
                    accepts: port.accepts.clone(),
                }),
        );
    }
    for inventory in &def.inventories {
        let role = match inventory.role {
            CoreInventoryRole::Fuel => CorePortRole::Fuel,
            CoreInventoryRole::Storage => CorePortRole::Storage,
            _ => continue,
        };
        for tile in all_edge_port_tiles(origin, footprint) {
            ports.push(SimBuildingPort {
                role,
                tile,
                surface_z,
                accepts: inventory.accepts.clone(),
            });
        }
    }
    ports.sort_by_key(|port| (port.tile.y, port.tile.x, core_port_role_order(port.role)));
    ports.dedup_by(|left, right| {
        left.role == right.role
            && left.tile == right.tile
            && left.surface_z == right.surface_z
            && left.accepts == right.accepts
    });
    ports
}

fn all_footprint_edge_tiles(origin: TilePos, footprint: &[TilePos]) -> Vec<TilePos> {
    let mut tiles = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ]
    .into_iter()
    .flat_map(|direction| footprint_edge_tiles(origin, footprint, direction))
    .collect::<Vec<_>>();
    tiles.sort();
    tiles.dedup();
    tiles
}

pub(super) fn port_tiles(
    origin: TilePos,
    footprint: &[TilePos],
    direction: Direction,
    port: &CorePortDef,
) -> Vec<TilePos> {
    let side = match port.side {
        CorePortSide::North => Direction::North,
        CorePortSide::East => Direction::East,
        CorePortSide::South => Direction::South,
        CorePortSide::West => Direction::West,
        CorePortSide::OutputDirection => direction,
        CorePortSide::OppositeOutput => direction.opposite(),
        CorePortSide::OutputDirectionLeft => direction.left(),
        CorePortSide::OutputDirectionRight => direction.right(),
        CorePortSide::AllEdges => {
            let mut tiles = [
                Direction::North,
                Direction::East,
                Direction::South,
                Direction::West,
            ]
            .into_iter()
            .flat_map(|side| footprint_edge_tiles(origin, footprint, side))
            .collect::<Vec<_>>();
            tiles.sort();
            tiles.dedup();
            if port.offsets.is_empty() {
                return tiles;
            }
            return port
                .offsets
                .iter()
                .filter_map(|offset| usize::try_from(*offset).ok())
                .filter_map(|offset| tiles.get(offset).copied())
                .collect();
        }
    };
    let edge = footprint_edge_tiles(origin, footprint, side);
    if port.offsets.is_empty() {
        return edge;
    }
    port.offsets
        .iter()
        .filter_map(|offset| usize::try_from(*offset).ok())
        .filter_map(|offset| edge.get(offset).copied())
        .collect()
}

pub(super) fn all_edge_port_tiles(origin: TilePos, footprint: &[TilePos]) -> Vec<TilePos> {
    let mut tiles = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ]
    .into_iter()
    .flat_map(|direction| edge_port_tiles(origin, footprint, direction))
    .collect::<Vec<_>>();
    tiles.sort();
    tiles.dedup();
    tiles
}

pub(super) fn footprint_edge_tiles(
    origin: TilePos,
    footprint: &[TilePos],
    direction: Direction,
) -> Vec<TilePos> {
    if footprint.is_empty() {
        return vec![origin];
    }
    let mut tiles = footprint.to_vec();
    match direction {
        Direction::North => {
            let edge = tiles.iter().map(|tile| tile.y).max().unwrap_or(origin.y);
            tiles.retain(|tile| tile.y == edge);
            tiles.sort_by_key(|tile| tile.x);
        }
        Direction::East => {
            let edge = tiles.iter().map(|tile| tile.x).max().unwrap_or(origin.x);
            tiles.retain(|tile| tile.x == edge);
            tiles.sort_by_key(|tile| tile.y);
        }
        Direction::South => {
            let edge = tiles.iter().map(|tile| tile.y).min().unwrap_or(origin.y);
            tiles.retain(|tile| tile.y == edge);
            tiles.sort_by_key(|tile| tile.x);
        }
        Direction::West => {
            let edge = tiles.iter().map(|tile| tile.x).min().unwrap_or(origin.x);
            tiles.retain(|tile| tile.x == edge);
            tiles.sort_by_key(|tile| tile.y);
        }
    }
    tiles
}

pub(super) fn edge_port_tiles(
    origin: TilePos,
    footprint: &[TilePos],
    direction: Direction,
) -> Vec<TilePos> {
    if footprint.is_empty() {
        return vec![adjacent_tile(origin, direction)];
    }
    footprint_edge_tiles(origin, footprint, direction)
        .into_iter()
        .map(|tile| adjacent_tile(tile, direction))
        .collect()
}

pub(super) fn core_port_role_order(role: CorePortRole) -> u8 {
    match role {
        CorePortRole::Input => 0,
        CorePortRole::Output => 1,
        CorePortRole::Fuel => 2,
        CorePortRole::Storage => 3,
        CorePortRole::BeltLane => 4,
    }
}

pub(super) fn port_inventory_role(role: CorePortRole) -> Option<CoreInventoryRole> {
    match role {
        CorePortRole::Input => Some(CoreInventoryRole::Input),
        CorePortRole::Output => Some(CoreInventoryRole::Output),
        CorePortRole::Fuel => Some(CoreInventoryRole::Fuel),
        CorePortRole::Storage => Some(CoreInventoryRole::Storage),
        CorePortRole::BeltLane => None,
    }
}

pub(super) fn direction_between_adjacent(from: TilePos, to: TilePos) -> Option<Direction> {
    match (to.x - from.x, to.y - from.y) {
        (0, 1) => Some(Direction::North),
        (1, 0) => Some(Direction::East),
        (0, -1) => Some(Direction::South),
        (-1, 0) => Some(Direction::West),
        _ => None,
    }
}

pub(super) fn pickup_role_order(role: CoreInventoryRole) -> Option<CandidateRoleOrder> {
    match role {
        CoreInventoryRole::Output => Some(CandidateRoleOrder::Output),
        CoreInventoryRole::Storage => Some(CandidateRoleOrder::Storage),
        CoreInventoryRole::Input | CoreInventoryRole::Fuel | CoreInventoryRole::InserterHand => {
            None
        }
    }
}

pub(super) fn drop_role_order(role: CoreInventoryRole) -> Option<CandidateRoleOrder> {
    match role {
        CoreInventoryRole::Input => Some(CandidateRoleOrder::Input),
        CoreInventoryRole::Fuel => Some(CandidateRoleOrder::Fuel),
        CoreInventoryRole::Storage => Some(CandidateRoleOrder::Storage),
        CoreInventoryRole::Output | CoreInventoryRole::InserterHand => None,
    }
}
