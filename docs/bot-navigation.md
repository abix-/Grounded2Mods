# Bot navigation

> **Authoritative on:** the one Modforge bot-navigation system used by both
> Unreal and Unity games. It defines how the bot selects a waypoint, finds a
> walkable path to it, and travels that path using the same input as a player.
> Approved words come from [the repository terminology](terminology.md).

## Goal

Modforge owns one engine-independent bot-navigation system. Ueforge and
Unityforge connect that system to their engines. They do not implement separate
Unreal and Unity bots.

The shared system lets a bot travel through a 3D game without recording the
entire trip and without moving the player directly. MISERY is the first Unreal
proof. A Unity game must use the same route, waypoint, path, bot, and player
input behavior through Unityforge.

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
  -> Modforge asks the game engine for a path to that waypoint
  -> the engine's A* returns the detailed path points
  -> the shared bot code compares the path with player position and view
  -> the shared bot code chooses virtual W/A/S/D and mouse movement
  -> Ueforge or Unityforge injects that player input
  -> the game's normal input bindings move and aim the player
  -> the bot observes the result and sends the next input
```

A* has one job: search the game's navigation data from the player's current
position to the selected waypoint and return one shared path format. It runs
when travel starts and runs again if the path becomes blocked. Waypoints are
not A* search nodes. The bot does not choose or alter the path returned by A*.

Unreal navigation and Unity navigation answer the same Modforge path request.
That engine call is the only part that differs. Everything that reads the path,
decides W/A/S/D and mouse movement, detects arrival or failure, and releases
input is shared Modforge code.

If the path becomes blocked, the bot asks A* for a new path from the player's
current position to the same waypoint. A waypoint is usable only when the game
engine returns a valid, complete path to it. A straight line never proves that
a target is reachable.

The bot may record debug positions showing where it actually walked. Debug
positions are evidence only. They are never used for navigation.

## Player input

The shared bot code produces virtual W/A/S/D state, relative mouse movement,
and key press and release. `InputSurface`, the player-input code implemented by
Ueforge or Unityforge, injects those controls into the game's normal input processing.
The game applies its existing bindings and performs movement, aiming, and
interaction exactly as it does for a player.

| Need | Unreal through Ueforge | Unity through Unityforge |
|---|---|---|
| Find a path | Ask Unreal navigation for a complete path and return its path points | Ask Unity navigation for a complete path and return its path corners as path points |
| Observe the player | Return current position and view | Return current position and view |
| Move and aim | Inject virtual W/A/S/D and relative mouse movement into Unreal player input | Inject the same virtual W/A/S/D and relative mouse movement into Unity player input |
| Interact | Inject the bound key press and release | Inject the same bound key press and release |

Both engines must return the same Modforge path and player-observation formats.
Both engines must accept the same Modforge player-input commands. Game-specific
code may translate data and input, but may not decide the route, steer the bot,
or perform the gameplay action directly.

The bot releases every held key when it arrives, stops, fails, or is cancelled.
It does not require the physical mouse, window focus, or OS-wide keyboard input.

The following are forbidden because they bypass the player's input route:

- Unreal `SimpleMoveToLocation`, `AddMovementInput`, `AddYawInput`, or
  `AddPitchInput`.
- Unity `NavMeshAgent.SetDestination`, `CharacterController.Move`, direct
  Rigidbody movement, or another call that moves the player.
- Direct calls to a game's interaction method or input-action handler.
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

A discovered loot box is the next waypoint. The bot asks the game engine for a
path to each candidate, rejects invalid or partial paths, and may choose the
reachable box with the lowest returned path cost. The engine's A* then supplies
the path to the selected box. The shared bot code travels it using player
input, aims at the box, presses the bound interaction key, and loots through
the game's real inventory UI.

## Required proof

The shared Modforge tests must give the bot the same path, player position, and
view through test Ueforge and Unityforge implementations. Both must produce the same
ordered W/A/S/D, mouse, interaction, and input-release commands.

### Unreal proof

A restarted MISERY run must:

1. Select the metal-door waypoint.
2. Use Unreal A* to find a complete path from the player to the door.
3. Travel that path using only virtual W/A/S/D and mouse movement.
4. Open the metal door with virtual `E` only when it is closed.
5. Repeat the same process for the expedition door and observe entry.
6. Find a reachable loot box, make it the waypoint, travel to it, open it, and
   loot it through player input.
7. Release all input on arrival and failure.
8. Meet the limits in [the MISERY performance design](../misery-mod/docs/performance.md).

### Unity proof

A Unity game must run the same shared bot code through Unityforge. One live run
must select a waypoint, receive a complete Unity navigation path, travel it
using virtual W/A/S/D and mouse movement, perform one bound interaction, observe
success, and release all input. No Unity movement, rotation, or interaction
method may be called directly.
