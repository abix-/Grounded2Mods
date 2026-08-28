//! Where a part can attach, cataloged from the game's own
//! buildings.
//!
//! The designers already built everything, and each part in a
//! building is CONNECTED to another part where the two meshes
//! share coordinates on a border: the bottom of a wall occupies
//! the same coordinates as the rim of its floor. **That shared
//! border is a STUD.** This module finds them.
//!
//! Two measured facts shape the test (misery parts.md):
//!
//! - Connected parts OVERLAP slightly rather than meeting
//!   exactly: a wall sinks 2 cm into its floor. Shared
//!   coordinates means within a tolerance, not equality.
//! - Placed parts sit at an angle, so borders are compared in
//!   the parts' OWN turned frame, never on world axes.
//!
//! Each stud is recorded on BOTH parts, in each part's own
//! frame. Per part, never per pair: any part with a matching
//! stud can substitute. The same idea as Wave Function Collapse,
//! learning which parts sit against which from a real example.

use std::collections::HashMap;

use crate::structure::PartDef;

/// One place a part can be attached to, in ITS OWN local frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Stud {
    /// Local position of the shared border's middle, metres,
    /// y up. Same units and axes as a `PartDef`'s extent.
    pub at: (f64, f64, f64),
    /// How far the attached part is turned relative to this one,
    /// degrees. Position alone is not enough: a wall laid across
    /// a floor and one stood along it sit at the same spot.
    pub turn: i64,
    /// How many placements confirmed this. 231 is a rule, 4 is a
    /// maybe.
    pub seen: usize,
    /// Which parts were actually seen here, counted. Answers
    /// "what does the game put here", and biases generation
    /// toward looking like this game rather than merely being
    /// legal.
    pub with: HashMap<String, usize>,
}

/// How borders are read.
#[derive(Clone, Copy, Debug)]
pub struct Derive {
    /// Two borders this close, metres, share coordinates. Covers
    /// the measured interlock (a wall sinks 2 cm into its floor)
    /// and hand-nudged placements.
    pub touch_m: f64,
    /// Stud positions round to this, centimetres, so a nudged
    /// placement does not invent a new stud.
    pub round_cm: f64,
    /// Building parts join at quarter turns. A pair whose
    /// relative turn is further than this, degrees, from a
    /// quarter is not compared at all.
    pub snap_deg: f64,
    /// A stud needs this many confirmations. One placed room is
    /// already a correct catalog entry, so the default is 1.
    pub min_seen: usize,
}

impl Default for Derive {
    fn default() -> Self {
        Self {
            touch_m: 0.05,
            round_cm: 1.0,
            snap_deg: 15.0,
            min_seen: 1,
        }
    }
}

/// A part's own box: pivot and extent, scaled. Metres, y up, in
/// the part's local frame.
fn own_box(p: &PartDef) -> ((f64, f64, f64), (f64, f64, f64)) {
    let s = p.scale.abs() as f64;
    let pv = (
        p.pivot.x as f64 * s,
        p.pivot.y as f64 * s,
        p.pivot.z as f64 * s,
    );
    let ex = (
        p.extent.x as f64 * s,
        p.extent.y as f64 * s,
        p.extent.z as f64 * s,
    );
    (
        (pv.0 - ex.0, pv.1 - ex.1, pv.2 - ex.2),
        (pv.0 + ex.0, pv.1 + ex.1, pv.2 + ex.2),
    )
}

/// Turn a vector about the up axis.
fn about_up(v: (f64, f64, f64), radians: f64) -> (f64, f64, f64) {
    let (s, c) = radians.sin_cos();
    (v.0 * c - v.2 * s, v.1, v.0 * s + v.2 * c)
}

/// Relative turn to degrees, snapped to the given step and
/// folded into 0..360.
fn turn_of(radians: f64, step_deg: f64) -> i64 {
    let deg = radians.to_degrees();
    let snapped = (deg / step_deg).round() * step_deg;
    ((snapped as i64 % 360) + 360) % 360
}

