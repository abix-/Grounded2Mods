# Bot navigation

> **Authoritative on:** how a Modforge bot selects a waypoint, finds a
> walkable path to it, and travels that path using the same input as a player.
> Approved words come from [the repository terminology](../../docs/terminology.md).

## Goal

The bot can travel through a 3D game without recording the entire trip and
without moving the player directly.

For the first MISERY trip, the route is:

```text
spawn
  -> metal-door [stop; press E if closed]
  -> expedition-door [stop; press E and confirm expedition entry]
```

These are three waypoints and two travel steps. A waypoint is a place where
the bot must stop. Intermediate turns are path points, not waypoints.

## How travel works

There is one pathfinding level:

```text
the route selects the next waypoint
  -> Unreal navigation's A* searches from the player to that waypoint
  -> Unreal returns the detailed path points
  -> the bot compares the path with the player's position and view
  -> the bot sends virtual W/A/S/D and mouse movement
  -> the game's normal input bindings move and aim the player
  -> the bot observes the result and sends the next input
```

A* searches the game's navigation mesh. Its start is the player's current
position and its goal is the selected waypoint. Waypoints are not A* search
nodes. The bot does not choose or alter the path returned by A*.

If the path becomes blocked, the bot asks A* for a new path from the player's
current position to the same waypoint. A waypoint is usable only when Unreal
returns a valid, complete path to it. A straight line never proves that a
target is reachable.

The bot may record debug positions showing where it actually walked. Debug
positions are evidence only. They are never used for navigation.

## Player input

`InputSurface`, the game-specific player-input adapter, injects virtual
W/A/S/D state, relative mouse movement, and `E` press and release into the
game's normal input processing. The game applies its existing bindings and
performs movement, aiming, and interaction exactly as it does for a player.

The bot releases every held key when it arrives, stops, fails, or is cancelled.
It does not require the physical mouse, window focus, or OS-wide keyboard input.

The following are forbidden because they bypass the player's input route:

- `SimpleMoveToLocation` or another navigation call that moves the player.
- `AddMovementInput`, `AddYawInput`, or `AddPitchInput`.
- Direct calls to MISERY's interaction or Enhanced Input action handlers.
- Direct player location, rotation, transform, or velocity writes.
- Physical mouse capture, cursor movement, window focus, or OS-wide input.

Reading player position, view rotation, door state, interaction range, and
completion state is allowed. Those observations decide the next player input
but never replace it.

## Doors

At the metal door, the bot stops and aims at the interaction point using
virtual mouse movement. When that point is in range, the bot observes whether
the door is closed. It sends one virtual `E` press and release only when the
door needs to open. Travel resumes only after the bot observes that passage is
available.

At the expedition door, the bot stops, aims through virtual mouse movement,
and sends one virtual `E` press and release. Reaching the waypoint is not
success. Success requires observing that the player entered the expedition.

## Live loot target

A discovered loot box is the next waypoint. The bot asks Unreal navigation for
a path to each candidate, rejects invalid or partial paths, and may choose the
reachable box with the lowest returned path cost. Unreal A* then supplies the
path to the selected box. The bot travels it using player input, aims at the
box, presses `E`, and loots through the game's real inventory UI.

## Required proof

A restarted MISERY run must:

1. Select the metal-door waypoint.
2. Use Unreal A* to find a complete path from the player to the door.
3. Travel that path using only virtual W/A/S/D and mouse movement.
4. Open the metal door with virtual `E` only when it is closed.
5. Repeat the same process for the expedition door and observe entry.
6. Find a reachable loot box, make it the waypoint, travel to it, open it, and
   loot it through player input.
7. Release all input on arrival and failure.
8. Meet the limits in [the MISERY performance design](../../misery-mod/docs/performance.md).

