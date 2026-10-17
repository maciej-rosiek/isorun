//! The port of incremental.test.ts: the incremental build (`BuildOptions::dirty`) must give
//! the scene the full build gives, at every flush, for the whole recorded session: the same
//! cells, billboards, lids, level and bounds, and the same layout revisions as the plain
//! `previous` chain. Replayed at several flush cadences, since a flush may gather one message
//! or many.

mod common;

use std::collections::HashSet;

use common::{gd, messages, replayed};
use isorun_scene::{CellKind, Scene};
use isorun_scene_webtiles::*;
use isorun_webtiles::{initial_state, map_key, reduce, GameState};

/// Everything a scene says, with the bookkeeping revisions left out and the cells in key order.
fn same_semantics(a: &Scene, b: &Scene) {
    assert_eq!(a.bounds, b.bounds);
    assert_eq!(a.player, b.player);
    assert_eq!(a.player_on_level, b.player_on_level);
    assert_eq!(a.level, b.level);
    assert_eq!(a.cells.len(), b.cells.len());
    for (k, c) in &a.cells {
        assert_eq!(Some(c), b.cells.get(k), "cell {k}");
    }
    assert_eq!(a.billboards, b.billboards);
}

fn full(st: &GameState) -> Scene {
    build_scene(st, gd(), BuildOptions::default())
}

fn incremental(st: &GameState, previous: Option<&Scene>, dirty: Dirty, record: &mut BuildRecord) -> Scene {
    build_scene(st, gd(), BuildOptions { previous, dirty: Some(dirty), record: Some(record) })
}

fn matches_the_full_build_at_every_flush(every: usize) {
    let mut state = initial_state();
    let mut chained: Option<Scene> = None;
    let mut inc: Option<Scene> = None;
    let mut record = BuildRecord::default();
    let (mut flushes, mut reused, mut carried) = (0, 0, 0);
    let mut flush = |state: &mut GameState| {
        flushes += 1;
        let f = full(state);
        let c = build_scene(state, gd(), BuildOptions { previous: chained.as_ref(), ..BuildOptions::default() });
        let had = inc.is_some();
        let i = incremental(state, inc.as_ref(), Dirty::of(state), &mut record);
        state.clear_dirty();
        same_semantics(&i, &f);
        // the layout revisions move as the plain chain's do
        assert_eq!(i.layout_revision == i.revision, c.layout_revision == c.revision);
        assert_eq!(i.revision, c.revision);
        if had {
            reused += 1;
            carried += record.carried();
        }
        chained = Some(c);
        inc = Some(i);
    };
    for (n, m) in messages().iter().enumerate() {
        reduce(&mut state, m);
        if (n + 1) % every == 0 {
            flush(&mut state);
        }
    }
    flush(&mut state);
    assert!(flushes > 2);
    // and it did carry cells over rather than rebuild them: most of an explored level stands still between flushes
    assert!(carried > 0);
    assert!(reused > 0);
}

#[test]
fn matches_the_full_build_at_every_flush_one_flush_every_message() {
    matches_the_full_build_at_every_flush(1);
}

#[test]
fn matches_the_full_build_at_every_flush_one_flush_every_7_messages() {
    matches_the_full_build_at_every_flush(7);
}

#[test]
fn matches_the_full_build_at_every_flush_one_flush_every_50_messages() {
    matches_the_full_build_at_every_flush(50);
}

#[test]
fn carries_untouched_cells_and_billboards_over_and_rebuilds_the_touched_ones() {
    let mut state = replayed();
    let mut record = BuildRecord::default();
    let a = incremental(&state, None, Dirty::of(&state), &mut record);
    state.clear_dirty();
    // nothing touched: everything stands as it is
    let none = HashSet::new();
    let b = incremental(&state, Some(&a), Dirty { cells: &none, map_cleared: false }, &mut record);
    assert_eq!(record.carried(), a.cells.len());
    assert_eq!(record.rebuilt(), 0);
    same_semantics(&b, &a);
    assert_eq!(b.layout_revision, a.layout_revision);
    // one cell touched: that cell is new, the rest stand
    let mut keys: Vec<_> = state.map.cells.keys().copied().collect();
    keys.sort();
    let some = keys.iter().map(|k| &state.map.cells[k]).find(|mc| mc.t.as_ref().is_some_and(|t| t.fg.is_some_and(|f| f.value() != 0))).unwrap();
    let one = HashSet::from([map_key(some.x, some.y)]);
    let c = incremental(&state, Some(&b), Dirty { cells: &one, map_cleared: false }, &mut record);
    assert_eq!(record.rebuilt(), 1);
    assert_eq!(record.carried(), c.cells.len() - 1);
    assert_eq!(c.layout_revision, b.layout_revision);
    // a clear builds everything afresh
    let d = incremental(&state, Some(&c), Dirty { cells: &none, map_cleared: true }, &mut record);
    assert_eq!(record.carried(), 0);
    assert_eq!(record.rebuilt(), d.cells.len());
    // and a record that describes another scene is not trusted
    let e = incremental(&state, Some(&b), Dirty { cells: &none, map_cleared: false }, &mut record);
    assert_eq!(record.carried(), 0);
    same_semantics(&e, &full(&state));
}

#[test]
fn does_not_trust_a_previous_scene_built_from_another_state() {
    let state = replayed();
    let mut other = initial_state();
    for m in &messages()[..40] {
        reduce(&mut other, m);
    }
    let mut record = BuildRecord::default();
    let foreign = build_scene(&other, gd(), BuildOptions { record: Some(&mut record), ..BuildOptions::default() });
    let none = HashSet::new();
    let s = incremental(&state, Some(&foreign), Dirty { cells: &none, map_cleared: false }, &mut record);
    assert_eq!(record.carried(), 0);
    same_semantics(&s, &full(&state));
}

#[test]
fn moves_the_lid_of_a_cell_whose_wall_neighbourhood_changed() {
    let gd = gd();
    let mut state = replayed();
    let mut record = BuildRecord::default();
    let a = incremental(&state, None, Dirty::of(&state), &mut record);
    state.clear_dirty();
    // find a wall cell of a non-dominant type: if the walls are all one type there is nothing to test here
    let mut walls: Vec<_> = a.cells.values().filter(|c| c.kind == CellKind::Wall && c.wall_tile.is_some()).collect();
    walls.sort_by_key(|c| (c.y, c.x));
    let dominant = a.level.ceiling_tile;
    let Some(odd) = walls.iter().find(|c| Some(gd.basetile(c.wall_tile.unwrap())) != dominant) else { return };
    // turn a dominant wall next to a floor into the odd type, the way a `map` message would
    let mut floors: Vec<_> = a.cells.values().filter(|c| c.kind == CellKind::Floor).collect();
    floors.sort_by_key(|c| (c.y, c.x));
    let floor = floors[0];
    let Some(target) = walls.iter().find(|c| Some(gd.basetile(c.wall_tile.unwrap())) == dominant && (c.x - floor.x).abs().max((c.y - floor.y).abs()) <= 1)
    else {
        return;
    };
    let odd_bg = state.map.cells[&map_key(odd.x, odd.y)].t.as_ref().unwrap().bg;
    let key = map_key(target.x, target.y);
    state.map.cells.get_mut(&key).unwrap().t.as_mut().unwrap().bg = odd_bg;
    state.dirty_cells.insert(key);
    let b = incremental(&state, Some(&a), Dirty::of(&state), &mut record);
    same_semantics(&b, &full(&state));
}