fn snap(v: f64, round_cm: f64) -> f64 {
    ((v * 100.0 / round_cm).round() * round_cm) / 100.0
}

/// The name a stud is filed under: the mesh where there is one,
/// the class otherwise.
fn name_of(p: &PartDef) -> &str {
    p.asset.as_deref().unwrap_or(&p.class)
}

/// Every stud in one set of placed parts.
///
/// For each pair whose relative turn is a quarter (within
/// `snap_deg`), part B's box is carried into part A's own frame.
/// If the boxes meet or overlap within `touch_m` on every axis,
/// the parts share a border, and the middle of the shared region
/// becomes a stud on each part, in each part's own frame.
pub fn studs_in(parts: &[PartDef], how: Derive) -> HashMap<String, Vec<Stud>> {
    // part name -> (rounded local position, turn) -> (count, who)
    type Key = (i64, i64, i64, i64);
    let mut acc: HashMap<String, HashMap<Key, (usize, HashMap<String, usize>)>> = HashMap::new();

    let mut note = |part: &str, at: (f64, f64, f64), turn: i64, other: &str| {
        let key = (
            (at.0 * 100.0).round() as i64,
            (at.1 * 100.0).round() as i64,
            (at.2 * 100.0).round() as i64,
            turn,
        );
        let entry = acc
            .entry(part.to_string())
            .or_default()
            .entry(key)
            .or_insert_with(|| (0, HashMap::new()));
        entry.0 += 1;
        *entry.1.entry(other.to_string()).or_default() += 1;
    };

    for (i, a) in parts.iter().enumerate() {
        for b in parts.iter().skip(i + 1) {
            let rel = (b.yaw - a.yaw) as f64;
            // Building parts join at quarter turns; anything
            // else is scenery leaning near scenery.
            let quarter = (rel.to_degrees() / 90.0).round() * 90.0;
            if (rel.to_degrees() - quarter).abs() > how.snap_deg {
                continue;
            }

            // B's box, in A's own frame: B's corners turned by
            // the relative yaw, then moved by the offset between
            // the two placements read in A's frame.
            let d = (
                (b.offset.x - a.offset.x) as f64,
                (b.offset.y - a.offset.y) as f64,
                (b.offset.z - a.offset.z) as f64,
            );
            let d = about_up(d, -(a.yaw as f64));
            let (bmin, bmax) = own_box(b);
            let mut lo = (f64::MAX, f64::MAX, f64::MAX);
            let mut hi = (f64::MIN, f64::MIN, f64::MIN);
            for cx in [bmin.0, bmax.0] {
                for cy in [bmin.1, bmax.1] {
                    for cz in [bmin.2, bmax.2] {
                        let w = about_up((cx, cy, cz), rel);
                        let w = (w.0 + d.0, w.1 + d.1, w.2 + d.2);
                        lo = (lo.0.min(w.0), lo.1.min(w.1), lo.2.min(w.2));
                        hi = (hi.0.max(w.0), hi.1.max(w.1), hi.2.max(w.2));
                    }
                }
            }
            let (amin, amax) = own_box(a);

            // Sharing a border: on every axis the two boxes meet,
            // overlap, or miss by no more than the tolerance.
            let gap = [
                (amin.0 - hi.0).max(lo.0 - amax.0),
                (amin.1 - hi.1).max(lo.1 - amax.1),
                (amin.2 - hi.2).max(lo.2 - amax.2),
            ];
            if gap.iter().any(|g| *g > how.touch_m) {
                continue;
            }

            // The middle of the shared region, in A's frame.
            let mid = |alo: f64, ahi: f64, blo: f64, bhi: f64| (alo.max(blo) + ahi.min(bhi)) / 2.0;
            let c = (
                mid(amin.0, amax.0, lo.0, hi.0),
                mid(amin.1, amax.1, lo.1, hi.1),
                mid(amin.2, amax.2, lo.2, hi.2),
            );
            let at_a = (
                snap(c.0, how.round_cm),
                snap(c.1, how.round_cm),
                snap(c.2, how.round_cm),
            );
            note(name_of(a), at_a, turn_of(rel, how.snap_deg), name_of(b));

            // The same point in B's frame: out of A's frame into
            // the shared placement space, then into B's.
            let w = about_up(c, a.yaw as f64);
            let w = (
                w.0 + a.offset.x as f64 - b.offset.x as f64,
                w.1 + a.offset.y as f64 - b.offset.y as f64,
                w.2 + a.offset.z as f64 - b.offset.z as f64,
            );
            let cb = about_up(w, -(b.yaw as f64));
            let at_b = (
                snap(cb.0, how.round_cm),
                snap(cb.1, how.round_cm),
                snap(cb.2, how.round_cm),
            );
            note(name_of(b), at_b, turn_of(-rel, how.snap_deg), name_of(a));
        }
    }

    acc.into_iter()
        .map(|(part, keys)| {
            let mut studs: Vec<Stud> = keys
                .into_iter()
                .filter(|(_, (n, _))| *n >= how.min_seen)
                .map(|((x, y, z, turn), (seen, with))| Stud {
                    at: (x as f64 / 100.0, y as f64 / 100.0, z as f64 / 100.0),
                    turn,
                    seen,
                    with,
                })
                .collect();
            // Commonest first: the way the game usually does it
            // is the first thing read, and the first reached for.
            studs.sort_by(|a, b| b.seen.cmp(&a.seen));
            (part, studs)
        })
        .filter(|(_, studs)| !studs.is_empty())
        .collect()
}

