//! What a person knows (topside life.md "What a person knows"):
//! things seen, who hurt them, the last seen threat. The brain only
//! considers what is in here; perception writes it; nothing is
//! broadcast (operator: no advertising, "things describe what they
//! are good for and people remember what they saw"). Engine-free:
//! glam positions and plain ids.

use glam::Vec3;

use crate::actor::ActorId;
use crate::survival::Need;

/// What a kind of thing is good for: the needs it can satisfy and
/// by how much, as a field on its def (an item's `good_for`, a
/// monument type's, the bunker's). A box of food is good for hunger;
/// a camp for rest and safety.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GoodFor {
    pub needs: Vec<(Need, f32)>,
}

impl GoodFor {
    pub fn new(needs: &[(Need, f32)]) -> Self {
        Self {
            needs: needs.to_vec(),
        }
    }

    /// How much this satisfies `need`, zero if it does not.
    pub fn satisfies(&self, need: Need) -> f32 {
        self.needs
            .iter()
            .find(|(n, _)| *n == need)
            .map_or(0.0, |(_, v)| *v)
    }
}

/// A thing the person knows of: what kind, where, when, and what
/// was seen inside it. `key` is the consumer's handle for it (an
/// entity index, a site index), so the brain can hand it back;
/// `kind` names the def, and the registry says what that kind does.
/// No copy of the def's numbers lives here (life.md "What things are
/// good for": memory stores only what it saw).
#[derive(Clone, Debug, PartialEq)]
pub struct Known {
    pub key: u64,
    pub kind: String,
    pub position: Vec3,
    /// Tick last seen.
    pub seen_at: u64,
    /// Tick last checked up close (looted, opened), if ever.
    pub checked_at: Option<u64>,
    /// The kinds of stacks seen inside it when last checked; None
    /// until looked (believed to hold something), empty when it was
    /// found bare.
    pub held: Option<Vec<String>>,
}

impl Known {
    /// Never looked inside, or found holding something.
    pub fn believed_to_hold(&self) -> bool {
        self.held.as_ref().is_none_or(|h| !h.is_empty())
    }

    /// Seen holding this kind of stack.
    pub fn held_kind(&self, kind: &str) -> bool {
        self.held.as_ref().is_some_and(|h| h.iter().any(|k| k == kind))
    }
}

/// A person's memory: things seen, grudges, the last threat.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Memory {
    pub known: Vec<Known>,
    /// Who hurt me, and how much, most recent last.
    pub hurt_by: Vec<(ActorId, f32)>,
    /// The last hostile seen: who, where, when.
    pub last_threat: Option<(ActorId, Vec3, u64)>,
}

/// Memories older than this are forgotten (ticks at 60 a second: a
/// game day of 1200 s is 72000 ticks; a thing is remembered for
/// three days).
pub const FORGET_AFTER: u64 = 3 * 72_000;

impl Memory {
    /// Note a thing seen now. An already known thing is refreshed
    /// (its seen tick and position), never duplicated.
    pub fn see(&mut self, key: u64, kind: &str, position: Vec3, now: u64) {
        match self.known.iter_mut().find(|k| k.key == key) {
            Some(k) => {
                k.seen_at = now;
                k.position = position;
            }
            None => self.known.push(Known {
                key,
                kind: kind.to_string(),
                position,
                seen_at: now,
                checked_at: None,
                held: None,
            }),
        }
    }

    /// Note a thing checked up close now: the kinds of stacks found
    /// inside it (empty when bare).
    pub fn checked(&mut self, key: u64, held: Vec<String>, now: u64) {
        if let Some(k) = self.known.iter_mut().find(|k| k.key == key) {
            k.checked_at = Some(now);
            k.seen_at = now;
            k.held = Some(held);
        }
    }

    /// A thing is gone (despawned): forget it.
    pub fn gone(&mut self, key: u64) {
        self.known.retain(|k| k.key != key);
    }

    pub fn hurt(&mut self, by: ActorId, amount: f32) {
        self.hurt_by.push((by, amount));
        if self.hurt_by.len() > 16 {
            self.hurt_by.remove(0);
        }
    }

    pub fn threat(&mut self, who: ActorId, at: Vec3, now: u64) {
        self.last_threat = Some((who, at, now));
    }

    /// Drop what is too old to trust.
    pub fn forget_old(&mut self, now: u64) {
        self.known.retain(|k| now.saturating_sub(k.seen_at) < FORGET_AFTER);
        if let Some((_, _, when)) = self.last_threat
            && now.saturating_sub(when) >= FORGET_AFTER
        {
            self.last_threat = None;
        }
    }

