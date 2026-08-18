dofile( "$SURVIVAL_DATA/Scripts/game/worlds/BaseWorld.lua")
dofile( "$CONTENT_DATA/Scripts/survival_spawns_3x.lua" )
dofile( "$SURVIVAL_DATA/Scripts/game/managers/WaterManager.lua" )
dofile( "$SURVIVAL_DATA/Scripts/game/managers/PackingStationManager.lua" )
dofile( "$SURVIVAL_DATA/Scripts/game/managers/RaidManager.lua" )
dofile( "$GAME_DATA/Scripts/game/managers/WeatherManager.lua" )
dofile( "$SURVIVAL_DATA/Scripts/game/managers/AmbienceManager.lua" )

Overworld = class( BaseWorld )

Overworld.terrainScript = "$CONTENT_DATA/Scripts/terrain/terrain_overworld.lua"
Overworld.groundMaterialSet = "$GAME_DATA/Terrain/Materials/gnd_standard_materialset.json"
Overworld.voxelMaterialSet = "$SURVIVAL_DATA/Terrain/Materials/voxel_materialset_outdoor.voxelmaterialset"
Overworld.enableSurface = true
Overworld.enableAssets = true
Overworld.enableClutter = true
Overworld.enableNodes = true
Overworld.enableCreations = true
Overworld.enableHarvestables = true
Overworld.enableKinematics = true
Overworld.enableVoxelTerrain = true
Overworld.renderMode = "outdoor"
Overworld.cellMinX = -112
Overworld.cellMaxX = 111
Overworld.cellMinY = -96
Overworld.cellMaxY = 95
Overworld.horizonWater = true
Overworld.hLod = true

function Overworld.server_onCreate( self )
	print( "Overworld.server_onCreate" )
	BaseWorld.server_onCreate( self )
	self.world.publicData.type = "Overworld"
	self.world.publicData.voxelMaterialSetName = "Overworld"

	self.sv.ambienceManager = sm.scriptableObject.createScriptableObject( sm.uuid.new( "635b5d17-fa35-4591-82ec-358da595bac0" ), nil, self.world )

	self.packingStationManager = PackingStationManager()
	self.packingStationManager:sv_onCreate( self )

	self.foreignConnections = sm.storage.load( STORAGE_CHANNEL_FOREIGN_CONNECTIONS )
	if self.foreignConnections == nil then
		self.foreignConnections = {}
	end

	WorldManager.Sv_RegisterWorld( "overworld", self.world )
	WorldManager.Sv_RegisterWorld( "weatherworld", self.world )



	self.world:setTerrainScriptData( { filterVoxelTrianglesFromNavMesh = true } )


	MiningManager.Sv_SetupWorldMiningConfig( self.world )
end

function Overworld.server_onDestroy( self )
	if self.sv.ambienceManager then
		self.sv.ambienceManager:destroy()
		self.sv.ambienceManager = nil
	end
	BaseWorld.server_onDestroy( self )
end

function Overworld.client_onCreate( self )
	BaseWorld.client_onCreate( self )
	print( "Overworld.client_onCreate" )

	self.world.clientPublicData.isLiftPlacable = true
	self.world.clientPublicData.allowSoilPlacement = true
	self.world.clientPublicData.musicParameter = 3 -- daynoon
	self.world.clientPublicData.musicUpdateProcedure = function()
		local time = sm.game.getTimeOfDay()
		if time > DAYCYCLE_DAWN and time < DAYCYCLE_NOON then -- dawn
			return 2
		elseif time >= DAYCYCLE_NOON and time < DAYCYCLE_NIGHT then -- daynoon
			return 3
		else -- night
			return 4
		end
	end
	self.world.clientPublicData.type = "Overworld"
	self.world.clientPublicData.voxelMaterialSetName = "Overworld"
	self.world.clientPublicData.ambience = "OutdoorAmbience"
	self.world.clientPublicData.ambienceUpdateProcedure = function( ambienceEffect )
		if ambienceEffect and sm.exists( ambienceEffect ) then
			local night = 1.0 - getDayCycleFraction()
			ambienceEffect:setParameter( "amb_day_night", night )
		end
	end

	WorldManager.Cl_RegisterWorld( "overworld", self.world )
	WorldManager.Cl_RegisterWorld( "weatherworld", self.world )
	sm.event.sendToGame( "cl_e_overworldCreated", self.world )