/// Add one set of studs into another: the same stud on the same
/// part (same place, same turn) gains the confirmations, a new
/// one is added. This is how one catalog grows over many levels.
pub fn merge(into: &mut HashMap<String, Vec<Stud>>, from: HashMap<String, Vec<Stud>>) {
    for (name, studs) in from {
        let list = into.entry(name).or_default();
        for s in studs {
            if let Some(e) = list.iter_mut().find(|e| e.at == s.at && e.turn == s.turn) {
                e.seen += s.seen;
                for (k, v) in s.with {
                    *e.with.entry(k).or_default() += v;
                }
            } else {
                list.push(s);
            }
        }
    }
}

/// Keep only studs confirmed at least `min_seen` times, commonest
/// first. Run ONCE at the end of a catalog pass: a real
/// attachment recurs across levels, a one-off paving seam does
/// not.
pub fn cull(map: &mut HashMap<String, Vec<Stud>>, min_seen: usize) {
    for list in map.values_mut() {
        list.retain(|s| s.seen >= min_seen);
        list.sort_by(|a, b| b.seen.cmp(&a.seen));
    }
    map.retain(|_, list| !list.is_empty());
}

/// Why a join is refused, in words a person can read back.
#[derive(Debug, PartialEq)]
pub enum Refusal {
    /// The first part has no such stud.
    NoStud,
    /// The game never puts the second part at this stud, or not
    /// often enough.
    NeverSeenHere { seen: usize },
    /// The second part never carries the mirror stud pointing
    /// back. One-sided evidence is a reading error, not a rule.
    NoMirror,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::NoStud => write!(f, "no stud at that spot"),
            Refusal::NeverSeenHere { seen } => {
                write!(
                    f,
                    "the game puts that part here {seen} time(s), fewer than required"
                )
            }
            Refusal::NoMirror => write!(f, "the other part never carries the mirror stud"),
        }
    }
}