    /// Known things worth something for `need` and believed to still
    /// hold something, with what they give. `worth` is the registry's
    /// answer for a remembered thing (the brain never reads the
    /// world, and memory never copies a def).
    pub fn good_for<'a>(
        &'a self,
        need: Need,
        worth: &'a dyn Fn(&Known, Need) -> f32,
    ) -> impl Iterator<Item = (&'a Known, f32)> + 'a {
        self.known
            .iter()
            .filter(|k| k.believed_to_hold())
            .filter_map(move |k| {
                let v = worth(k, need);
                (v > 0.0).then_some((k, v))
            })
    }

    /// Known things never checked up close, nearest to `from` first:
    /// where to go looking.
    pub fn unchecked_nearest(&self, from: Vec3) -> Option<&Known> {
        self.known
            .iter()
            .filter(|k| k.checked_at.is_none())
            .min_by(|a, b| {
                a.position
                    .distance(from)
                    .total_cmp(&b.position.distance(from))
            })
    }

    pub fn knows(&self, key: u64) -> bool {
        self.known.iter().any(|k| k.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry's answer in these tests: a storage box is worth
    /// 50 for hunger, nothing else is worth anything.
    fn worth(known: &Known, need: Need) -> f32 {
        if known.kind == "storage box" && need == Need::Hunger {
            50.0
        } else {
            0.0
        }
    }

    fn kinds(kinds: &[&str]) -> Vec<String> {
        kinds.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn seeing_twice_refreshes_and_never_duplicates() {
        let mut m = Memory::default();
        m.see(7, "storage box", Vec3::new(1.0, 0.0, 0.0), 10);
        m.see(7, "storage box", Vec3::new(2.0, 0.0, 0.0), 20);
        assert_eq!(m.known.len(), 1);
        assert_eq!(m.known[0].seen_at, 20);
        assert_eq!(m.known[0].position.x, 2.0);
        assert!(m.knows(7) && !m.knows(8));
    }

    #[test]
    fn a_checked_box_is_remembered_by_what_it_held() {
        let mut m = Memory::default();
        m.see(7, "storage box", Vec3::ZERO, 10);
        assert!(m.known[0].believed_to_hold(), "never looked: believed full");
        assert!(!m.known[0].held_kind("canned food"), "not seen yet");
        m.checked(7, kinds(&["canned food", "canned food", "pipe"]), 30);
        let box_ = &m.known[0];
        assert_eq!(box_.checked_at, Some(30));
        assert!(box_.held_kind("canned food") && box_.held_kind("pipe"));
        assert!(!box_.held_kind("cloth"));
        assert!(box_.believed_to_hold());
    }

    #[test]
    fn checked_empty_things_are_not_good_for_anything_until_seen_full_again() {
        let mut m = Memory::default();
        m.see(7, "storage box", Vec3::ZERO, 10);
        assert_eq!(m.good_for(Need::Hunger, &worth).count(), 1);
        assert_eq!(m.good_for(Need::Rest, &worth).count(), 0, "a box is not a bed");
        m.checked(7, vec![], 30);
        assert_eq!(m.good_for(Need::Hunger, &worth).count(), 0, "known empty");
        assert!(m.unchecked_nearest(Vec3::ZERO).is_none(), "checked, so not a place to look");
        m.checked(7, kinds(&["canned food"]), 40);
        assert_eq!(m.good_for(Need::Hunger, &worth).count(), 1);
    }

    #[test]
    fn looking_goes_to_the_nearest_unchecked_thing() {
        let mut m = Memory::default();
        m.see(1, "wreck", Vec3::new(50.0, 0.0, 0.0), 1);
        m.see(2, "wreck", Vec3::new(10.0, 0.0, 0.0), 1);
        m.see(3, "wreck", Vec3::new(5.0, 0.0, 0.0), 1);
        m.checked(3, vec![], 2);
        assert_eq!(m.unchecked_nearest(Vec3::ZERO).unwrap().key, 2);
    }

    #[test]
    fn old_things_and_threats_are_forgotten_and_grudges_are_kept_short() {
        let mut m = Memory::default();
        m.see(1, "wreck", Vec3::ZERO, 0);
        m.threat(ActorId(9), Vec3::ZERO, 0);
        m.forget_old(FORGET_AFTER - 1);
        assert_eq!(m.known.len(), 1);
        assert!(m.last_threat.is_some());
        m.forget_old(FORGET_AFTER);
        assert!(m.known.is_empty());
        assert!(m.last_threat.is_none());
        for i in 0..20 {
            m.hurt(ActorId(i), 1.0);
        }
        assert_eq!(m.hurt_by.len(), 16);
        assert_eq!(m.hurt_by.last().unwrap().0, ActorId(19));
        m.gone(1);
    }
}
