
-- Ticks between node respawns
local DefaultTicksBetweenRespawns = {
	["HAYBOT"] = DaysInTicks( 10 ),
	["TOTEBOT_GREEN"] = DaysInTicks( 10 ),
	["TOTEBOT_LEAF"] = DaysInTicks( 10 ),
	["TOTEBOT_BLUE"] = DaysInTicks( 10 ),
	["TOTEBOT_RED"] = DaysInTicks( 10 ),
	["TOTEBOT_YELLOW"] = DaysInTicks( 10 ),
	["SEEDBOT"] = DaysInTicks( 10 ),
	["TAPEBOT"] = DaysInTicks( 10 ),
	["TAPEBOT_GREEN"] = DaysInTicks( 10 ),
	["TAPEBOT_YELLOW"] = DaysInTicks( 10 ),
	["TAPEBOT_RED"] = DaysInTicks( 10 ),
	["FARMBOT"] = DaysInTicks( 10 ),
	["WOC"] = DaysInTicks( 10 ),
	["GLOWGORP"] = DaysInTicks( 10 ),
	["CABLEBOT"] = DaysInTicks( 10 ),
	["MINERBOT"] = DaysInTicks( 10 ),
	["LOOTCRATE"] = DaysInTicks( 10 ),
	["EPICLOOTCRATE"] = DaysInTicks( 10 ),
	["LEGENDARYLOOTCRATE"] = DaysInTicks( 10 ),
}

local TicksBetweenRespawns = {
	["Overworld"] = DefaultTicksBetweenRespawns,
	["DungeonWorld"] = {},
	["WarehouseWorld"] = {},
	["UndergroundWorld"] = DefaultTicksBetweenRespawns,
	["DrillbotWorld"] = {},
}

local function GetTicksBetweenRespawns( tag )
	local pubData = sm.world.getCurrentWorld().publicData
	assert( pubData )
	local worldSpecifics = TicksBetweenRespawns[pubData.type] or {}
	return worldSpecifics[tag]
end


local OverworldPoiSpawnChances = {
	["HAYBOT"] = 33,
	["TOTEBOT_GREEN"] = 33,
	["TOTEBOT_LEAF"] = 33,
	["TOTEBOT_BLUE"] = 33,
	["TOTEBOT_RED"] = 33,
	["TOTEBOT_YELLOW"] = 33,
	["SEEDBOT"] = 20,
	["TAPEBOT"] = 33,
	["TAPEBOT_GREEN"] = 33,
	["TAPEBOT_YELLOW"] = 33,
	["TAPEBOT_RED"] = 33,
	["FARMBOT"] = 33,
	["WOC"] = 33,
	["GLOWGORP"] = 20,
	["CABLEBOT"] = 33,
	["MINERBOT"] = 33,
}

local OverworldSpawnChances = {
	["HAYBOT"] = 5,
	["TOTEBOT_GREEN"] = 5,
}

local OverworldSeedbotBaseSpawnChance = 10
local OverworldSeedbotPerTierSpawnChance = 5
local OverworldSeedbotMaxSpawnChance = 30

local OverworldHaybotInFieldSpawnChance = 15

local UndergroundSpawnChances = {
	["GLOWGORP"] = 50,
}

local DefaultSpawnChances = {
	["HAYBOT"] = 10,
	["TOTEBOT_GREEN"] = 20,
	["TOTEBOT_LEAF"] = 10,
	["TOTEBOT_BLUE"] = 10,
	["TOTEBOT_RED"] = 10,
	["TOTEBOT_YELLOW"] = 10,
	["SEEDBOT"] = 10,
	["TAPEBOT"] = 10,
	["TAPEBOT_GREEN"] = 10,
	["TAPEBOT_YELLOW"] = 10,
	["TAPEBOT_RED"] = 10,
	["FARMBOT"] = 4,
	["WOC"] = 10,
	["GLOWGORP"] = 2,
	["CABLEBOT"] = 10,
	["MINERBOT"] = 10,
}

