//! Game actions: what an actor is told to do this tick, whoever is
//! telling it. Quake 3's event system in our terms (topside
//! references.md "Input"): every input is one record, the platform
//! only produces them, the game only consumes them, a binding table
//! maps keys to actions so no game system reads a key, and a journal
//! of the records replays a session.
//!
//! Engine-agnostic: the consumer reads its engine's raw keys and
//! mouse in ONE place, runs them through [`Bindings`], and pushes
//! [`Action`]s onto each actor's [`ActionQueue`] stamped with the
//! fixed tick they apply to. An NPC's AI pushes the same actions.
//! Movement, fire, interaction, and the HUD drain the queue; none of
//! them knows what a key is.

use serde::{Deserialize, Serialize};

/// One thing an actor can be told to do.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// Walk: x is right, y is forward, each -1 to 1.
    Move { x: f32, y: f32 },
    /// Turn the view by this much, in radians: yaw then pitch.
    Look { yaw: f32, pitch: f32 },
    /// The trigger is held this tick.
    Attack,
    /// Use what is looked at (E).
    Use,
    /// Open or close the inventory.
    Inventory,
    /// Close whatever panel is open.
    Cancel,
    /// Use hotbar slot `index` (the number keys).
    Hotbar { index: usize },
    /// Drop the hovered stack (O).
    Drop,
    /// Transfer the hovered stack (T); `half` is Shift+T.
    Transfer { half: bool },
    /// Admin: toggle flying through everything (Quake's noclip, V).
    Fly,
}

/// A key or button, by the name a binding file uses. The consumer
/// maps its engine's codes to these names once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    W,
    A,
    S,
    D,
    E,
    I,
    O,
    T,
    V,
    Tab,
    Escape,
    ShiftLeft,
    ShiftRight,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Digit0,
    MouseLeft,
    MouseRight,
}

/// What a key is bound to. Held keys repeat their action every tick
/// while down (move, attack); tapped keys fire once on press.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Binding {
    Held(Action),
    Tapped(Action),
}

/// Key to action, as data. Quake 3's `bind` table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bindings {
    pub bindings: Vec<(Key, Binding)>,
}

impl Bindings {
    /// The stock layout: WASD, mouse to attack, E to use, I or Tab
    /// for the inventory, Escape to cancel, digits for the hotbar,
    /// O to drop, T to transfer.
    pub fn defaults() -> Self {
        use Action::*;
        use Binding::*;
        Self {
            bindings: vec![
                (Key::W, Held(Move { x: 0.0, y: 1.0 })),
                (Key::S, Held(Move { x: 0.0, y: -1.0 })),
                (Key::A, Held(Move { x: -1.0, y: 0.0 })),
                (Key::D, Held(Move { x: 1.0, y: 0.0 })),
                (Key::MouseLeft, Held(Attack)),
                (Key::E, Tapped(Use)),
                (Key::I, Tapped(Inventory)),
                (Key::Tab, Tapped(Inventory)),
                (Key::Escape, Tapped(Cancel)),
                (Key::V, Tapped(Fly)),
                (Key::Digit1, Tapped(Hotbar { index: 0 })),
                (Key::Digit2, Tapped(Hotbar { index: 1 })),
                (Key::Digit3, Tapped(Hotbar { index: 2 })),
                (Key::Digit4, Tapped(Hotbar { index: 3 })),
                (Key::Digit5, Tapped(Hotbar { index: 4 })),
                (Key::Digit6, Tapped(Hotbar { index: 5 })),
                (Key::Digit7, Tapped(Hotbar { index: 6 })),
                (Key::Digit8, Tapped(Hotbar { index: 7 })),
                (Key::Digit9, Tapped(Hotbar { index: 8 })),
                (Key::Digit0, Tapped(Hotbar { index: 9 })),
                (Key::O, Tapped(Drop)),
                (Key::T, Tapped(Transfer { half: false })),
            ],
        }
    }

