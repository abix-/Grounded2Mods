//! Where a piece can attach, learned from how a game's own
//! designers put pieces together.
//!
//! A piece's size does not say where it attaches. A mesh's
//! bounding box is not its module: a floor tile measured 5.18 by
//! 5.73 m while sitting on a 3 by 4 m grid, because it has a lip
//! and the lip is really there. Its NAME is not evidence either.
//!
//! What is evidence is where the designers put things. Two pieces
//! that keep turning up at the same offset from each other are
//! joined, and that offset is the attachment.
//!
//! **A join is evidence. A stud is the model.** One observed join
//! yields TWO studs, one on each piece, each in THAT PIECE'S OWN
//! local frame. Measured in MISERY, a wall sits 350 cm above a
//! floor tile turned 90 degrees, 231 times, which gives:
//!
//! ```text
//! the FLOOR  a stud at local (0, +3.5, 0)   something attaches above me
//! the WALL   a stud at local (0, -3.5, 0)   I attach to something below me
//! ```
//!
//! Local frames are the point. Two pieces placed at different
//! angles are only comparable once each is read in its own, and
//! comparability is what lets ANY piece with a matching stud take
//! the other's place. That is how Lego works, and an edge list of
//! "this wall meets this floor" cannot express it.
//!
//! Prior art: Wave Function Collapse learns which tiles were
//! observed adjacent to which in an example, then generates from
//! that. This is the same idea in three dimensions with a real
//! building as the example.

use std::collections::HashMap;

/// One sighting: two pieces, where each was, and which way each
/// faced. Raw evidence, never edited, because the rules derived
/// from it will change and the sightings will not.
#[derive(Clone, Debug, PartialEq)]
pub struct Join {
    pub from: String,
    pub to: String,
    /// World offset from `from`'s origin to `to`'s, metres.
    pub offset: (f64, f64, f64),
    /// Facing of each piece, radians, as placed.
    pub from_yaw: f64,
    pub to_yaw: f64,
}

/// One place a piece can be attached to, in ITS OWN local frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Stud {
    /// Local position, metres, y up. Same units and axes as a
    /// `PieceDef`'s extent.
    pub at: (f64, f64, f64),
    /// How far the attached piece is turned relative to this one,
    /// degrees. Position alone is not enough: a wall laid across
    /// a floor and one stood along it sit at the same spot.
    pub turn: i64,
    /// How many times this was seen. Left visible rather than
    /// collapsed into a boolean, because 231 is a rule and 4 is a
    /// maybe.
    pub seen: usize,
    /// Which pieces were actually seen here, counted. Answers
    /// "what does the game put here", and biases generation
    /// toward looking like this game rather than merely being
    /// legal.
    pub with: HashMap<String, usize>,
}

/// Turn an offset measured in the world into one piece's local
/// frame, by undoing that piece's own facing.
///
/// Only the turn about the up axis is undone: kit pieces stand
/// upright, and a piece that is tilted is not something a stud
/// model describes anyway.
fn into_local(offset: (f64, f64, f64), yaw: f64) -> (f64, f64, f64) {
    let (s, c) = (-yaw).sin_cos();
    let (x, y, z) = offset;
    (x * c - z * s, y, x * s + z * c)
}

/// Round a position so a hand-nudged placement does not invent a
/// new stud. Centimetres in, metres out.
fn snap(v: f64, round_cm: f64) -> f64 {
    ((v * 100.0 / round_cm).round() * round_cm) / 100.0
}

/// Relative turn to degrees, snapped and folded into 0..360.
fn turn_of(radians: f64, snap_deg: f64) -> i64 {
    let deg = radians.to_degrees();
    let snapped = (deg / snap_deg).round() * snap_deg;
    ((snapped as i64 % 360) + 360) % 360
}

/// How the sightings are read.
#[derive(Clone, Copy, Debug)]
pub struct Derive {
    /// Positions round to this, centimetres.
    pub round_cm: f64,
    /// Turns snap to this, degrees.
    pub snap_deg: f64,
    /// A stud needs this many sightings before it counts as one
    /// rather than as a coincidence.
    pub min_seen: usize,
}

impl Default for Derive {
    fn default() -> Self {
        Self { round_cm: 1.0, snap_deg: 15.0, min_seen: 4 }
    }
}