-- Random spawn % chance for non poi cells
local function UnitRandomSpawnChance( tag, cell )
	local pubData = sm.world.getCurrentWorld().publicData
	local worldType = pubData and pubData.type
	if worldType == "Overworld" then
		if cell.isPoi and OverworldPoiSpawnChances[tag] then
			return OverworldPoiSpawnChances[tag]
		end
		if tag == "SEEDBOT" then
			local seedMaxLootTier = ProgressionManager.Sv_GetSeedTier()
			return math.min( OverworldSeedbotBaseSpawnChance + seedMaxLootTier * OverworldSeedbotPerTierSpawnChance, OverworldSeedbotMaxSpawnChance )
		elseif tag == "HAYBOT" and cell.tags and valueExists( cell.tags, "FIELD" ) then
			return OverworldHaybotInFieldSpawnChance
		else
			return OverworldSpawnChances[tag] or DefaultSpawnChances[tag] or 100
		end
	elseif worldType == "UndergroundWorld" then
		return UndergroundSpawnChances[tag] or DefaultSpawnChances[tag] or 100
	end
	return DefaultSpawnChances[tag] or 100
end

local RuinchestRefillTime = DaysInTicks( 5 )
local FarmerballRespawnTime = DaysInTicks( 30 )

-- true = on poi, false = not on poi
local LootcrateStartareaSpawnChance = { [true] = 40, [false] = 2 }
local LootcrateCommonSpawnChance = { [true] = 30, [false] = 1 }
local LootcrateEpicChance = { [true] = 15, [false] = 5 }
local LootcrateLegendaryChance = { [true] = 10, [false] = 5 }
local LootcrateCommonToEpicUpgradeChance = { [true] = 0, [false] = 0 }


-- Units