    pub fn bind(&mut self, key: Key, binding: Binding) {
        self.bindings.retain(|(k, _)| *k != key);
        self.bindings.push((key, binding));
    }

    pub fn binding(&self, key: Key) -> Option<Binding> {
        self.bindings.iter().find(|(k, _)| *k == key).map(|(_, b)| *b)
    }

    /// Turn one frame of raw key state into actions. `pressed` are
    /// the keys down now, `just_pressed` the ones that went down
    /// this frame. Held bindings fire while down; tapped ones fire
    /// on the press. Opposite move keys cancel: the moves are
    /// summed into one `Move`.
    pub fn resolve(&self, pressed: &[Key], just_pressed: &[Key]) -> Vec<Action> {
        let mut out = Vec::new();
        let mut move_sum: Option<(f32, f32)> = None;
        let shift = pressed.contains(&Key::ShiftLeft) || pressed.contains(&Key::ShiftRight);
        for (key, binding) in &self.bindings {
            let fires = match binding {
                Binding::Held(_) => pressed.contains(key),
                Binding::Tapped(_) => just_pressed.contains(key),
            };
            if !fires {
                continue;
            }
            match binding {
                Binding::Held(Action::Move { x, y }) | Binding::Tapped(Action::Move { x, y }) => {
                    let (sx, sy) = move_sum.unwrap_or((0.0, 0.0));
                    move_sum = Some((sx + x, sy + y));
                }
                Binding::Tapped(Action::Transfer { .. }) => {
                    out.push(Action::Transfer { half: shift });
                }
                Binding::Held(a) | Binding::Tapped(a) => out.push(*a),
            }
        }
        if let Some((x, y)) = move_sum {
            if x != 0.0 || y != 0.0 {
                out.push(Action::Move {
                    x: x.clamp(-1.0, 1.0),
                    y: y.clamp(-1.0, 1.0),
                });
            }
        }
        out
    }
}

/// One action stamped with the fixed tick it applies to (Quake 3's
/// `evTime`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stamped {
    pub tick: u64,
    pub action: Action,
}

/// An actor's pending actions. The producer (keyboard or AI) pushes;
/// the systems drain what applies to the current tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActionQueue {
    pending: Vec<Stamped>,
}

impl ActionQueue {
    pub fn push(&mut self, tick: u64, action: Action) {
        self.pending.push(Stamped { tick, action });
    }

    /// Everything stamped at or before `tick`, in order, removed.
    pub fn drain_through(&mut self, tick: u64) -> Vec<Stamped> {
        let (due, later): (Vec<_>, Vec<_>) = self.pending.drain(..).partition(|s| s.tick <= tick);
        self.pending = later;
        due
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// The net movement this tick: the last `Move`, or standing still.
    pub fn movement(actions: &[Stamped]) -> (f32, f32) {
        actions
            .iter()
            .rev()
            .find_map(|s| match s.action {
                Action::Move { x, y } => Some((x, y)),
                _ => None,
            })
            .unwrap_or((0.0, 0.0))
    }

    /// The summed look delta this tick.
    pub fn look(actions: &[Stamped]) -> (f32, f32) {
        actions.iter().fold((0.0, 0.0), |(y, p), s| match s.action {
            Action::Look { yaw, pitch } => (y + yaw, p + pitch),
            _ => (y, p),
        })
    }

    pub fn has(actions: &[Stamped], action: Action) -> bool {
        actions.iter().any(|s| s.action == action)
    }
}

/// A session on disk: the world seed and every action in order.
/// Replaying it through the same fixed step reproduces the session
/// (Quake 3's `journal.dat`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Journal {
    pub seed: u64,
    pub actions: Vec<Stamped>,
}