end

function Overworld.client_onDestroy( self )
end

function Overworld.server_onRefresh( self )
end

function Overworld.server_onFixedUpdate( self )
	BaseWorld.server_onFixedUpdate( self )
	TornadoManager.Sv_OnWorldFixedUpdate( self.world )
	RaidManager.Sv_OnWorldFixedUpdate( self.world )
end

function Overworld.client_onFixedUpdate( self )
	BaseWorld.client_onFixedUpdate( self )
	TornadoManager.Cl_OnWorldFixedUpdate( self.world )
end

function Overworld.client_onUpdate( self, dt )
	BaseWorld.client_onUpdate( self, dt )
	RaidManager.Cl_OnWorldUpdate( self.world.id, dt )
end

function Overworld.sv_loadShipOrStationOnCell( self, x, y )
	local powerCoreSocketInteractables = sm.cell.getInteractablesByUuid( x, y, obj_survivalobject_powercoresocket )
	for _, powerCoreSocketInteractable in ipairs( powerCoreSocketInteractables ) do
		local lightInteractables = sm.cell.getInteractablesByUuid( x, y, obj_spaceship_ceilinglight )
		local shipworkbenchInteractables = sm.cell.getInteractablesByUuid( x, y, obj_survivalobject_workbench )
		local noteterminalInteractables = sm.cell.getInteractablesByUuid( x, y, obj_survivalobject_terminal )
		local dispenserbotInteractables = sm.cell.getInteractablesByUuid( x, y, obj_survivalobject_dispenserbot )

		powerCoreSocketInteractable:setParams( {
			lightInteractables = lightInteractables,
			shipworkbenchInteractables = shipworkbenchInteractables,
			noteterminalInteractables = noteterminalInteractables,
			dispenserbotInteractables = dispenserbotInteractables
		} )

		assert( #dispenserbotInteractables <= 1 )
		for _, dispenserbotInteractable in ipairs( dispenserbotInteractables ) do
			local dispenserbotSpawnerInteractables = sm.cell.getInteractablesByUuid( x, y, obj_survivalobject_dispenserbot_spawner )
			assert( #dispenserbotSpawnerInteractables == 1 )
			dispenserbotInteractable:setParams( { spawner = dispenserbotSpawnerInteractables[1] } )
		end
	end
end

function Overworld.sv_loadHideoutOnCell( self, x, y )
	local questGiverInteractables = sm.cell.getInteractablesByUuid( x, y, obj_hideout_questgiver )
	if #questGiverInteractables > 0 then
		local questGiverInteractable = questGiverInteractables[1]
		local buttonInteractables = sm.cell.getInteractablesByUuid( x, y, obj_hideout_button )
		local vacuumInteractables = sm.cell.getInteractablesByUuid( x, y, obj_hideout_dropoff )
		local cameraNodes = sm.cell.getNodesByTag( x, y, "CAMERA" )
		local dropzoneNodes = sm.cell.getNodesByTag( x, y, "HIDEOUT_DROPZONE" )
		if #vacuumInteractables > 0 and #buttonInteractables > 0 and #cameraNodes > 0 and #dropzoneNodes > 0 then
			questGiverInteractable:setParams( { vacuumInteractable = vacuumInteractables[1], buttonInteractable = buttonInteractables[1], cameraNode = cameraNodes[1], dropzoneNode = dropzoneNodes[1] } )
		else
			sm.log.info( "Failed to load hideout: ", #vacuumInteractables, " vacuums, ", #buttonInteractables, " buttons, ", #cameraNodes > 0, " cameras, ", #dropzoneNodes > 0, " dropzones." )
		end
	end
end

function Overworld.sv_loadSpawnersOnCell( self, x, y )
	local nodes = sm.cell.getNodesByTag( x, y, "PLAYER_SPAWN" )
	g_respawnManager:sv_addSpawners( nodes )
	g_respawnManager:sv_setLatestSpawners( nodes )
end

function Overworld.sv_reloadSpawnersOnCell( self, x, y )
	local nodes = sm.cell.getNodesByTag( x, y, "PLAYER_SPAWN" )
	g_respawnManager:sv_setLatestSpawners( nodes )
end

function Overworld.sv_spawnNewCharacter( self, params )
	local spawnPosition = g_survivalDev and SURVIVAL_DEV_SPAWN_POINT or START_AREA_SPAWN_POINT
	if params.fallbackPosition then
		spawnPosition = params.fallbackPosition
	end
	local yaw = 0
	local pitch = 0

	local nodes = sm.cell.getNodesByTag( params.x, params.y, "PLAYER_SPAWN" )
	if #nodes > 0 then
		local spawnerIndex = ( ( params.player.id - 1 ) % #nodes ) + 1
		local spawnNode = nodes[spawnerIndex]
		spawnPosition = spawnNode.position + sm.vec3.new( 0, 0, 0.7 )

		local spawnDirection = spawnNode.rotation * sm.vec3.new( 0, 0, 1 )
		--pitch = math.asin( spawnDirection.z )
		yaw = math.atan2( spawnDirection.y, spawnDirection.x ) - math.pi/2
	end

	local character = sm.character.createCharacter( params.player, self.world, spawnPosition, yaw, pitch )
	params.player:setCharacter( character )
end

-- World cell callbacks

function Overworld.server_onTerrainCreated( self )
	BaseWorld.server_onTerrainCreated( self )
	g_overworldTerrainLoaded = true
end

function Overworld.server_onTerrainLoaded( self )
	BaseWorld.server_onTerrainLoaded( self )
	g_overworldTerrainLoaded = true
end

function Overworld.client_onTerrainCreated( self )
	g_clientOverworldTerrainLoaded = true
end

function Overworld.client_onTerrainLoaded( self )
	g_clientOverworldTerrainLoaded = true
end

function Overworld.server_onCellCreated( self, x, y )
	BaseWorld.server_onCellCreated( self, x, y )
	ScannerbotManager.Sv_OnWorldCellLoaded( x, y )
	TornadoManager.Sv_OnCellLoaded( self.world, x, y )

	self:sv_loadSpawnersOnCell( x, y )

	local tags = sm.cell.getTags( x, y )

	if valueExists( tags, "WAREHOUSE4QUEST" ) then
		local objects = sm.cell.getInteractablesByTags( x, y, { "QUEST_WAREHOUSE_KEY_READER", "KEYCARD_READER" } )
		if #objects == 1 then
			objects[1]:setParams( { isQuestCardReader = true } )
		end
	end

	self:sv_loadHideoutOnCell( x, y )
	self:sv_loadShipOrStationOnCell( x, y )

	ElevatorManager.Sv_LoadElevatorsOnOverworldCell( x, y, tags )
	MinidungeonElevatorManager.Sv_LoadElevatorsOnOverworldCell( x, y, tags, self.world )

	self.packingStationManager:sv_onCellLoaded( x, y )
end

function Overworld.server_onCellLoaded( self, x, y )
	BaseWorld.server_onCellLoaded( self, x, y )
	ScannerbotManager.Sv_OnWorldCellLoaded( x, y )
	TornadoManager.Sv_OnCellLoaded( self.world, x, y )
	
	self:sv_reloadSpawnersOnCell( x, y )
end

function Overworld.server_onCellUnloaded( self, x, y )
	BaseWorld.server_onCellUnloaded( self, x, y )
	ScannerbotManager.Sv_OnWorldCellUnloaded( x, y )
	TornadoManager.Sv_OnCellUnloaded( x, y )
end

function Overworld.server_onProjectile( self, hitPos, hitTime, hitVelocity, _, attacker, damage, userData, hitNormal, target, projectileUuid )
	BaseWorld.server_onProjectile( self, hitPos, hitTime, hitVelocity, _, attacker, damage, userData, hitNormal, target, projectileUuid )
end



