/// Derive every piece's studs from the sightings.
///
/// Each join contributes to BOTH pieces: the `from` piece learns
/// that something attaches at that offset, and the `to` piece
/// learns that it attaches at the mirror of it. Both are recorded
/// in their own local frame, which is what makes them comparable
/// between pieces placed at different angles.
pub fn derive(joins: &[Join], how: Derive) -> HashMap<String, Vec<Stud>> {
    // piece -> (rounded local position, turn) -> (count, who)
    type Key = (i64, i64, i64, i64);
    let mut acc: HashMap<String, HashMap<Key, (usize, HashMap<String, usize>)>> = HashMap::new();

    let mut note = |piece: &str, at: (f64, f64, f64), turn: i64, other: &str| {
        let key = (
            (at.0 * 100.0).round() as i64,
            (at.1 * 100.0).round() as i64,
            (at.2 * 100.0).round() as i64,
            turn,
        );
        let entry = acc
            .entry(piece.to_string())
            .or_default()
            .entry(key)
            .or_insert_with(|| (0, HashMap::new()));
        entry.0 += 1;
        *entry.1.entry(other.to_string()).or_default() += 1;
    };

    for j in joins {
        let relative = j.to_yaw - j.from_yaw;

        // On `from`: something attaches over there, turned so.
        let at = into_local(j.offset, j.from_yaw);
        let at = (
            snap(at.0, how.round_cm),
            snap(at.1, how.round_cm),
            snap(at.2, how.round_cm),
        );
        note(&j.from, at, turn_of(relative, how.snap_deg), &j.to);

        // On `to`: I attach to something back that way. The
        // mirror, read in the OTHER piece's frame, which is not
        // simply the negated vector once the two face differently.
        let back = (-j.offset.0, -j.offset.1, -j.offset.2);
        let at = into_local(back, j.to_yaw);
        let at = (
            snap(at.0, how.round_cm),
            snap(at.1, how.round_cm),
            snap(at.2, how.round_cm),
        );
        note(&j.to, at, turn_of(-relative, how.snap_deg), &j.from);
    }

    acc.into_iter()
        .map(|(piece, keys)| {
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
            // should be the first thing read, and the first thing
            // reached for.
            studs.sort_by(|a, b| b.seen.cmp(&a.seen));
            (piece, studs)
        })
        .filter(|(_, studs)| !studs.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn join(from: &str, to: &str, offset: (f64, f64, f64), from_yaw: f64, to_yaw: f64) -> Join {
        Join {
            from: from.into(),
            to: to.into(),
            offset,
            from_yaw,
            to_yaw,
        }
    }

    /// The measured case: a wall 3.5 m above a floor, turned 90.
    /// Both pieces must come out of it, each in its own frame.
    #[test]
    fn one_join_gives_both_pieces_a_stud() {
        let quarter = std::f64::consts::FRAC_PI_2;
        let joins: Vec<Join> = (0..5)
            .map(|_| join("floor", "wall", (0.0, 3.5, 0.0), 0.0, quarter))
            .collect();
        let studs = derive(&joins, Derive::default());

        let floor = &studs["floor"][0];
        assert_eq!(floor.at, (0.0, 3.5, 0.0), "something attaches above the floor");
        assert_eq!(floor.turn, 90, "turned a quarter from the floor");
        assert_eq!(floor.seen, 5);
        assert_eq!(floor.with["wall"], 5);

        let wall = &studs["wall"][0];
        assert_eq!(wall.at.1, -3.5, "the wall attaches to something below it");
        assert_eq!(wall.turn, 270, "and the floor is a quarter back the other way");
    }

    /// The whole point: a stud is comparable BETWEEN pieces, so a
    /// third piece seen in the same place gets the same stud and
    /// can therefore substitute.
    #[test]
    fn a_different_piece_in_the_same_place_gets_the_same_stud() {
        let quarter = std::f64::consts::FRAC_PI_2;
        let mut joins: Vec<Join> = (0..5)
            .map(|_| join("floor", "wall", (0.0, 3.5, 0.0), 0.0, quarter))
            .collect();
        joins.extend((0..5).map(|_| join("floor", "window", (0.0, 3.5, 0.0), 0.0, quarter)));
        let studs = derive(&joins, Derive::default());

        // One stud on the floor, used by two different pieces.
        assert_eq!(studs["floor"].len(), 1);
        assert_eq!(studs["floor"][0].seen, 10);
        assert_eq!(studs["floor"][0].with["wall"], 5);
        assert_eq!(studs["floor"][0].with["window"], 5);

        // And both of those pieces have the SAME stud, which is
        // what makes them interchangeable there.
        assert_eq!(studs["wall"][0].at, studs["window"][0].at);
        assert_eq!(studs["wall"][0].turn, studs["window"][0].turn);
    }

    /// The same join seen with the floor turned reads as the same
    /// stud, because each piece is read in its own frame. Without
    /// that, every rotation of a building would invent new studs.
    #[test]
    fn turning_the_whole_thing_does_not_invent_studs() {
        let quarter = std::f64::consts::FRAC_PI_2;
        let upright: Vec<Join> = (0..5)
            .map(|_| join("floor", "wall", (0.0, 3.5, 0.0), 0.0, quarter))
            .collect();
        // The same pair, the whole assembly turned a quarter: the
        // world offset rotates with it.
        let turned: Vec<Join> = (0..5)
            .map(|_| join("floor", "wall", (0.0, 3.5, 0.0), quarter, quarter + quarter))
            .collect();

        let a = derive(&upright, Derive::default());
        let b = derive(&turned, Derive::default());
        assert_eq!(a["floor"][0].at, b["floor"][0].at);
        assert_eq!(a["floor"][0].turn, b["floor"][0].turn);
    }

    /// A one-off is not a rule.
    #[test]
    fn a_coincidence_is_not_a_stud() {
        let joins = vec![join("floor", "oddity", (1.7, 0.3, 0.9), 0.0, 0.0)];
        let studs = derive(&joins, Derive::default());
        assert!(studs.is_empty(), "one sighting should not become a stud");
    }

    /// Two pieces a whole tile apart are not joined; they are
    /// neighbours of neighbours. The caller decides what counts as
    /// touching, but distinct offsets must stay distinct here.
    #[test]
    fn distinct_offsets_stay_distinct() {
        let mut joins: Vec<Join> = (0..5)
            .map(|_| join("floor", "wall", (0.0, 3.5, 0.0), 0.0, 0.0))
            .collect();
        joins.extend((0..5).map(|_| join("floor", "wall", (3.0, 3.5, 0.0), 0.0, 0.0)));
        let studs = derive(&joins, Derive::default());
        assert_eq!(studs["floor"].len(), 2, "two places, two studs");
    }
}