impl Journal {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            actions: Vec::new(),
        }
    }

    pub fn record(&mut self, stamped: Stamped) {
        self.actions.push(stamped);
    }

    /// The recorded actions for one tick.
    pub fn at(&self, tick: u64) -> impl Iterator<Item = &Stamped> {
        self.actions.iter().filter(move |s| s.tick == tick)
    }

    pub fn last_tick(&self) -> Option<u64> {
        self.actions.iter().map(|s| s.tick).max()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a journal serialises")
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_keys_repeat_and_tapped_keys_fire_once() {
        let b = Bindings::defaults();
        let frame1 = b.resolve(&[Key::W, Key::MouseLeft, Key::E], &[Key::E]);
        assert!(frame1.contains(&Action::Attack));
        assert!(frame1.contains(&Action::Use));
        assert!(frame1.contains(&Action::Move { x: 0.0, y: 1.0 }));
        // Next frame, E still down but not just pressed: no Use.
        let frame2 = b.resolve(&[Key::W, Key::MouseLeft, Key::E], &[]);
        assert!(frame2.contains(&Action::Attack));
        assert!(!frame2.contains(&Action::Use));
    }

    #[test]
    fn opposite_move_keys_cancel_and_diagonals_sum() {
        let b = Bindings::defaults();
        assert!(b.resolve(&[Key::W, Key::S], &[]).is_empty(), "W and S cancel");
        let diagonal = b.resolve(&[Key::W, Key::D], &[]);
        assert_eq!(diagonal, vec![Action::Move { x: 1.0, y: 1.0 }]);
    }

    #[test]
    fn shift_t_transfers_half_and_keys_can_be_rebound() {
        let mut b = Bindings::defaults();
        assert_eq!(b.resolve(&[Key::T], &[Key::T]), vec![Action::Transfer { half: false }]);
        assert_eq!(
            b.resolve(&[Key::T, Key::ShiftLeft], &[Key::T]),
            vec![Action::Transfer { half: true }]
        );
        b.bind(Key::MouseRight, Binding::Held(Action::Attack));
        b.bind(Key::MouseLeft, Binding::Tapped(Action::Use));
        let r = b.resolve(&[Key::MouseRight, Key::MouseLeft], &[Key::MouseLeft]);
        assert!(r.contains(&Action::Attack) && r.contains(&Action::Use));
        assert_eq!(b.binding(Key::MouseLeft), Some(Binding::Tapped(Action::Use)));
    }

    #[test]
    fn the_queue_hands_out_what_is_due_by_tick() {
        let mut q = ActionQueue::default();
        q.push(5, Action::Attack);
        q.push(6, Action::Use);
        q.push(5, Action::Move { x: 0.0, y: 1.0 });
        let due = q.drain_through(5);
        assert_eq!(due.len(), 2);
        assert!(ActionQueue::has(&due, Action::Attack));
        assert_eq!(ActionQueue::movement(&due), (0.0, 1.0));
        assert!(!q.is_empty());
        let later = q.drain_through(6);
        assert_eq!(later[0].action, Action::Use);
        assert!(q.is_empty());
    }

    #[test]
    fn look_deltas_sum_within_a_tick() {
        let due = vec![
            Stamped {
                tick: 1,
                action: Action::Look {
                    yaw: 0.1,
                    pitch: -0.05,
                },
            },
            Stamped {
                tick: 1,
                action: Action::Look {
                    yaw: 0.2,
                    pitch: 0.0,
                },
            },
        ];
        let (yaw, pitch) = ActionQueue::look(&due);
        assert!((yaw - 0.3).abs() < 1e-6 && (pitch + 0.05).abs() < 1e-6);
    }

    #[test]
    fn a_journal_round_trips_and_replays_by_tick() {
        let mut j = Journal::new(42);
        j.record(Stamped {
            tick: 1,
            action: Action::Move { x: 0.0, y: 1.0 },
        });
        j.record(Stamped {
            tick: 3,
            action: Action::Attack,
        });
        let back = Journal::from_json(&j.to_json()).unwrap();
        assert_eq!(back, j);
        assert_eq!(back.seed, 42);
        assert_eq!(back.at(3).count(), 1);
        assert_eq!(back.at(2).count(), 0);
        assert_eq!(back.last_tick(), Some(3));
        assert!(Journal::from_json("nope").is_err());
    }
}