function CreateUnit( tag, node, customParams )
	local toYaw = function( rotation )
		local spawnDirection = rotation * sm.vec3.new( 0, 0, 1 )
		return math.atan2( spawnDirection.y, spawnDirection.x ) - math.pi / 2
	end
	local params = customParams or {}
	append( params, node.params )
	if not params.tetherPoint then
		params.tetherPoint = node.position
	end
	if not params.lootType then
		local pubData = sm.world.getCurrentWorld().publicData
		local worldType = pubData and pubData.type
		if params.raider then
			params.lootType = "farmraid"
		elseif worldType == "DungeonWorld" then
			params.lootType = "growlab"
		elseif worldType == "WarehouseWorld" then
			params.lootType = "warehouse"
		elseif worldType == "UndergroundWorld" then
			params.lootType = "underground"
		end
	end
	-- Units with spawn animation are created invisible and then need to be made visible from the character
	local visible = params.spawnAnimation == nil or not params.spawnAnimation

	local unit
	if tag == "HAYBOT" then
		unit = sm.unit.createUnit( unit_haybot, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "TOTEBOT_GREEN" then
		unit = sm.unit.createUnit( unit_totebot_green, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "TOTEBOT_LEAF" then
		unit = sm.unit.createUnit( unit_totebot_leaf, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "TOTEBOT_BLUE" then
		unit = sm.unit.createUnit( unit_totebot_blue, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "TOTEBOT_RED" then
		unit = sm.unit.createUnit( unit_totebot_red, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "TOTEBOT_YELLOW" then
		unit = sm.unit.createUnit( unit_totebot_yellow, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "SEEDBOT" then
		if not params.seedType then
			-- Not on seedbots: "cotton", "pigmentflower"
			local seedMaxLootTier = ProgressionManager.Sv_GetSeedTier()
			local seedType
			if seedMaxLootTier >= 8 and math.random() < 1 / 8.5 then
				seedType = "pineapple"
			elseif seedMaxLootTier >= 7 and math.random() < 1 / 7.5 then
				seedType = "broccoli"
			elseif seedMaxLootTier >= 6 and math.random() < 1 / 6.5 then
				seedType = "orange"
			elseif seedMaxLootTier >= 5 and math.random() < 1 / 5.5 then
				seedType = "blueberry"
			elseif seedMaxLootTier >= 4 and math.random() < 1 / 4.5 then
				seedType = "banana"
			elseif seedMaxLootTier >= 3 and math.random() < 1 / 3.5 then
				seedType = "redbeet"
			elseif seedMaxLootTier >= 2 and math.random() < 1 / 2.5 then
				seedType = "carrot"
			elseif math.random() < 1 / 1.5 then
				seedType = "tomato"
			else
				seedType = "potato"
			end
			params.seedType = seedType
		end
		unit = sm.unit.createUnit( unit_seedbot, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "FARMBOT" then
		unit = sm.unit.createUnit( unit_farmbot, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "TAPEBOT" then
		unit = sm.unit.createUnit( g_tapebots[math.random( 1, #g_tapebots )], node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "TAPEBOT_GREEN" then
		unit = sm.unit.createUnit( g_greenTapebots[math.random( 1, #g_greenTapebots )], node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "TAPEBOT_YELLOW" then
		unit = sm.unit.createUnit( unit_tapebot_yellow, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "TAPEBOT_RED" then
		unit = sm.unit.createUnit( unit_tapebot_red, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "WOC" then
		unit = sm.unit.createUnit( unit_woc, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "GLOWGORP" then
		unit = sm.unit.createUnit( unit_worm, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "CABLEBOT" then
		unit = sm.unit.createUnit( unit_cablebot, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "MINERBOT" then
		unit = sm.unit.createUnit( unit_minerbot, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "TRASHBOT" then
		unit = sm.unit.createUnit( unit_trashbot, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "TRASHBOT2" then
		unit = sm.unit.createUnit( unit_trashbot2, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "SHIPMECHANIC" then
		unit = sm.unit.createUnit( unit_shipMechanic, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "QUESTNPC_HUBERT" then
		unit = sm.unit.createUnit( unit_questnpchubert, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "BABYWOC" then
		unit = sm.unit.createUnit( unit_baby_woc, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "QUESTNPC_LORENZO" then
		unit = sm.unit.createUnit( unit_questnpclorenzo, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "QUESTNPC_PHE" then
		unit = sm.unit.createUnit( unit_questnpcphe, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "BUILDERGUIDE_SCRAPPER" then
		unit = sm.unit.createUnit( unit_builderguideScrapper, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "BUILDERGUIDE_FARMER" then
		unit = sm.unit.createUnit( unit_builderguideFarmer, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "BUILDERGUIDE_TOTEBOT" then
		unit = sm.unit.createUnit( unit_builerguideTotebot, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "BUILDERGUIDE_GLOWGORP" then
		unit = sm.unit.createUnit( unit_builerguideWorm, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "BUILDERGUIDE_WOC" then
		unit = sm.unit.createUnit( unit_builderguideWoc, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "BUILDERGUIDE_FARMERSLEEPY" then
	 	unit = sm.unit.createUnit( unit_builderguideFarmersleepy, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "BUILDERGUIDE_FARMERKO" then
	 	unit = sm.unit.createUnit( unit_builderguideFarmerko, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "CINEMATIC_DRILLBOT" then
	 	unit = sm.unit.createUnit( unit_cinematicdrillbot, node.position, toYaw( node.rotation ), params, nil, visible )
	elseif tag == "GENERAL_QUESTACTOR" then
		if SafeNestedAccess( node, "params", "questActorParams" ).characterUuid then
			unit = sm.unit.createUnit( node.params.questActorParams.characterUuid, node.position, toYaw( node.rotation ), params, nil, visible )
		else
			sm.log.warning( "Trying to spawn GENERAL_QUESTACTOR without characterUuid in node params." )
		end
	else
		assert( false, "No unit constructor for tag "..tag )
	end
	assert( unit )
	return unit
end

local function SpawnUnits( cell, nodes, storage, tag, respawn )
	local pubData = sm.world.getCurrentWorld().publicData
	local worldType = pubData and pubData.type
	shuffle( nodes )
	local spawnCount = 0
	for _, node in ipairs( nodes ) do
		local noSpawn = false

		local skipRespawn = node.params and node.params.respawn ~= nil and node.params.respawn ~= true
		if ( node.params and ( ( node.params.eventName and node.params.eventName ~= "" ) or ( node.params.spawnRange and node.params.spawnRange > 0.0 ) or node.params.manualSpawn or node.params.singleSpawn ) ) or respawn ~= nil and respawn and skipRespawn then
			-- Skip spawing units that are spawned by events or manual spawns or are not respawnable
			noSpawn = true
		end

		if worldType == "Overworld" then
			if tag == "SEEDBOT" and node.params and node.params.seedType then
				local seedTier = SEED_TYPE_TO_SEED_TIER[node.params.seedType] or 1
				if seedTier > ProgressionManager.Sv_GetSeedTier() then
					noSpawn = true
				end
			end
		end

		if noSpawn then
		elseif node.params and node.params.guaranteed then
			table.insert( storage.units, CreateUnit( tag, node, {} ) )
			spawnCount = spawnCount + 1
		elseif math.random( 100 ) <= UnitRandomSpawnChance( tag, cell ) then
			table.insert( storage.units, CreateUnit( tag, node, {} ) )
			spawnCount = spawnCount + 1
		end
	end
	return spawnCount
end

local function UnitSpawnFunction( cell, nodes, tag )
	local storage = { units = {} }
	-- Spawn units on nodes
	local spawnCount = SpawnUnits( cell, nodes, storage, tag )
	assert( #storage.units == spawnCount )
	return storage
end

-- This respawn function remove all old units from this cell and creates new ones
local function UnitRespawnAllFunction( cell, nodes, storage, tag )
	-- Remove old units
	for _, unit in ipairs( storage.units ) do
		sm.unit.forceDestroy( unit )
	end
	storage.units = {}
	-- Respawn units on nodes
	SpawnUnits( cell, nodes, storage, tag, true )
end

-- Lootcrate

local function SpawnCommonLootcrate( cell, nodes, storage )
	shuffle( nodes )
	for _, node in ipairs( nodes ) do
		local guaranteed = node.params and node.params.guaranteed
		if cell.isStartArea then
			if guaranteed or math.random( 100 ) <= LootcrateStartareaSpawnChance[cell.isPoi] then
				local harvestable = sm.harvestable.createHarvestable( hvs_lootcrate, node.position, node.rotation )
				harvestable:setParams( { lootTable = "lootsource_lootcrate_common_startarea" } )
				table.insert( storage.harvestables, harvestable )
			end
		else
			if guaranteed or math.random( 100 ) <= LootcrateCommonSpawnChance[cell.isPoi] then
				if math.random( 100 ) <= LootcrateCommonToEpicUpgradeChance[cell.isPoi] then
					local harvestable = sm.harvestable.createHarvestable( hvs_lootcrateepic, node.position, node.rotation )
					harvestable:setParams( { lootTable = "lootsource_lootcrate_epic" } )
					table.insert( storage.harvestables, harvestable )
				else
					local harvestable = sm.harvestable.createHarvestable( hvs_lootcrate, node.position, node.rotation )
					harvestable:setParams( { lootTable = "lootsource_lootcrate_common" } )
					table.insert( storage.harvestables, harvestable )
				end
			end
		end
	end
end

local function CommonLootcrateSpawnFunction( cell, nodes, tag )
	local storage = { harvestables = {} }
	SpawnCommonLootcrate( cell, nodes, storage )
	return storage
end

local function CommonLootcrateRespawnFunction( cell, nodes, storage, tag  )
	for idxHvs, hvs in reverse_ipairs( storage.harvestables ) do
		if sm.exists( hvs ) then
			for idxNode, node in ipairs( nodes ) do
				if sm.vec3.getDistanceSquared( hvs:getPosition(), node.position ) < 0.01 then
					table.remove( nodes, idxNode )
					break
				end
			end
		else
			table.remove( storage.harvestables, idxHvs )
		end
	end
	SpawnCommonLootcrate( cell, nodes, storage )
end


-- Epic lootcrate

local function SpawnEpicLootcrate( cell, nodes, storage )
	shuffle( nodes )
	for _, node in ipairs( nodes ) do
		local guaranteed = node.params and node.params.guaranteed
		if guaranteed or math.random( 100 ) <= LootcrateEpicChance[cell.isPoi] then
			local harvestable = sm.harvestable.createHarvestable( hvs_lootcrateepic, node.position, node.rotation )
			harvestable:setParams( { lootTable = "lootsource_lootcrate_epic" } )
			table.insert( storage.harvestables, harvestable )
		end
	end
end

local function EpicLootcrateSpawnFunction( cell, nodes, tag )
	local storage = { harvestables = {} }
	SpawnEpicLootcrate( cell, nodes, storage )
	return storage
end

local function EpicLootcrateRespawnFunction( cell, nodes, storage, tag )
	for idxHvs, hvs in reverse_ipairs( storage.harvestables ) do
		if sm.exists( hvs ) then
			for idxNode, node in ipairs( nodes ) do
				if sm.vec3.getDistanceSquared( hvs:getPosition(), node.position ) < 0.01 then
					table.remove( nodes, idxNode )
					break
				end
			end
		else
			table.remove( storage.harvestables, idxHvs )
		end
	end
	SpawnEpicLootcrate( cell, nodes, storage )
end


-- Legendary lootcrate

local function SpawnLegendaryLootcrate( cell, nodes, storage )
	local spawnCount = 0
	shuffle( nodes )
	for _, node in ipairs( nodes ) do
		local guaranteed = node.params and node.params.guaranteed
		if guaranteed or math.random( 100 ) <= LootcrateLegendaryChance[cell.isPoi] then
			local harvestable = sm.harvestable.createHarvestable( hvs_lootcratelegendary, node.position, node.rotation )
			harvestable:setParams( { lootTable = "lootsource_lootcrate_legendary" } )
			table.insert( storage.harvestables, harvestable )
			spawnCount = spawnCount + 1
		end
	end
	return spawnCount
end

local function LegendaryLootcrateSpawnFunction( cell, nodes, tag )
	local storage = { harvestables = {} }
	SpawnLegendaryLootcrate( cell, nodes, storage )
	return storage
end

local function LegendaryLootcrateRespawnFunction( cell, nodes, storage, tag )
	for idx, hvs in reverse_ipairs( storage.harvestables ) do
		if sm.exists( hvs ) then
			for idxNode, node in ipairs( nodes ) do
				if sm.vec3.getDistanceSquared( hvs:getPosition(), node.position ) < 0.01 then
					table.remove( nodes, idxNode )
					break
				end
			end
		else
			table.remove( storage.harvestables, idx )
		end
	end
	SpawnLegendaryLootcrate( cell, nodes, storage )
end


-- Ruin chests

local function RuinchestSpawnFunction( cell, interactables, uuid )
	local storage = { containers = {}, ticks = math.floor( sm.game.getCurrentTick() / RuinchestRefillTime ) * RuinchestRefillTime }
	assert( interactables )
	for _, interactable in ipairs( interactables ) do
		local container = interactable:getContainer()
		local loot
		if cell.isStartArea then
			loot = SelectLoot( "lootsource_ruinchest_startarea", container:getSize() )
		else
			loot = SelectLoot( "lootsource_ruinchest", container:getSize() )
		end

		sm.container.beginTransaction()
		for i,loot in ipairs( loot ) do
			sm.container.setItem( container, i - 1, loot.uuid, loot.quantity )
		end
		sm.container.endTransaction()
		table.insert( storage.containers, container )
	end
	return storage
end

local function RuinchestRespawnFunction( cell, storage, uuid )
	-- Refill containers
	assert( storage.containers )
	local currentTick = sm.game.getCurrentTick()
	if storage.ticks == nil then -- Patching old saves
		storage.ticks = math.max( math.floor( currentTick / RuinchestRefillTime ) * RuinchestRefillTime - RuinchestRefillTime * 3, 0 )
	end
	local iterations = math.floor( ( currentTick - storage.ticks ) / RuinchestRefillTime )
	storage.ticks = storage.ticks + iterations * RuinchestRefillTime

	for idx, container in reverse_ipairs( storage.containers ) do
		if sm.exists( container ) then
			local size = container:getSize()
			local items = {}
			for i = 1, size do
				local item = sm.container.getItem( container, i - 1 )
				if not item.uuid:isNil() then
					items[#items + 1] = item
				end
			end

			local startSize = #items
			for i = 1, iterations do
				local loot = SelectLoot( "lootsource_ruinchest_cycling", size )
				for _, item in ipairs( loot ) do
					items[#items + 1] = item
				end
				if #items >= startSize + size then
					break
				end
			end
			sm.container.beginTransaction()
			for i = 1, size do
				local index = math.max( #items - size, 0 ) + i
				if index <= #items then
					local item = items[index]
					sm.container.setItem( container, i - 1, item.uuid, item.quantity, item.instance )
				else
					sm.container.setItem( container, i - 1, sm.uuid.getNil(), 0 )
				end
			end
			sm.container.endTransaction()
		else
			table.remove( storage.containers, idx )
		end
	end
end

-- Farmer balls

local function FarmerBallSpawnFunction( cell, interactables, uuid )
	local storage = { farmers = {} }
	assert( interactables )
	for _, interactable in ipairs( interactables ) do
		local farmer = { part = interactable:getShape(), pos = interactable:getShape():getWorldPosition(), rot = interactable:getShape():getWorldRotation() }
		table.insert( storage.farmers, farmer )
	end
	return storage
end

local function FarmerBallRespawnFunction( cell, storage, uuid )
	assert( storage.farmers )
	for _, farmer in ipairs( storage.farmers ) do
		if not sm.exists( farmer.part ) then
			farmer.part = sm.shape.createPart( ITEMS.obj_survivalobject_farmerball, farmer.pos, farmer.rot, true, false )
		end
	end
end

local NodeSpawnDescriptions = {
	-- Units
	["HAYBOT"] = {
		channel = STORAGE_CHANNEL_HAYBOT_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["TOTEBOT_GREEN"] = {
		channel = STORAGE_CHANNEL_TOTEBOT_GREEN_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["TOTEBOT_LEAF"] = {
		channel = STORAGE_CHANNEL_TOTEBOT_LEAF_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["TOTEBOT_BLUE"] = {
		channel = STORAGE_CHANNEL_TOTEBOT_BLUE_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["TOTEBOT_RED"] = {
		channel = STORAGE_CHANNEL_TOTEBOT_RED_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["TOTEBOT_YELLOW"] = {
		channel = STORAGE_CHANNEL_TOTEBOT_YELLOW_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["SEEDBOT"] = {
		channel = STORAGE_CHANNEL_SEEDBOT_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["TAPEBOT"] = {
		channel = STORAGE_CHANNEL_TAPEBOT_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["TAPEBOT_GREEN"] = {
		channel = STORAGE_CHANNEL_TAPEBOT_GREEN_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	}, 
	["TAPEBOT_YELLOW"] = {
		channel = STORAGE_CHANNEL_TAPEBOT_YELLOW_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["TAPEBOT_RED"] = {
		channel = STORAGE_CHANNEL_TAPEBOT_RED_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["FARMBOT"] = {
		channel = STORAGE_CHANNEL_FARMBOT_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["WOC"] = {
		channel = STORAGE_CHANNEL_WOC_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["GLOWGORP"] = {
		channel = STORAGE_CHANNEL_GLOWGORP_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["CABLEBOT"] = {
		channel = STORAGE_CHANNEL_CABLEBOT_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["MINERBOT"] = {
		channel = STORAGE_CHANNEL_MINERBOT_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	-- Loot
	["LOOTCRATE"] = {
		channel = STORAGE_CHANNEL_LOOTCRATE_SPAWNS,
		spawn = CommonLootcrateSpawnFunction,
		respawn = CommonLootcrateRespawnFunction,
	},
	["EPICLOOTCRATE"] = {
		channel = STORAGE_CHANNEL_EPICLOOTCRATE_SPAWNS,
		spawn = EpicLootcrateSpawnFunction,
		respawn = EpicLootcrateRespawnFunction,
	},
	["LEGENDARYLOOTCRATE"] = {
		channel = STORAGE_CHANNEL_LEGENDARYLOOTCRATE_SPAWNS,
		spawn = LegendaryLootcrateSpawnFunction,
		respawn = LegendaryLootcrateRespawnFunction,
	},
	["BUILDERGUIDE_SCRAPPER"] = {
		channel = STORAGE_CHANNEL_BUILDERGUIDE_SCRAPPER_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["BUILDERGUIDE_FARMER"] = {
		channel = STORAGE_CHANNEL_BUILDERGUIDE_FARMER_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["BUILDERGUIDE_TOTEBOT"] = {
		channel = STORAGE_CHANNEL_BUILDERGUIDE_TOTEBOT_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["BUILDERGUIDE_GLOWGORP"] = {
		channel = STORAGE_CHANNEL_BUILDERGUIDE_GLOWGORP_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["BUILDERGUIDE_WOC"] = {
		channel = STORAGE_CHANNEL_BUILDERGUIDE_WOC_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["BUILDERGUIDE_FARMERSLEEPY"] = {
		channel = STORAGE_CHANNEL_BUILDERGUIDE_FARMERSLEEPY_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["BUILDERGUIDE_FARMERKO"] = {
		channel = STORAGE_CHANNEL_BUILDERGUIDE_FARMERKO_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	},
	["CINEMATIC_DRILLBOT"] = {
		channel = STORAGE_CHANNEL_CINEMATIC_DRILLBOT_SPAWNS,
		spawn = UnitSpawnFunction,
		respawn = UnitRespawnAllFunction,
	}
}

function SpawnFromNodeOnCellCreated( cell, tag )
	local cellKey = CellKey( cell.x, cell.y )
	local spawnDesc = NodeSpawnDescriptions[tag]
	assert( spawnDesc, "No spawn description attached to this tag! "..tag )
	local nodes = sm.cell.getNodesByTag( cell.x, cell.y, tag )
	if #nodes == 0 then
		return
	end
	--if tag == "LOOTCRATE" then
	--	for _, node in ipairs( nodes ) do
	--		sm.debugDraw.addAabb( tag..tostring( node.position ), node.position - 0.25, node.position + 0.25, sm.color.new( 1, 1, 0, 0.5 ) )
	--	end
	--end
	local storage = spawnDesc.spawn( cell, nodes, tag )
	if GetTicksBetweenRespawns( tag ) then -- No need to save if it never respawns
		sm.storage.save( { spawnDesc.channel, cell.worldId, cellKey }, { tick = sm.game.getCurrentTick(), storage = storage } )
	end
end

function RespawnFromNodeOnCellLoaded( cell, tag )
	local cellKey = CellKey( cell.x, cell.y )
	local spawnDesc = NodeSpawnDescriptions[tag]
	assert( spawnDesc, "No spawn description attached to this tag!" )
	if spawnDesc.respawn == nil then
		return
	end
	local ticksBetweenRespawns = GetTicksBetweenRespawns( tag )
	if ticksBetweenRespawns then
		local currentTick = sm.game.getCurrentTick()
		local loaded = sm.storage.load( { spawnDesc.channel, cell.worldId, cellKey } ) or { tick = math.floor( sm.game.getCurrentTick() / ticksBetweenRespawns ) * ticksBetweenRespawns }
		if currentTick < loaded.tick then
			sm.log.warning( "Something tried to spawn using a stored tick higher than current tick. Likely a save file that has been moved: currentTick: "..currentTick..", loaded.tick: "..loaded.tick )
			loaded.tick = currentTick
		end
		if currentTick - loaded.tick > ticksBetweenRespawns then
			local nodes = sm.cell.getNodesByTag( cell.x, cell.y, tag )
			if #nodes == 0 then
				return
			end
			spawnDesc.respawn( cell, nodes, loaded.storage, tag )
			sm.storage.save( { spawnDesc.channel, cell.worldId, cellKey }, { tick = math.floor( sm.game.getCurrentTick() / ticksBetweenRespawns ) * ticksBetweenRespawns, storage = loaded.storage } )
		end
	end
end

function ResetNodeSpawnTimer( cell, tag )
	local cellKey = CellKey( cell.x, cell.y )
	local spawnDesc = NodeSpawnDescriptions[tag]
	assert( spawnDesc, "No spawn description attached to this tag!" )
	if spawnDesc.respawn == nil then
		return
	end
	local ticksBetweenRespawns = GetTicksBetweenRespawns( tag )
	if ticksBetweenRespawns then
		local loaded = sm.storage.load( { spawnDesc.channel, cell.worldId, cellKey } )
		if loaded then
			sm.storage.save( { spawnDesc.channel, cell.worldId, cellKey }, { tick = -ticksBetweenRespawns, storage = loaded.storage } )
		end
	end
end

function UntrackUnitRespawn( cell, tag, unit )
	local cellKey = CellKey( cell.x, cell.y )
	local spawnDesc = NodeSpawnDescriptions[tag]
	assert( spawnDesc, "No spawn description attached to this tag!" )
	if spawnDesc.respawn == nil then
		return
	end
	local loaded = sm.storage.load( { spawnDesc.channel, cell.worldId, cellKey } )
	if loaded then
		removeFromArray( loaded.storage.units, function( u ) return u == unit end )
		sm.storage.save( { spawnDesc.channel, cell.worldId, cellKey }, { tick = loaded.tick, storage = loaded.storage } )
	end
end


local InteractableOnSpawnDescriptions = {
	-- Ruin chests
	[tostring( ITEMS.obj_survivalobject_ruinchest )] = {
		channel = STORAGE_CHANNEL_RUINCHEST_SPAWNS,
		ticksBetweenRespawns = RuinchestRefillTime,
		spawn = RuinchestSpawnFunction,
		respawn = RuinchestRespawnFunction,
	},
	-- Farmer balls
	[tostring( ITEMS.obj_survivalobject_farmerball )] = {
		channel = STORAGE_CHANNEL_FARMERBALL_SPAWNS,
		ticksBetweenRespawns = FarmerballRespawnTime,
		spawn = FarmerBallSpawnFunction,
		respawn = FarmerBallRespawnFunction,
	}
}

function RespawningInteractableOnCellCreated( cell, uuid )
	local cellKey = CellKey( cell.x, cell.y )
	local spawnDesc = InteractableOnSpawnDescriptions[tostring( uuid )]
	assert( spawnDesc, "No spawn description attached to this tag!" )
	local interactables = sm.cell.getInteractablesByUuid( cell.x, cell.y, uuid )
	if #interactables == 0 then
		return
	end
	local storage = spawnDesc.spawn( cell, interactables, uuid )
	sm.storage.save( { spawnDesc.channel, cell.worldId, cellKey }, { tick = math.floor( sm.game.getCurrentTick() / spawnDesc.ticksBetweenRespawns ) * spawnDesc.ticksBetweenRespawns, storage = storage } )
end

function TryRespawnInteractableOnCellLoaded( cell, uuid )
	local cellKey = CellKey( cell.x, cell.y )
	local spawnDesc = InteractableOnSpawnDescriptions[tostring( uuid )]
	assert( spawnDesc, "No spawn description attached to this tag!" )
	if spawnDesc.respawn == nil then
		return
	end
	local currentTick = sm.game.getCurrentTick()
	local loaded = sm.storage.load( { spawnDesc.channel, cell.worldId, cellKey } ) or { tick = math.floor( sm.game.getCurrentTick() / spawnDesc.ticksBetweenRespawns ) * spawnDesc.ticksBetweenRespawns }
	assert( currentTick >= loaded.tick, "currentTick: "..currentTick..", loaded.tick: "..loaded.tick )
	assert( spawnDesc.ticksBetweenRespawns )
	if ( currentTick - loaded.tick ) > spawnDesc.ticksBetweenRespawns then
		spawnDesc.respawn( cell, loaded.storage, uuid )
		sm.storage.save( { spawnDesc.channel, cell.worldId, cellKey }, { tick = math.floor( sm.game.getCurrentTick() / spawnDesc.ticksBetweenRespawns ) * spawnDesc.ticksBetweenRespawns, storage = loaded.storage } )
	end
end