/// THE LEGO RULE: may `other` join `part` at the stud at `at`
/// (part's own frame) turned `turn`?
///
/// The catalog IS the rule. Legal means the game itself does it:
/// the stud's partners include `other` at least `min_seen` times,
/// AND `other` carries a mirror stud that lists `part` back.
/// Both sides must agree; that is what recording every stud on
/// both parts was for. No kinds, no taxonomy: where the game
/// does something a rule forbids, the rule is wrong.
pub fn may_join(
    catalog: &HashMap<String, Vec<Stud>>,
    part: &str,
    at: (f64, f64, f64),
    turn: i64,
    other: &str,
    min_seen: usize,
) -> Result<(), Refusal> {
    let Some(stud) = catalog
        .get(part)
        .and_then(|list| list.iter().find(|s| s.at == at && s.turn == turn))
    else {
        return Err(Refusal::NoStud);
    };
    let seen = stud.with.get(other).copied().unwrap_or(0);
    if seen < min_seen.max(1) {
        return Err(Refusal::NeverSeenHere { seen });
    }
    let mirrored = catalog
        .get(other)
        .is_some_and(|list| list.iter().any(|s| s.with.contains_key(part)));
    if !mirrored {
        return Err(Refusal::NoMirror);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    /// A part at a spot: floor tiles are 2 m half-wide slabs with
    /// the pivot at a corner (the real SM_Floor_400x400 numbers);
    /// walls are thin tall panels with the pivot at the bottom
    /// edge (the real SM_Wall numbers).
    fn place(asset: &str, x: f32, y: f32, z: f32, yaw: f32) -> PartDef {
        let (extent, pivot) = match asset {
            a if a.starts_with("floor") => (Vec3::new(2.0, 0.11, 2.0), Vec3::new(2.0, -0.09, -2.0)),
            a if a.starts_with("wall") => (Vec3::new(0.1, 2.0, 2.0), Vec3::new(0.0, 2.0, -2.0)),
            _ => (Vec3::new(0.5, 0.5, 0.5), Vec3::ZERO),
        };
        PartDef {
            class: "StaticMeshActor".into(),
            asset: Some(asset.into()),
            offset: Vec3::new(x, y, z),
            yaw,
            pitch: 0.0,
            roll: 0.0,
            scale: 1.0,
            extent,
            pivot,
        }
    }

    /// Two floor tiles laid side by side share a border and each
    /// gets a stud; the same two a whole tile apart get nothing.
    #[test]
    fn neighbours_share_a_border_and_strangers_do_not() {
        let side_by_side = vec![
            place("floor", 0.0, 0.0, 0.0, 0.0),
            place("floor", 4.0, 0.0, 0.0, 0.0),
        ];
        let studs = studs_in(&side_by_side, Derive::default());
        assert_eq!(studs["floor"].len(), 2, "a stud on each side of the seam");

        let apart = vec![
            place("floor", 0.0, 0.0, 0.0, 0.0),
            place("floor", 8.5, 0.0, 0.0, 0.0),
        ];
        assert!(
            studs_in(&apart, Derive::default()).is_empty(),
            "a tile apart is not a border"
        );
    }

    /// The measured interlock: a wall standing on a floor sits
    /// with its base 2 cm below the walking surface, overlapping
    /// the floor's box. It still shares the border; that is what
    /// the tolerance is for.
    #[test]
    fn the_interlock_overlap_still_counts() {
        let parts = vec![
            place("floor", 0.0, 0.0, 0.0, 0.0),
            place("wall", 1.0, 0.0, 0.0, 0.0),
        ];
        let studs = studs_in(&parts, Derive::default());
        assert!(studs.contains_key("wall"), "the wall carries a stud");
        assert!(studs.contains_key("floor"), "and so does the floor");
        let wall = &studs["wall"][0];
        assert!(
            wall.at.1 < 0.5,
            "the wall's stud is at its base, got {:?}",
            wall.at
        );
        assert_eq!(studs["floor"][0].with["wall"], 1);
    }

    /// Turning the whole assembly must not invent studs: each
    /// part is read in its own frame.
    #[test]
    fn turning_the_whole_thing_does_not_invent_studs() {
        let quarter = std::f32::consts::FRAC_PI_2;
        let upright = vec![
            place("floor", 0.0, 0.0, 0.0, 0.0),
            place("wall", 1.0, 0.0, 0.0, 0.0),
        ];
        // The same pair, the whole thing turned a quarter about
        // the floor's spot: the wall's offset turns with it.
        let w = about_up((1.0, 0.0, 0.0), quarter as f64);
        let turned = vec![
            place("floor", 0.0, 0.0, 0.0, quarter),
            place("wall", w.0 as f32, w.1 as f32, w.2 as f32, quarter),
        ];
        let a = studs_in(&upright, Derive::default());
        let b = studs_in(&turned, Derive::default());
        assert_eq!(a["floor"][0].at, b["floor"][0].at);
        assert_eq!(a["floor"][0].turn, b["floor"][0].turn);
        assert_eq!(a["wall"][0].at, b["wall"][0].at);
    }

    /// The same stud seen in two levels gains confirmations; the
    /// cull then keeps it and drops the one-off.
    #[test]
    fn confirmations_accumulate_across_levels_and_one_offs_fall() {
        let room = vec![
            place("floor", 0.0, 0.0, 0.0, 0.0),
            place("wall", 1.0, 0.0, 0.0, 0.0),
        ];
        let oddity = vec![
            place("floor", 0.0, 0.0, 0.0, 0.0),
            place("floor", 3.7, 0.0, 0.0, 0.0),
        ];
        let mut acc = HashMap::new();
        for _ in 0..3 {
            merge(&mut acc, studs_in(&room, Derive::default()));
        }
        merge(&mut acc, studs_in(&oddity, Derive::default()));

        cull(&mut acc, 3);
        let wall = &acc["wall"][0];
        assert_eq!(wall.seen, 3, "three levels confirmed the wall stud");
        assert!(
            acc["floor"].iter().all(|s| s.seen >= 3),
            "the one-off paving seam fell to the cull"
        );
    }

    /// THE LEGO RULE, on a catalog built from real placements: a
    /// wall joins its floor, a window frame is refused with the
    /// reason named, and the substitution the design promises
    /// (a door wall at a plain wall's stud) is legal for free.
    #[test]
    fn the_rule_allows_what_the_game_does_and_refuses_the_rest() {
        let room = vec![
            place("floor", 0.0, 0.0, 0.0, 0.0),
            place("wall", 1.0, 0.0, 0.0, 0.0),
            place("wall_door", 3.0, 0.0, 0.0, 0.0),
        ];
        let catalog = studs_in(&room, Derive::default());

        let stud = catalog["floor"]
            .iter()
            .find(|s| s.with.contains_key("wall"))
            .expect("the wall was seen on the floor");
        assert_eq!(
            may_join(&catalog, "floor", stud.at, stud.turn, "wall", 1),
            Ok(()),
            "the game puts walls on floors"
        );
        // The wall's seam and the door wall's seam are different
        // studs, so at the WALL'S stud a window frame was seen
        // zero times.
        assert_eq!(
            may_join(&catalog, "floor", stud.at, stud.turn, "window_frame", 1),
            Err(Refusal::NeverSeenHere { seen: 0 }),
            "the game never puts a window frame there"
        );
        assert_eq!(
            may_join(&catalog, "floor", (9.0, 9.0, 9.0), 0, "wall", 1),
            Err(Refusal::NoStud),
            "no stud at a made-up spot"
        );

        // Substitution: wherever the floor's stud saw a door
        // wall, the door wall is legal exactly like the wall.
        let door_stud = catalog["floor"]
            .iter()
            .find(|s| s.with.contains_key("wall_door"))
            .expect("the door wall was seen on the floor");
        assert_eq!(
            may_join(
                &catalog,
                "floor",
                door_stud.at,
                door_stud.turn,
                "wall_door",
                1
            ),
            Ok(())
        );
    }

    /// A pair at a non-quarter relative turn is scenery, not a
    /// connection.
    #[test]
    fn odd_angles_are_not_compared() {
        let parts = vec![
            place("floor", 0.0, 0.0, 0.0, 0.0),
            place("floor", 4.0, 0.0, 0.0, 0.6),
        ];
        assert!(studs_in(&parts, Derive::default()).is_empty());
    }
}
