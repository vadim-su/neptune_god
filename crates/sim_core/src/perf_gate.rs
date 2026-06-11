//! Performance gate: long-line transport must stay within scan/chunk budgets.

use crate::catalog::{CoreCatalog, TEST_IRON_ORE};
use crate::ids::TilePos;
use crate::topology::graph::Direction;
use crate::units::UnitsPerTick;
use crate::view::VisibleTileBounds;
use crate::world::SimWorld;

const LONG_LINE_TILES: i32 = 50_000;
const ITEMS_PER_LANE: usize = 50_000;
const TOTAL_ITEMS: usize = ITEMS_PER_LANE * 2;
const SAMPLE_TICKS: usize = 120;
const MAX_TOTAL_MOVEMENT_SCANS: usize = 1_000;
const MAX_CHANGED_CHUNKS_TOUCHED: usize = 0;
const MAX_VISIBLE_ITEMS: usize = 512;
const MAX_VISIBLE_QUERY_VISITS: usize = 1_000;
const MAX_VISIBLE_PATH_TILE_VISITS: usize = 1_000;

fn build_long_line_100k_world_for_tests() -> SimWorld {
    let mut world = SimWorld::with_catalog(CoreCatalog::for_tests());
    world
        .build_straight_belt_line_for_tests(
            TilePos::new(0, 0),
            LONG_LINE_TILES,
            Direction::East,
            UnitsPerTick::new(8),
        )
        .unwrap();
    world
        .insert_many_at_line_start_for_tests(0, 0, TEST_IRON_ORE, ITEMS_PER_LANE)
        .unwrap();
    world
        .insert_many_at_line_start_for_tests(0, 1, TEST_IRON_ORE, ITEMS_PER_LANE)
        .unwrap();
    world
}

fn assert_visible_query_bound(world: &SimWorld, bounds: VisibleTileBounds, label: &str) {
    let (visible_items, stats) = world.visible_items_for_bounds_with_stats_for_tests(bounds);

    assert!(
        visible_items.len() <= MAX_VISIBLE_ITEMS,
        "{label} returned {} visible items",
        visible_items.len()
    );
    assert!(
        stats.item_visits <= MAX_VISIBLE_QUERY_VISITS,
        "{label} visited {} items",
        stats.item_visits
    );
    assert!(
        stats.path_tile_visits <= MAX_VISIBLE_PATH_TILE_VISITS,
        "{label} visited {} path tiles",
        stats.path_tile_visits
    );
}

#[test]
fn long_line_100k_tick_work_stays_bounded() {
    let mut world = build_long_line_100k_world_for_tests();
    let mut total_items_scanned = 0;
    let mut total_changed_chunks = 0;
    let first_output = world.tick_core_only_for_tests();
    total_items_scanned += first_output.metrics.items_scanned;
    total_changed_chunks += first_output.diff.changed_chunks.len();
    for _ in 1..SAMPLE_TICKS {
        let output = world.tick_core_only_for_tests();
        total_items_scanned += output.metrics.items_scanned;
        total_changed_chunks += output.diff.changed_chunks.len();
    }

    assert_eq!(first_output.metrics.simulated_items, TOTAL_ITEMS);
    assert_eq!(first_output.metrics.active_lines, 1);
    assert!(
        total_items_scanned <= MAX_TOTAL_MOVEMENT_SCANS,
        "100k line movement scanned {total_items_scanned} items over {SAMPLE_TICKS} ticks"
    );
    assert_eq!(
        total_changed_chunks, MAX_CHANGED_CHUNKS_TOUCHED,
        "100k line movement touched changed chunks over {SAMPLE_TICKS} ticks"
    );
}

#[test]
fn long_line_100k_visible_queries_stay_bounded_to_view() {
    let world = build_long_line_100k_world_for_tests();

    assert_visible_query_bound(
        &world,
        VisibleTileBounds::new(TilePos::new(0, -8), TilePos::new(128, 8)),
        "start window",
    );
    assert_visible_query_bound(
        &world,
        VisibleTileBounds::new(TilePos::new(25_000, 0), TilePos::new(25_004, 0)),
        "middle window",
    );
    assert_visible_query_bound(
        &world,
        VisibleTileBounds::new(
            TilePos::new(LONG_LINE_TILES - 1, 0),
            TilePos::new(LONG_LINE_TILES - 1, 0),
        ),
        "tail window",
    );

    let (visible_items, stats) =
        world.visible_items_for_bounds_with_stats_for_tests(VisibleTileBounds::new(
            TilePos::new(LONG_LINE_TILES + 10, -8),
            TilePos::new(LONG_LINE_TILES + 128, 8),
        ));
    assert_eq!(visible_items.len(), 0, "offscreen window returned items");
    assert!(
        stats.item_visits <= MAX_VISIBLE_QUERY_VISITS,
        "offscreen window visited {} items",
        stats.item_visits
    );
    assert!(
        stats.path_tile_visits <= MAX_VISIBLE_PATH_TILE_VISITS,
        "offscreen window visited {} path tiles",
        stats.path_tile_visits
    );
}
