# fish-mod research: type system map

Probed from live game via walk_class + inspect_object on port 17174.
Game: How to Fish (Unity 6, Mono, FishNet networking, Steam AppID 4001890).

## found classes (14 of 39 candidates)

| Class | Instances | GameObject | Key fields |
|---|---:|---|---|
| AudioManager | 1 | SceneManager | _audioSourcePrefab, _volume, _mainMixer, _fxGroup, _islandAmbientSource, _seaAmbientSource |
| SpawnManager | 1 | SpawnManager | _playerSpawnPoint, _boatPrefab, _boatSpawnPoint |
| IslandManager | 1 | SceneManager | _clientManager, _nearPlayerRange (15.0), _islandInfos[], _scenes[], _curIsland (Byte) |
| Boat | 1 | Boat(Clone) | FishNet NetworkBehaviour. _motor, _steering, _vel, _anchor. Has Rigidbody, NetworkObject |
| BoatInteractable | 1 | Boat(Clone) | Attached to Boat object |
| SlotMachine | 1 | SlotMachine(Clone) | FishNet NetworkBehaviour |
| EndGameToggles | 1 | SceneManager | Attached to SceneManager |
| InventorySlot | 4 | InventorySlot | UI element for player inventory display |
| Player | 1 | PlayerHolder(Clone) | See Player detail below |
| FishingRod | 1 | Crab Fishing Rod(Clone) | See FishingRod detail below. Item subtype with DefaultBait, Bait, SwayTransform |
| Item | 25 | various | Base class for holdable objects. ID (Byte), DefaultWorth, TotalWorth, Holder, Cookness, SkinPreset. Example: RockCrab (ID=79, HP=70, DefaultWorth=7) |
| SaveManager | 1 | SceneManager | MonoBehaviour, minimal fields |
| ServerManager | 1 | NetworkManager | FishNet.Managing.Server.ServerManager. Clients dict, _authenticator (SteamLobbyAuthenticator), _syncTypeRate (0.1), _frameRate (500), Started=true |
| ClientManager | 1 | NetworkManager | FishNet.Managing.Client.ClientManager. Connection, _frameRate (500), Started=true |

## not found (25 candidates)

SceneManager (the game's own, not FishNet's), PlayerHolder, LocalPlayer, VisualPhysicsBoat,
Seagull, Clam, ClamSpawner, SeagullSpawner, Knife, ItemDot, HitMarkerUI, CanvasText,
CharacterInteractable, LocalUI, Chat, PlayerController, Fish, FishSpawner, Inventory,
Shop, Weather, DayNightCycle, Quest, QuestManager, LobbyManager.

These may use different class names, be inactive, or only exist in certain game states.
The MonoBehaviour walk from the previous session found many of these as object names
(Seagull, Clam, Knife, ItemDot, HitMarkerUI, Chat, Fish objects) so they exist as
instances but their actual type names differ from the guessed candidate names.

## Player detail

The Player class (on the PlayerHolder(Clone) GameObject) is the central hub.
All subsystems are component references on the same GameObject:

- PlayerMovement: locomotion
- PlayerInventory: inventory management
- PlayerHolding: held item logic
- PlayerUI: HUD
- PlayerTutorial: tutorial state
- PlayerCamera: camera control
- OtherPlayer: other player representation
- PlayerToolMovement: tool sway/bob
- PlayerPunching: melee
- PlayerHands: hand models
- PlayerVitals: health/hunger
- PlayerScreenShake: screen effects
- PlayerArms: arm models
- PlayerSkills: skill progression
- PlayerBody: body model
- PlayerLegs: leg model
- PlayerMouth: eating animation
- PlayerSkin: skin customization
- PlayerDying: death state
- PlayerEating: eating state
- PlayerColDetector: collision
- PlayerEffects: visual effects
- PlayerThinking: thought bubbles
- PlayerAimAssist: aim assist
- PlayerUnderwater: underwater state
- PlayerKillScore: kill tracking
- PlayerDeathCam: death camera

Other Player fields: SteamName ("Abix"), _steamID (SyncVar), _isCrouching (SyncVar),
_rigidbody, _transform (LocalPlayer), CamObject (Camera transform), CurCam (Camera).

## FishingRod detail

FishingRod is an Item subtype (Crab Fishing Rod, ID=59, DefaultWorth=3).
Key fields beyond Item base:
- DefaultBait: BaitInfo ("Default Crab Bait")
- Bait: Bait component
- FishingRodCrab: concrete Tool type
- SwayTransform, SwayRotAroundTransform (RodBase)
- HandsMesh (SkinnedMeshRenderer)
- TiltAmount, SwayPosForce, SwayRotForce, FallForce, MaxSwayPos, MaxSwayRot
- CanLookAround, MaxLookAmount, LookSpeed

## Item system

Items are FishNet NetworkBehaviours. Each has:
- ID (Byte): unique item type identifier
- Holder/SyncedHolder/LastHolder: Player references
- DefaultWorth/TotalWorth (Int32): sell value
- Cookness (Single): cooking state 0.0 to 1.0
- RandomizedWeight (Single): weight variance
- IsInInventory, IsInteractable, CanPickUp (Boolean)
- Type (ItemType enum, seen: "Item")
- Buoyancy (Single): water physics
- Optional component refs: Tool, Weapon, Melee, Fish, Bird, Creature, Explosive, DeadPlayer, FishingRod, Radio

Creature subclass (seen: Crab) adds:
- Hp/_localHp/MaxHp: health (70 for RockCrab)
- IsDead, IsEndangered, BossType
- FullnessToRestore (40), HpToRestore (25): nutrition when eaten
- Movement: _moveSpeed (2.5), _jumpVel (2.5), _moveAcceleration (20.0)
- _walkSideways (true for crabs)

## networking

All gameplay objects use FishNet. Every Item, Player, Boat etc. inherits NetworkBehaviour.
Properties always present: IsHost, IsClient, IsServer, IsSpawned, IsOwner, IsController,
HasAuthority, ObjectId, OwnerId, NetworkManager, ServerManager, ClientManager.

The server uses SteamLobbyAuthenticator. Both server and client run at 500 fps frame rate.
RemoteTimeoutType is "Development" with 60s timeout on both sides.

## next steps

- Probe the 25 not-found candidates using walk_class("MonoBehaviour") output to find
  their actual type names
- Inspect PlayerMovement, PlayerInventory, PlayerVitals, PlayerSkills for mod targets
- Inspect Boat in detail for sailing mechanics
- Map the Item ID table (which Byte ID maps to which item)
- Find the Shop and Quest systems (may be different class names)
