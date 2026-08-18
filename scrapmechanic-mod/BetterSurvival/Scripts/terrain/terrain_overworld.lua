dofile "$SURVIVAL_DATA/Scripts/game/survival_constants.lua"
dofile "$SURVIVAL_DATA/Scripts/terrain/random_generation.lua"
dofile "$CONTENT_DATA/Scripts/terrain/overworld/generate_cells.lua"
dofile "$SURVIVAL_DATA/Scripts/util.lua"
dofile "$SURVIVAL_DATA/Scripts/kinematic_util.lua"
dofile "$SURVIVAL_DATA/Scripts/terrain/terrain_util2.lua"

dofile "$GAME_DATA/Scripts/terrain/terrain_area_util.lua"
dofile "$GAME_DATA/Scripts/terrain/terrain_prepare_cell_util.lua"

-- Versions
-- 1: Adds 'mechanicStation'
-- 2: Adds 'crashedShip' and changed the table format to { vec3, world }
local LOCATION_STORAGE_VERSION = 2
local QUESTMARKER_STORAGE_VERSION = 1




-- POI tiles whose tile storage keys suppress bond builder (flavor) dialogs.
-- Collected into STORAGE_CHANNEL_BONDBUILDER_EXCLUSION_TILES during world generation.
local BondBuilderExclusionPoiTypes = {
	[POI_HIDEOUT_XL] = true,
	[POI_BUILDERQUEST_RESOURCECAR] = true,
	[POI_BUILDERQUEST_WOCHOUSE] = true,
	[POI_BUILDERQUEST_CARDBOARDPOOP] = true,
	[POI_BUILDERQUEST_XYLOPHONE] = true,
	[POI_BUILDERQUEST_BEESUIT] = true,
	[POI_BUILDERQUEST_CAROUSEL] = true,
	[POI_BUILDERQUEST_CROWBAR] = true,
	[POI_BUILDERQUEST_COMPASS] = true,
	[POI_BUILDERQUEST_NICEHOUSE_MEDIUM] = true,
	[POI_BUILDERQUEST_STEELBRIDGE_MEDIUM] = true,
	[POI_BUILDERQUEST_SLEDGEHAMMER_MEDIUM] = true,
	[POI_BUILDERQUEST_BAGUETTE_MEDIUM] = true,
	[POI_FOREST_BUILDERQUEST_SAWBLADEARM] = true,
	[POI_DESERT_BUILDERQUEST_BIGFAN] = true,
	[POI_DESERT_BUILDERQUEST_GARDEN] = true,
	[POI_FIELD_BUILDERQUEST_CORNHEART] = true,
	[POI_FIELD_BUILDERQUEST_COZYBED] = true,
	[POI_BURNTFOREST_BUILDERQUEST_TOTEBOTKEY] = true,
	[POI_BURNTFOREST_BUILDERQUEST_CATAPULT_MEDIUM] = true,
	[POI_AUTUMNFOREST_BUILDERQUEST_POPCORN] = true,
	[POI_AUTUMNFOREST_BUILDERQUEST_MUSICBOX_MEDIUM] = true,
}

local GRAPHICS_CELL_PADDING = 8

----------------------------------------------------------------------------------------------------
-- Initialization
----------------------------------------------------------------------------------------------------

function Init( world, generatorIndex )
	g_world = world
	g_generatorIndex = generatorIndex

	initRoadAndCliffTiles()
	initMeadowTiles()
	initForestTiles()
	initFieldTiles()
	initBurntForestTiles()
	initAutumnForestTiles()
	initLakeTiles()
	initDesertTiles()
	initPoiTiles()
	initBiomeRoadTiles()

	local worldFile = sm.json.open( ExcavationIsland.worldFile )
	for _, cell in pairs( worldFile.cellData ) do
		if cell.path and cell.path ~= "" then
			AddTile( nil, cell.path, nil, nil )
		end
	end
end


function Create( xMin, xMax, yMin, yMax, seed, data )
	print( "Create overworld terrain", xMin, xMax, yMin, yMax )

	-- v0.5.0: GRAPHICS_CELL_PADDING is no longer included in min/max
	xMin = xMin - GRAPHICS_CELL_PADDING
	xMax = xMax + GRAPHICS_CELL_PADDING
	yMin = yMin - GRAPHICS_CELL_PADDING
	yMax = yMax + GRAPHICS_CELL_PADDING












	--seed = 631994210
	print( string.format( "Seed: %.0f", seed ) )
	generateOverworldCelldata( xMin, xMax, yMin, yMax, seed, data, GRAPHICS_CELL_PADDING )


	sm.terrainGeneration.setGameStorageNoSave( "tileData"..g_world.id, GetTileDatabase() )

	g_cellData.version = CURRENT_VERSION
	sm.terrainData.save( g_cellData )

	GenerateOceanCells()
	GenerateHLodCells()
	CreateControlPoints()
	UpdateLocationStorage()
	PrecacheLocationStorage()
	CreateCellDataStorage()
	
	local elevatorLocationStorage = sm.terrainGeneration.loadGameStorage( STORAGE_CHANNEL_ELEVATOR_LOCATIONS ) or {}
	if not elevatorLocationStorage[g_world.id] then
		elevatorLocationStorage[g_world.id] = {}
	end

	-- Tile storage keys for tiles that suppress bond builder (flavor) dialogs. Keys already include
	-- the world id, so they are stored in a single global set covering all worlds.
	local bondBuilderExclusionStorage = sm.terrainGeneration.loadGameStorage( STORAGE_CHANNEL_BONDBUILDER_EXCLUSION_TILES ) or {}

	print( "Finding all underground elevators" )
	for y = g_cellData.bounds.yMin, g_cellData.bounds.yMax do
		for x = g_cellData.bounds.xMin, g_cellData.bounds.xMax do
			local uid = g_cellData.uid[y][x]
			local cell = { x = x, y = y, offsetX = g_cellData.xOffset[y][x], offsetY = g_cellData.yOffset[y][x], rotation = g_cellData.rotation[y][x] }
			concat( elevatorLocationStorage[g_world.id], FindConnectionNodes( uid, cell, "UNDERGROUND_ELEVATOR", UNDERGROUND_CONNECTIONS ) )

			if not uid:isNil() and BondBuilderExclusionPoiTypes[GetPoiType( uid )] then
				local key = CalculateTileStorageKey( g_world.id, x, y )
				if key then
					bondBuilderExclusionStorage[key] = true
				end
			end
		end
	end
	sm.terrainGeneration.saveGameStorage( STORAGE_CHANNEL_ELEVATOR_LOCATIONS, elevatorLocationStorage )

	sm.terrainGeneration.saveGameStorage( STORAGE_CHANNEL_BONDBUILDER_EXCLUSION_TILES, bondBuilderExclusionStorage )
end


function Load()
	if g_generatorIndex == 0 and sm.isHost then
		sm.terrainGeneration.setGameStorageNoSave( "tileData"..g_world.id, GetTileDatabase() )
	end
	
	if sm.terrainData.exists() then
		g_cellData = sm.terrainData.load()
		if g_generatorIndex == 0 and UpgradeCellData( g_cellData ) then
			sm.terrainData.save( g_cellData )
		end
		
		GenerateOceanCells()
		GenerateHLodCells()
		CreateControlPoints()
		UpdateLocationStorage()
		PrecacheLocationStorage()
		CreateCellDataStorage()

		return true
	end
	return false
end

----------------------------------------------------------------------------------------------------

function CreateControlPoints()
	g_cpWestEdge = {}
	g_cpSouthEdge = {}
	g_cpMid = {}
	for y = g_cellData.bounds.yMin, g_cellData.bounds.yMax do
		g_cpWestEdge[y] = {}
		g_cpSouthEdge[y] = {}
		g_cpMid[y] = {}
		for x = g_cellData.bounds.xMin, g_cellData.bounds.xMax do
			g_cpWestEdge[y][x] = ( getCornerElevationLevel( x, y ) + getCornerElevationLevel( x, y + 1 ) ) / 2
			g_cpSouthEdge[y][x] = ( getCornerElevationLevel( x, y ) + getCornerElevationLevel( x + 1, y ) ) / 2
			local cpEastEdge = ( getCornerElevationLevel( x + 1, y ) + getCornerElevationLevel( x + 1, y + 1 ) ) / 2
			g_cpMid[y][x] = ( g_cpWestEdge[y][x] + cpEastEdge ) / 2
		end
	end
end

function PrecacheLocationStorage()










































end

function UpdateLocationStorage()
	if g_generatorIndex == 0 and sm.isHost then

-- find location and quest marker nodes, prints lines to use in the nodeLocations and markerLocations tables below

























		-- Location storage (version-gated)
		local locationStorage = sm.terrainGeneration.loadGameStorage( STORAGE_CHANNEL_LOCATIONS ) or { [g_world.id] = { version = 0 } }
		if locationStorage[g_world.id] == nil then
			locationStorage[g_world.id] = { version = 0 }
		end
		if locationStorage[g_world.id].version ~= LOCATION_STORAGE_VERSION then
			locationStorage = { [g_world.id] = { version = LOCATION_STORAGE_VERSION } }

			-- locations generated from location nodes tiles. place generated lines here.
			local nodeLocations = {}
			nodeLocations[#nodeLocations+1] = { name = "excavationKey1", uuid = sm.uuid.new("bf0ba240-416f-4f32-b87d-3a445919e72a"), pos = sm.vec3.new( 524.11169433594, 591.79669189453, -85.533950805664 ) }
			nodeLocations[#nodeLocations+1] = { name = "garage", uuid = sm.uuid.new("c1af7c32-42e8-471f-93b6-019f2c22ed10"), pos = sm.vec3.new( 336.06533813477, 204.72267150879, 11.998781204224 ) }
			nodeLocations[#nodeLocations+1] = { name = "growlab_01", uuid = sm.uuid.new("e70e6ba1-29a3-40a4-9ec3-cc2ed60a69c9"), pos = sm.vec3.new( 135.5, 152, 60 ) }
			nodeLocations[#nodeLocations+1] = { name = "growlab_02", uuid = sm.uuid.new("d159bbf6-7b87-4073-8da7-c6cc3b85e4b5"), pos = sm.vec3.new( 118.5, 107.50001525879, 63.375 ) }
			nodeLocations[#nodeLocations+1] = { name = "growlab_02", uuid = sm.uuid.new("d159bbf6-7b87-4073-8da7-c6cc3b85e4b5"), pos = sm.vec3.new( 118.5, 114, 62.75 ) }
			nodeLocations[#nodeLocations+1] = { name = "growlab_03", uuid = sm.uuid.new("312e8d1c-de9c-479d-861a-cace1cb480f7"), pos = sm.vec3.new( 130.75, 105.25003051758, 18.375 ) }
			nodeLocations[#nodeLocations+1] = { name = "growlab_03", uuid = sm.uuid.new("312e8d1c-de9c-479d-861a-cace1cb480f7"), pos = sm.vec3.new( 131, 112, 18 ) }
			nodeLocations[#nodeLocations+1] = { name = "growlab_04", uuid = sm.uuid.new("b5b956c1-bab0-4bbe-abb0-e0ab8d3f1fab"), pos = sm.vec3.new( 102.5, 102.75, 3 ) }
			nodeLocations[#nodeLocations+1] = { name = "growlab_04", uuid = sm.uuid.new("b5b956c1-bab0-4bbe-abb0-e0ab8d3f1fab"), pos = sm.vec3.new( 102.5, 96.000022888184, 3.375 ) }
			nodeLocations[#nodeLocations+1] = { name = "growlab_05", uuid = sm.uuid.new("08f0037b-9233-4fe8-b7e1-c1a0c4a2913b"), pos = sm.vec3.new( 289, 256, 2.875 ) }
			nodeLocations[#nodeLocations+1] = { name = "growlab_05", uuid = sm.uuid.new("08f0037b-9233-4fe8-b7e1-c1a0c4a2913b"), pos = sm.vec3.new( 295.5, 256, 2 ) }
			nodeLocations[#nodeLocations+1] = { name = "growlab_06", uuid = sm.uuid.new("8e1538ae-6169-4053-b9ae-d80258c6fb3b"), pos = sm.vec3.new( 210, 327, 53.375 ) }
			nodeLocations[#nodeLocations+1] = { name = "growlab_06", uuid = sm.uuid.new("8e1538ae-6169-4053-b9ae-d80258c6fb3b"), pos = sm.vec3.new( 210, 334, 53 ) }
			nodeLocations[#nodeLocations+1] = { name = "growlab_07", uuid = sm.uuid.new("c1af7c32-42e8-471f-93b6-019f2c22ed10"), pos = sm.vec3.new( 229, 194.75, 26.75 ) }

			-- manually entered values for locations in POI tiles.
			local nodelessLocation = {}
			nodelessLocation[#nodelessLocation+1] = { name = "mechanicStation", poiType = POI_MECHANICSTATION_QUEST_MEDIUM, size = 2, pos = sm.vec3.new(43.5, 85.0, 5.0) }
			nodelessLocation[#nodelessLocation+1] = { name = "hideout", poiType = POI_HIDEOUT_XL, size = 8, pos = sm.vec3.new( 207.75, 284.25, 12.0 ) }
			nodelessLocation[#nodelessLocation+1] = { name = "vegetableStation", poiType = POI_PACKINGSTATIONVEG_MEDIUM, size = 2, pos = sm.vec3.new( 37, 102, 18 ) }
			nodelessLocation[#nodelessLocation+1] = { name = "fruitStation", poiType = POI_PACKINGSTATIONFRUIT_MEDIUM, size = 2, pos = sm.vec3.new( 37, 102, 18 ) }
			nodelessLocation[#nodelessLocation+1] = { name = "labyrinth", poiType = POI_LABYRINTH_MEDIUM, size = 2, pos = sm.vec3.new( 37, 102, 18 ) }

			-- static locations entered directly
			locationStorage["crashedShip"] = { pos = sm.vec3.new( -2372.0, -2623.0, 8.0 ), world = g_world }
			locationStorage["drillbotMountain"] = { pos = sm.vec3.new( -2510, -2640, 30 ), world = g_world }

			function AddLocation( name, tileSize, pos, cellX, cellY )
				assert(cellX)
				assert(cellY)
				local minCellX, minCellY = GetTileMinCell( cellX, cellY, tileSize )
				local rx, ry = RotateLocal( cellX, cellY, pos.x, pos.y, tileSize * CELL_SIZE )
				local x = (cellX - (cellX - minCellX)) * CELL_SIZE + rx
				local y = (cellY - (cellY - minCellY)) * CELL_SIZE + ry
				local z = pos.z + getElevationHeightAt( x, y ) + getCliffHeightAt( x, y )
				locationStorage[name] = { pos = sm.vec3.new( x, y, z ), world = g_world }
			end

			for cellY = g_cellData.bounds.yMin, g_cellData.bounds.yMax do
				for cellX = g_cellData.bounds.xMin, g_cellData.bounds.xMax do
					local uid = GetCellTileUid( cellX, cellY )

					for index, search in ipairs( nodeLocations ) do
						if search.uuid == uid then
							AddLocation( search.name, GetSize( uid ), search.pos, cellX, cellY )
							table.remove( nodeLocations, index )
							break
						end
					end
					for index, search in ipairs( nodelessLocation ) do
						if search.poiType == GetPoiType( uid ) then
							AddLocation( search.name, GetSize( uid ), search.pos, cellX, cellY )
							table.remove( nodelessLocation, index )
							break
						end
					end
				end
			end

			sm.terrainGeneration.saveGameStorage( STORAGE_CHANNEL_LOCATIONS, locationStorage )
		end

		-- Quest marker precaching
		local questMarkerStorage = sm.terrainGeneration.loadGameStorage( STORAGE_CHANNEL_PRECACHED_MARKERS ) or {}
		local questMarkerVersions = questMarkerStorage["versions"] or {}
		-- Treat existing marker data without version info as current version (preserves data from prior release)
		if questMarkerStorage[g_world.id] ~= nil and questMarkerVersions[g_world.id] == nil then
			questMarkerVersions[g_world.id] = QUESTMARKER_STORAGE_VERSION
		end
		if questMarkerVersions[g_world.id] ~= QUESTMARKER_STORAGE_VERSION then
			-- quest markers generated from quest marker nodes tiles. place generated lines here.
			local markerLocations = {}
			markerLocations[#markerLocations+1] = { name = "builder_quest_02", uuid = sm.uuid.new("bb5acabd-562f-449f-bece-5a8351c34b6e"), pos = sm.vec3.new( 23, 30.5, 5.6799998283386 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_03", uuid = sm.uuid.new("cd2e757d-249d-49af-979c-14428b41f7ad"), pos = sm.vec3.new( 24.75, 22.5, 3.1800000667572 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_04", uuid = sm.uuid.new("6c57b05d-36c7-46df-ae45-7403f756199d"), pos = sm.vec3.new( 30.25, 31.5, 3.4300000667572 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_05", uuid = sm.uuid.new("d5532a26-4450-4bc7-9db9-ff370be3dee9"), pos = sm.vec3.new( 36.75, 38.25, 5.9099998474121 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_06", uuid = sm.uuid.new("6ddf24cb-db84-4534-81a8-a9ce81e83d84"), pos = sm.vec3.new( 30, 33.25, 5.9099998474121 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_07", uuid = sm.uuid.new("c62ca228-3190-4c09-bd1a-88eb32e0695b"), pos = sm.vec3.new( 24.25, 33.5, 13.680000305176 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_08", uuid = sm.uuid.new("4bb32278-6e87-482d-a9e3-3476a45a194b"), pos = sm.vec3.new( 29, 23, 3.1800000667572 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_09", uuid = sm.uuid.new("84cce463-a967-443f-95ff-9fcdb73262b0"), pos = sm.vec3.new( 35.75, 28, 12.180000305176 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_10", uuid = sm.uuid.new("86854faa-1d4f-4f2a-bb57-877abb8dfbc3"), pos = sm.vec3.new( 39.5, 39.25, 6.9099998474121 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_11", uuid = sm.uuid.new("e010fb27-2d28-44fb-a8e6-e9f9664850a7"), pos = sm.vec3.new( 61.75, 58.75, 9.6800003051758 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_12", uuid = sm.uuid.new("5c34eac0-2870-4978-8f05-7478d6e03baa"), pos = sm.vec3.new( 23.25, 29.25, 5.4299998283386 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_13", uuid = sm.uuid.new("9269286f-6bde-4d6d-ad57-3ef57c88980e"), pos = sm.vec3.new( 32.25, 42.25, 7.9099998474121 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_14", uuid = sm.uuid.new("405bb852-e090-4ee5-8a2f-24f2395d5d5f"), pos = sm.vec3.new( 29, 33.25, 32.674999237061 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_15", uuid = sm.uuid.new("49b86128-327e-457c-98f5-1949bee17c3d"), pos = sm.vec3.new( 28.75, 27, 1.6799999475479 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_16", uuid = sm.uuid.new("d149b9cf-de7e-4fcd-8e82-69c07214d3af"), pos = sm.vec3.new( 28.75, 29.75, 4.4299998283386 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_17", uuid = sm.uuid.new("eb056a47-8122-4fd5-8f45-fbd96c249997"), pos = sm.vec3.new( 70, 92.5, 2.4300000667572 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_18", uuid = sm.uuid.new("da1275c8-2133-442a-b649-6961a5ddeb7f"), pos = sm.vec3.new( 81, 52.25, 16.909999847412 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_19", uuid = sm.uuid.new("311d997e-b8b0-491d-ac9d-176335e94e97"), pos = sm.vec3.new( 37, 70.75, 7.4299998283386 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_20", uuid = sm.uuid.new("328be143-d67d-4b73-a15a-3df26c106f20"), pos = sm.vec3.new( 42.75, 66.75, 2.4300000667572 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_21", uuid = sm.uuid.new("d92fa65f-9eda-4e95-ac60-7cd35af64abf"), pos = sm.vec3.new( 59.5, 66, 11.909999847412 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_advanced_car", uuid = sm.uuid.new("f3dda4db-8450-4e9d-a501-ec6dbf14a78a"), pos = sm.vec3.new( 112.14583587646, 15.041519165039, 3.7466886043549 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_first_car", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 94.145835876465, 51.541519165039, 3.4966886043549 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "builder_quest_harvest_car", uuid = sm.uuid.new("e732c8c9-4580-4ca9-98fd-dbb79c97a623"), pos = sm.vec3.new( 26.958480834961, 52.145835876465, 3.7466886043549 ), isSideQuest = true }
			markerLocations[#markerLocations+1] = { name = "quest_bosstrain.marker_gyrocore", uuid = sm.uuid.new("5deb2830-b52e-40af-91c2-e53aee6c5165"), pos = sm.vec3.new( 45.999988555908, 40.499912261963, 25.076929092407 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_bosstrain.marker_insert", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 59.824802398682, 79.013061523438, 3.3149545192719 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_build_watchtower.marker", uuid = sm.uuid.new("7d7556b3-0dc7-4b95-9d92-731013b19fc0"), pos = sm.vec3.new( 335.61038208008, 109.64981842041, 21.904363632202 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_clear_minidungeon_keyreader.marker", uuid = sm.uuid.new("e70e6ba1-29a3-40a4-9ec3-cc2ed60a69c9"), pos = sm.vec3.new( 139.74311828613, 145.63092041016, 59.54455947876 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_clear_minidungeon_position.marker", uuid = sm.uuid.new("e70e6ba1-29a3-40a4-9ec3-cc2ed60a69c9"), pos = sm.vec3.new( 135.53179931641, 143.93862915039, 60.387001037598 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_clear_ruins.marker", uuid = sm.uuid.new("a47695ef-2028-44c7-8247-5fcad4e10bf8"), pos = sm.vec3.new( 35.333766937256, 53.866268157959, 59.121746063232 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_clear_warehouse.key_reader", uuid = sm.uuid.new("5e9fa630-76fe-4693-be63-00ecd9acf201"), pos = sm.vec3.new( 136.87411499023, 76.196235656738, 2.92138671875 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_clear_warehouse.ladder", uuid = sm.uuid.new("5e9fa630-76fe-4693-be63-00ecd9acf201"), pos = sm.vec3.new( 113, 64.75, 3 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_clear_warehouse.trashbot_position", uuid = sm.uuid.new("5e9fa630-76fe-4693-be63-00ecd9acf201"), pos = sm.vec3.new( 125.48307800293, 129.48014831543, 187.93795776367 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_connector.marker_connect", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 61.509735107422, 60.510257720947, 2.2665734291077 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_connector.marker_workbench", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 64.295341491699, 69.879249572754, 2.3357515335083 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_deliver_vegetables.dropoff_marker", uuid = sm.uuid.new("7d7556b3-0dc7-4b95-9d92-731013b19fc0"), pos = sm.vec3.new( 249.45135498047, 284.99481201172, 19.955978393555 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_deliver_vegetables.find_station_marker", uuid = sm.uuid.new("f3dda4db-8450-4e9d-a501-ec6dbf14a78a"), pos = sm.vec3.new( 37.500865936279, 93.207527160645, 6.2739868164062 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_deliver_vegetables.station_marker", uuid = sm.uuid.new("f3dda4db-8450-4e9d-a501-ec6dbf14a78a"), pos = sm.vec3.new( 37.510135650635, 95.194229125977, 8.9698867797852 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_endgame.marker_locker", uuid = sm.uuid.new("7736cd40-0adf-459b-8ffc-d0fa4caa5f59"), pos = sm.vec3.new( 187.81971740723, 171.81852722168, 30.765302658081 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_farming.marker_bucket", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 167.48799133301, 119.84285736084, 1.8902509212494 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_farming.marker_seeds", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 162.41748046875, 120.49054718018, 1.8902509212494 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_farming.marker_soil", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 164.44085693359, 113.43280792236, 1.8902509212494 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_farming.marker_water", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 162.8960723877, 100.86740112305, -0.84744334220886 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_find_recording.area_waypoint", uuid = sm.uuid.new("3ffa9284-021e-4679-8c61-6c35b04361f4"), pos = sm.vec3.new( 71, 54.25, 21.25 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_find_recording.recording_marker", uuid = sm.uuid.new("3ffa9284-021e-4679-8c61-6c35b04361f4"), pos = sm.vec3.new( 86.677154541016, 46.537548065186, -18.480836868286 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_findexcavation.button", uuid = sm.uuid.new("ba31a522-7659-4ec5-b933-8b83960c57f2"), pos = sm.vec3.new( 485.55630493164, 479.99038696289, 31.263917922974 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_findexcavation.descend", uuid = sm.uuid.new("bf0ba240-416f-4f32-b87d-3a445919e72a"), pos = sm.vec3.new( 510.00973510742, 654.81384277344, -84.472930908203 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_findexcavation.elevator", uuid = sm.uuid.new("bf0ba240-416f-4f32-b87d-3a445919e72a"), pos = sm.vec3.new( 525.96221923828, 575.54779052734, -85.51741027832 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_mechanicstation.marker_battery", uuid = sm.uuid.new("3ef31461-6f4e-4fb5-938d-875fb837d736"), pos = sm.vec3.new( 101, 80, 16 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_mechanicstation.marker_socket", uuid = sm.uuid.new("3ef31461-6f4e-4fb5-938d-875fb837d736"), pos = sm.vec3.new( 50, 85, 7 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_mystery_call.marker", uuid = sm.uuid.new("7d7556b3-0dc7-4b95-9d92-731013b19fc0"), pos = sm.vec3.new( 341.44476318359, 107.02053070068, 20.538444519043 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_phone.marker_phone", uuid = sm.uuid.new("3ef31461-6f4e-4fb5-938d-875fb837d736"), pos = sm.vec3.new( 57.350406646729, 85.020179748535, 5.1809678077698 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_shared_farmer.marker", uuid = sm.uuid.new("7d7556b3-0dc7-4b95-9d92-731013b19fc0"), pos = sm.vec3.new( 330.66616821289, 102.5, 18.25 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_trader.trader_location", uuid = sm.uuid.new("7d7556b3-0dc7-4b95-9d92-731013b19fc0"), pos = sm.vec3.new( 208.31945800781, 282.49868774414, 14.08354473114 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_tutorial.marker_battery", uuid = sm.uuid.new("943c232a-a780-4099-bffc-54ce08c184c5"), pos = sm.vec3.new( 60.253761291504, 22.253644943237, 26.787912368774 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_tutorial.marker_bucket", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 160.21343994141, 102.84412384033, 0.39653751254082 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_tutorial.marker_fire01", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 60.541923522949, 68.494812011719, 0.33374857902527 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_tutorial.marker_fire02", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 63.249713897705, 70.477691650391, 0.3807954788208 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_tutorial.marker_fire03", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 60.937088012695, 61.177017211914, 0.56004989147186 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_tutorial.marker_fire04", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 55.916625976562, 61.726871490479, 0.61515974998474 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_tutorial.marker_ruin", uuid = sm.uuid.new("943c232a-a780-4099-bffc-54ce08c184c5"), pos = sm.vec3.new( 49.395263671875, 37.038410186768, 41.092952728271 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_tutorial.marker_socket", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 61.746501922607, 73.492279052734, 1.8751908540726 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_tutorial.marker_spaceship", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 61, 64.668380737305, 1.7788771390915 ), isSideQuest = false }
			markerLocations[#markerLocations+1] = { name = "quest_tutorial.marker_water", uuid = sm.uuid.new("28c8f354-3919-46e4-a311-6c3ceee5b5d9"), pos = sm.vec3.new( 156.05027770996, 101.56636047363, -1.4474740028381 ), isSideQuest = false }

			local questMarkers = {}

			function AddQuestMarker( name, tileSize, pos, cellX, cellY, isSideQuest )
				assert(cellX)
				assert(cellY)
				local minCellX, minCellY = GetTileMinCell( cellX, cellY, tileSize )
				local rx, ry = RotateLocal( cellX, cellY, pos.x, pos.y, tileSize * CELL_SIZE )
				local x = (cellX - (cellX - minCellX)) * CELL_SIZE + rx
				local y = (cellY - (cellY - minCellY)) * CELL_SIZE + ry
				local z = pos.z + getElevationHeightAt( x, y ) + getCliffHeightAt( x, y )
				questMarkers[#questMarkers+1] = { pos = sm.vec3.new( x, y, z ), world = g_world, params = { name = name, isSideQuest = isSideQuest } }
			end

			for cellY = g_cellData.bounds.yMin, g_cellData.bounds.yMax do
				for cellX = g_cellData.bounds.xMin, g_cellData.bounds.xMax do
					local uid = GetCellTileUid( cellX, cellY )

					for index, search in reverse_ipairs( markerLocations ) do
						if search.uuid == uid then
							AddQuestMarker( search.name, GetSize( uid ), search.pos, cellX, cellY, search.isSideQuest )
							table.remove( markerLocations, index )
						end
					end
				end
			end

			questMarkerStorage["versions"] = questMarkerVersions
			questMarkerVersions[g_world.id] = QUESTMARKER_STORAGE_VERSION
			questMarkerStorage[g_world.id] = questMarkers
			sm.terrainGeneration.saveGameStorage( STORAGE_CHANNEL_PRECACHED_MARKERS, questMarkerStorage )
		end
	end
end

----------------------------------------------------------------------------------------------------
-- Utility
----------------------------------------------------------------------------------------------------

function getMidElevation( cellX, cellY )
	if insideCellBounds( cellX, cellY ) then
		return g_cpMid[cellY][cellX]
	end
	return 0
end

function getWestElevation( cellX, cellY )
	if insideCellBounds( cellX, cellY ) then
		return g_cpWestEdge[cellY][cellX]
	end
	return 0
end

function getEastElevation( cellX, cellY )
	return getWestElevation( cellX + 1, cellY )
end

function getSouthElevation( cellX, cellY )
	if insideCellBounds( cellX, cellY ) then
		return g_cpSouthEdge[cellY][cellX]
	end
	return 0
end

function getSouthWestElevation( cellX, cellY )
	if insideCellBounds( cellX, cellY ) then
		return g_cellData.elevation[cellY][cellX]
	end
	return 0
end

function getSouthEastElevation( cellX, cellY )
	return getSouthWestElevation( cellX + 1, cellY )
end


function getNorthElevation( cellX, cellY )
	return getSouthElevation( cellX, cellY + 1 )
end

----------------------------------------------------------------------------------------------------

function flatTowardsWest( cellX, cellY )
	local flags = getRoadCliffFlags( cellX, cellY )
	if bit.band( MASK_ROADS_SN, flags ) == 0 then return 0 end
	if bit.band( MASK_ROADS, flags ) ~= 0 then return 1 end
	return 0
end

function flatTowardsEast( cellX, cellY )
	local flags = getRoadCliffFlags( cellX, cellY )
	if bit.band( MASK_ROADS_SN, flags ) == 0 then return 0 end
	if bit.band( MASK_ROADS, flags ) ~= 0 then return 1 end
	return 0
end

function flatTowardsSouth( cellX, cellY )
	local flags = getRoadCliffFlags( cellX, cellY )
	if bit.band( MASK_ROADS_WE, flags ) == 0 then return 0 end
	if bit.band( MASK_ROADS, flags ) ~= 0 then return 1 end
	return 0
end

function flatTowardsNorth( cellX, cellY )
	local flags = getRoadCliffFlags( cellX, cellY )
	if bit.band( MASK_ROADS_WE, flags ) == 0 then return 0 end
	if bit.band( MASK_ROADS, flags ) ~= 0 then return 1 end
	return 0
end

----------------------------------------------------------------------------------------------------

function getMidElevationX( x0, y0, xFract )
	local t = xFract
	local x1 = x0 + 1
	local c0, c1, c2, c3

	c0 = getMidElevation( x0, y0 )

	if flatTowardsEast( x0, y0 ) == 1 then
		c1 = getMidElevation( x0, y0 )
	else
		c1 = getEastElevation( x0, y0 )
	end

	if flatTowardsWest( x1, y0 ) == 1 then
		c2 = getMidElevation( x1, y0 )
	else
		c2 = getWestElevation( x1, y0 )
	end

	c3 = getMidElevation( x1, y0 )

	return sm.util.bezier3( c0, c1, c2, c3, t )
end

function getSouthElevationX( x0, y0, xFract )
	local t = xFract
	local x1 = x0 + 1

	local flatnessEast = flatTowardsEast( x0, y0 - 1 ) * 0.5 + flatTowardsEast( x0, y0 ) * 0.5
	local flatnessWest = flatTowardsWest( x1, y0 - 1 ) * 0.5 + flatTowardsWest( x1, y0 ) * 0.5

	local c0 = getSouthElevation( x0, y0 )
	local c1 = sm.util.lerp( getSouthEastElevation( x0, y0 ), getSouthElevation( x0, y0 ), flatnessEast )
	local c2 = sm.util.lerp( getSouthWestElevation( x1, y0 ), getSouthElevation( x1, y0 ), flatnessWest )
	local c3 = getSouthElevation( x1, y0 )

	return sm.util.bezier3( c0, c1, c2, c3, t )
end

function getNorthElevationX( x0, y0, xFract )
	return getSouthElevationX( x0, y0 + 1, xFract )
end

----------------------------------------------------------------------------------------------------

local function getFraction( x, y )
	local cellX, cellY = getCell( x, y )
	return x / CELL_SIZE - cellX, y / CELL_SIZE - cellY
end

local function getElev( x, y )
	local cellX, cellY = getCell( x, y )
	local xFract, yFract = getFraction( x, y ) -- Fraction in cell [0,1)

	local x0, y0
	local xFract2, yFract2 --Mid to mid

	if xFract < 0.5 then
		x0 = cellX - 1
		xFract2 = xFract + 0.5
	else
		x0 = cellX
		xFract2 = xFract - 0.5
	end

	if yFract < 0.5 then
		y0 = cellY - 1
		yFract2 = yFract + 0.5
	else
		y0 = cellY
		yFract2 = yFract - 0.5
	end

	local flatnessNorth = sm.util.lerp( flatTowardsNorth( x0, y0 ), flatTowardsNorth( x0 + 1, y0 ), xFract2 )
	local flatnessSouth = sm.util.lerp( flatTowardsSouth( x0, y0 + 1 ), flatTowardsSouth( x0 + 1, y0 + 1 ), xFract2 )

	local c0 = getMidElevationX( x0, y0, xFract2 )
	local c1 = sm.util.lerp( getNorthElevationX( x0, y0, xFract2 ), getMidElevationX( x0, y0, xFract2 ), flatnessNorth )
	local c2 = sm.util.lerp( getSouthElevationX( x0, y0 + 1, xFract2 ), getMidElevationX( x0, y0 + 1, xFract2 ), flatnessSouth )
	local c3 = getMidElevationX( x0, y0 + 1, xFract2 )

	return sm.util.bezier3( c0, c1, c2, c3, yFract2 )
end

local cellNodeCache = {}

function GetNodesForCellWithCache( uid, tileCellOffsetX, tileCellOffsetY )
	local nodes
	local hash = sm.util.hash( uid, tileCellOffsetX, tileCellOffsetY )
	if cellNodeCache[hash] then
		nodes = cellNodeCache[hash]
	else
		nodes = sm.terrainTile.getNodesForCell( uid, tileCellOffsetX, tileCellOffsetY )
		cellNodeCache[hash] = nodes
	end
	return nodes
end

local flattenCache = {}

function getElevationHeightAt( x, y )
	local cellX, cellY = getCell( x, y )

	local blend
	local cacheKey = bit.bor( bit.lshift( cellY + 128, 8 ), cellX + 128 )
	local flattenList = flattenCache[cacheKey]

	if flattenList == nil then
		flattenList = {}

		for i = -1, 1 do
			for j = -1, 1 do
				local uid, tileCellOffsetX, tileCellOffsetY = GetCellTileUidAndOffset( cellX + j, cellY + i )
				if not uid:isNil() then
					local nodes = GetNodesForCellWithCache( uid, tileCellOffsetX, tileCellOffsetY )
					for _, node in ipairs( nodes ) do
						if ValueExists( node.tags, "FLATTEN" ) then
							local rx, ry = RotateLocal( cellX + j, cellY + i, round( node.pos.x ), round( node.pos.y ) )
							flattenList[#flattenList + 1] = {
								x = rx + ( cellX + j ) * CELL_SIZE,
								y = ry + ( cellY + i ) * CELL_SIZE,
								r = node.scale.x * 0.5
							}
						end
					end
				end
			end
		end

		-- Randomly flatten some cells for good building spot
		if insideCellBounds( cellX, cellY ) then
			if #flattenList == 0 and g_cellData.flags[cellY][cellX] == 0 and sm.noise.intNoise2d( cellX, cellY, g_cellData.seed + 358 ) % 7 == 0 then
				flattenList[#flattenList + 1] = {
					x = ( cellX + 0.5 ) * CELL_SIZE,
					y = ( cellY + 0.5 ) * CELL_SIZE,
					r = 16
				}
			end
		end

		flattenCache[cacheKey] = flattenList
	end

	for _,flat in ipairs( flattenList ) do
		local dx = x - flat.x
		local dy = y - flat.y
		local dst = math.sqrt( dx * dx + dy * dy ) / flat.r
		if dst < 2.0 then
			local p = sm.util.smoothstep( 2, 1, dst )
			if not blend or p > blend.p then
				blend = {
					x = flat.x,
					y = flat.y,
					p = p
				}
			end
		end
	end


	if blend then
		local a = getElev( x, y )
		local b = math.floor( getElev( blend.x, blend.y ) * 4 + 0.5 ) * 0.25
		return a + ( b - a ) * blend.p
	end

	return getElev( x, y )
end

----------------------------------------------------------------------------------------------------

function getCliffHeightAt( x, y )
	local cellX, cellY = getCell( x, y )

	local cliffLevelSW = getCornerCliffLevel( cellX, cellY )
	local cliffLevelSE = getCornerCliffLevel( cellX + 1, cellY )
	local cliffLevelNW = getCornerCliffLevel( cellX, cellY + 1 )
	local cliffLevelNE = getCornerCliffLevel( cellX + 1, cellY + 1 )

	return math.min( math.min( cliffLevelSW, cliffLevelSE ), math.min( cliffLevelNW, cliffLevelNE ) ) * 8
end

----------------------------------------------------------------------------------------------------

function getDetailHeightAt( x, y, lod )
	local cellX, cellY = getCell( x, y )
	local uid, tileCellOffsetX, tileCellOffsetY = GetCellTileUidAndOffset( cellX, cellY )

	local rx, ry = InverseRotateLocal( cellX, cellY, x - cellX * CELL_SIZE, y - cellY * CELL_SIZE )

	return sm.terrainTile.getHeightAt( uid, tileCellOffsetX, tileCellOffsetY, lod, rx, ry )
end

----------------------------------------------------------------------------------------------------

function getElevationNormalAt( x, y )
	local o = 2.0
	local dx = getElevationHeightAt( x + o, y ) - getElevationHeightAt( x - o, y )
	local dy = getElevationHeightAt( x, y + o ) - getElevationHeightAt( x, y - o )
	local xDir = sm.vec3.new( o * 2, 0, dx )
	local yDir = sm.vec3.new( 0, o * 2, dy )
	return sm.vec3.normalize( sm.vec3.cross( xDir, yDir ) )
end

----------------------------------------------------------------------------------------------------
-- Generator API Getters
----------------------------------------------------------------------------------------------------

function GetCellTileUidAndOffset( cellX, cellY )
	if InsideCellBounds( cellX, cellY ) then
		return 	g_cellData.uid[cellY][cellX],
				g_cellData.xOffset[cellY][cellX],
				g_cellData.yOffset[cellY][cellX]
	end
	return sm.uuid.getNil(), 0, 0
end

----------------------------------------------------------------------------------------------------

function GetHeightAt( x, y, lod )






	local height = -16
	local cellX, cellY = getCell( x, y )
	if insideCellBounds( cellX, cellY ) then
		height = getDetailHeightAt( x, y, lod )
		height = height + getElevationHeightAt( x, y )
		height = height + getCliffHeightAt( x, y )
	end





	return height
end

----------------------------------------------------------------------------------------------------

function GetColorAt( x, y, lod )






	local noise = sm.noise.octaveNoise2d( x / 8, y / 8, 5, 45 )
	local brightness = noise * 0.25 + 0.75





	local cellX, cellY = getCell( x, y )

	if insideCellBounds( cellX, cellY ) then
		local uid, tileCellOffsetX, tileCellOffsetY = GetCellTileUidAndOffset( cellX, cellY )

		local rx, ry = InverseRotateLocal( cellX, cellY, x - cellX * CELL_SIZE, y - cellY * CELL_SIZE )

		local r, g, b = sm.terrainTile.getColorAt( uid, tileCellOffsetX, tileCellOffsetY, lod, rx, ry )

		local color = { r, g, b }















		return color[1] * brightness, color[2] * brightness, color[3] * brightness
	else

		local color = { 0.0, 0.5, 1.0 }
		return color[1] * brightness, color[2] * brightness, color[3] * brightness
	end
end

----------------------------------------------------------------------------------------------------

function GetMaterialAt( x, y, lod )






	local cellX, cellY = getCell( x, y )
	if insideCellBounds( cellX, cellY ) then

--		if cellX == 0 then
--			return 1, 0, 0, 0, 0, 0, 0, 0
--		elseif cellX == 1 then
--			return 0, 1, 0, 0, 0, 0, 0, 0
--		elseif cellX == 2 then
--			return 0, 0, 1, 0, 0, 0, 0, 0
--		elseif cellX == 3 then
--			return 0, 0, 0, 1, 0, 0, 0, 0
--		elseif cellX == 4 then
--			return 0, 0, 0, 0, 1, 0, 0, 0
--		elseif cellX == 5 then
--			return 0, 0, 0, 0, 0, 1, 0, 0
--		elseif cellX == 6 then
--			return 0, 0, 0, 0, 0, 0, 1, 0
--		elseif cellX == 7 then
--			return 0, 0, 0, 0, 0, 0, 0, 1
--		end
		local uid, tileCellOffsetX, tileCellOffsetY = GetCellTileUidAndOffset( cellX, cellY )

		local rx, ry = InverseRotateLocal( cellX, cellY, x - cellX * CELL_SIZE, y - cellY * CELL_SIZE )

		return sm.terrainTile.getMaterialAt( uid, tileCellOffsetX, tileCellOffsetY, lod, rx, ry )
	end
	return 1, 0, 0, 0, 0, 0, 0, 0
end

----------------------------------------------------------------------------------------------------

function GetClutterIdxAt( x, y )






	local cellX, cellY = getCell( x * 0.5, y * 0.5 )
	if insideCellBounds( cellX, cellY ) then
		local uid, tileCellOffsetX, tileCellOffsetY = GetCellTileUidAndOffset( cellX, cellY )

		local rx, ry = InverseRotateLocal( cellX, cellY, x - cellX * CELL_SIZE * 2, y - cellY * CELL_SIZE * 2, CELL_SIZE * 2 - 1 )

		return sm.terrainTile.getClutterIdxAt( uid, tileCellOffsetX, tileCellOffsetY, rx, ry )
	else
		return -1
	end
end

----------------------------------------------------------------------------------------------------

function GetEffectMaterialAt( x, y )





	local mat0, mat1, mat2, mat3, mat4, mat5, mat6, mat7 = GetMaterialAt(x, y, 0)

	local materialWeights = {}
	materialWeights["Grass"] = math.max(mat4, mat7)
	materialWeights["Rock"] = math.max(mat0, mat2, mat5)
	materialWeights["Dirt"] = math.max(mat3, mat6)
	materialWeights["Sand"] = math.max(mat1)
	local weightThreshold = 0.25
	local selectedKey = "Grass"

	for key, weight in pairs(materialWeights) do
		if weight > materialWeights[selectedKey] and weight > weightThreshold then
			selectedKey = key
		end
	end

	return selectedKey
end

----------------------------------------------------------------------------------------------------

blueprintTable = {}

blueprintTable["Kit_RoadsideMarket_Antenna"] =						{	"$SURVIVAL_DATA/LocalBlueprints/Kit_RoadsideMarket_Antenna_01.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/Kit_RoadsideMarket_Antenna_02.blueprint" }

blueprintTable["Kit_RoadsideMarket_LightLantern"] =					{	"$SURVIVAL_DATA/LocalBlueprints/Kit_RoadsideMarket_LightLantern_01.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/Kit_RoadsideMarket_LightLantern_02.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/Kit_RoadsideMarket_LightLantern_03.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/Kit_RoadsideMarket_LightLantern_04.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/Kit_RoadsideMarket_LightLantern_05.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/Kit_RoadsideMarket_LightLantern_06.blueprint" }

blueprintTable["RoadsideMarket_Clutter_BoxWood"] =					{{	"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_BoxWood_01.blueprint", 10 },
																		"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_BoxWood_02.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_BoxWood_03.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_BoxWood_04.blueprint" }

blueprintTable["RoadsideMarket_Clutter_FruitCrateDisplay"] =		{	"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_FruitCrateDisplay_01.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_FruitCrateDisplay_02.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_FruitCrateDisplay_03.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_FruitCrateDisplay_04.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_FruitCrateDisplay_05.blueprint" }

blueprintTable["RoadsideMarket_Clutter_PottedPlant"] =				{	"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_PottedPlant_01.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_PottedPlant_02.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_PottedPlant_03.blueprint" }

blueprintTable["RoadsideMarket_Clutter_SeedBag"] =					{	"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_SeedBag_01.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/RoadsideMarket_Clutter_SeedBag_02.blueprint" }

blueprintTable["WarehouseExterior_Barrack"] =						{	"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Barrack_01.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Barrack_02.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Barrack_03.blueprint" }

blueprintTable["WarehouseExterior_Clutter_JunkCrate"] =				{	"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Clutter_JunkCrate_01.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Clutter_JunkCrate_02.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Clutter_JunkCrate_03.blueprint" }

blueprintTable["WarehouseExterior_Clutter_JunkSpill"] =				{	"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Clutter_JunkSpill_01.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Clutter_JunkSpill_02.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Clutter_JunkSpill_03.blueprint" }

blueprintTable["WarehouseExterior_Clutter_Pallet_Junk"] =			{	"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Clutter_Pallet_Junk_01.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Clutter_Pallet_Junk_02.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Clutter_Pallet_Junk_03.blueprint" }

blueprintTable["WarehouseExterior_Roof_Tower_Water"] =				{	"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Roof_Tower_Water_01.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/WarehouseExterior_Roof_Tower_Water_02.blueprint" }

blueprintTable["GameplayBlueprints/loot_structureparts_concrete"] =	{	"$SURVIVAL_DATA/LocalBlueprints/GameplayBlueprints/loot_structureparts_concrete_01.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/GameplayBlueprints/loot_structureparts_concrete_02.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/GameplayBlueprints/loot_structureparts_concrete_03.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/GameplayBlueprints/loot_structureparts_concrete_04.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/GameplayBlueprints/loot_structureparts_concrete_05.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/GameplayBlueprints/loot_structureparts_concrete_06.blueprint",
																		"$SURVIVAL_DATA/LocalBlueprints/GameplayBlueprints/loot_structureparts_concrete_07.blueprint"}

----------------------------------------------------------------------------------------------------

prefabTable = {}

prefabTable["RoadsideMarket_ShackLarge"] =							{	"$SURVIVAL_DATA/LocalPrefabs/RoadsideMarket_ShackLarge_01.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/RoadsideMarket_ShackLarge_02.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/RoadsideMarket_ShackLarge_03.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/RoadsideMarket_ShackLarge_04.prefab" }

prefabTable["RoadsideMarket_ShackMedium"] =							{	"$SURVIVAL_DATA/LocalPrefabs/RoadsideMarket_ShackMedium_01.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/RoadsideMarket_ShackMedium_02.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/RoadsideMarket_ShackMedium_03.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/RoadsideMarket_ShackMedium_04.prefab" }

prefabTable["RoadsideMarket_ShackSmall"] =							{	"$SURVIVAL_DATA/LocalPrefabs/RoadsideMarket_ShackSmall_01.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/RoadsideMarket_ShackSmall_02.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/RoadsideMarket_ShackSmall_03.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/RoadsideMarket_ShackSmall_04.prefab" }

prefabTable["gameplay_prefabs/loot_randomparts_shelf_4x4"] =		{	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_01.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_02.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_03.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_04.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_05.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_06.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_07.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_08.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_09.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_10.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_11.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_12.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_13.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_14.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_15.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_16.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_17.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x4_18.prefab" }

prefabTable["gameplay_prefabs/loot_randomparts_shelf_4x8"] =		{	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_01.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_02.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_03.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_04.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_05.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_06.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_07.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_08.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_09.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_09.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_10.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_11.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_12.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_13.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_14.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_15.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_16.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_17.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_18.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_19.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_20.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_21.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_randomparts_shelf_4x8_22.prefab" }

prefabTable["gameplay_prefabs/loot_structureparts_wood"] =			{	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_structureparts_wood_01.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_structureparts_wood_02.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_structureparts_wood_03.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_structureparts_wood_04.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_structureparts_wood_05.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_structureparts_wood_06.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_structureparts_wood_07.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_structureparts_wood_08.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_structureparts_wood_09.prefab",
																		"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/loot_structureparts_wood_10.prefab" }

prefabTable["gameplay_prefabs/random_abandoned_vehicle"] =			{{	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/random_abandoned_vehicle_01.prefab", 60 },
																	 {	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/random_abandoned_vehicle_02.prefab", 1 },
																	 {	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/random_abandoned_vehicle_03.prefab", 10 },
																	 {	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/random_abandoned_vehicle_04.prefab", 5 },
																	 {	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/random_abandoned_vehicle_05.prefab", 10 },
																	 {	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/random_abandoned_vehicle_06.prefab", 5 },
																	 {	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/random_abandoned_vehicle_07.prefab", 10 },
																	 {	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/random_abandoned_vehicle_08.prefab", 5 },
																	 {	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/random_abandoned_vehicle_09.prefab", 10 },
																	 {	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/random_abandoned_vehicle_10.prefab", 5 },
																	 {	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/random_abandoned_vehicle_11.prefab", 10 },
																	 {	"$SURVIVAL_DATA/LocalPrefabs/gameplay_prefabs/random_abandoned_vehicle_12.prefab", 5 }}


prefabTable["GameplayPrefabs/Ruin/surprisespawner_haypile"] =		{{	"$SURVIVAL_DATA/LocalPrefabs/GameplayPrefabs/Ruin/surprisespawner_haypile_01.prefab", 10 },
																	{	"$SURVIVAL_DATA/LocalPrefabs/GameplayPrefabs/Ruin/surprisespawner_haypile_02.prefab", 20 },
																	{	"$SURVIVAL_DATA/LocalPrefabs/GameplayPrefabs/Ruin/surprisespawner_haypile_03.prefab", 1 },
																	{	"$SURVIVAL_DATA/LocalPrefabs/GameplayPrefabs/Ruin/surprisespawner_haypile_04.prefab", 10 }}

----------------------------------------------------------------------------------------------------

local randomizationTables = {
	prefabTable = prefabTable,
	blueprintTable = blueprintTable
}

function PrepareCell( cellX, cellY, loadFlags, lodLevel )
	DefaultPrepareCell( cellX, cellY, loadFlags, randomizationTables )
end

----------------------------------------------------------------------------------------------------

-- Invalid asset hunt!
invalidAssets = {
}

local water_asset_uuid = sm.uuid.new( "990cce84-a683-4ea6-83cc-d0aee5e71e15" )

function GetXYPosition( object, cellX, cellY, rx, ry )
	local x, y
	if object.rootPrefabCenterPosition == nil then
		x = cellX * CELL_SIZE + rx
		y = cellY * CELL_SIZE + ry
	else
		local prefabRx, prefabRy = RotateLocal( cellX, cellY, object.rootPrefabCenterPosition.x, object.rootPrefabCenterPosition.y )
		x = cellX * CELL_SIZE + prefabRx
		y = cellY * CELL_SIZE + prefabRy
	end
	return x, y
end

function GetAssetsForCell( cellX, cellY, size )






	local prefabAssets = g_prefabAssets
	g_prefabAssets = {}

	local uid, tileCellOffsetX, tileCellOffsetY = GetCellTileUidAndOffset( cellX, cellY )
	
	if not uid:isNil() then
		local lake = isLake( cellX, cellY )
		
		local key = CalculateTileStorageKey( g_world.id, cellX, cellY ) .. SYNC_KEY
		local tileStorage = sm.terrainGeneration.loadGameStorage( key ) or {}

 		local assets = sm.terrainTile.getAssetsForCell( uid, tileCellOffsetX, tileCellOffsetY, size )
		
		concat( assets, prefabAssets )

		FilterTileStorageTaggedObjects( assets, tileStorage )
		for i = 1, #assets, 1 do
			local asset = assets[i]
			if invalidAssets[tostring( asset.uuid )] then
				sm.log.error( "Invalid asset {"..tostring( asset.uuid ).."} in tile: '"..GetTilePath( uid ).."'" )
			end

			local rx, ry = RotateLocal( cellX, cellY, asset.pos.x, asset.pos.y )
			local x, y = GetXYPosition( asset, cellX, cellY, rx, ry )

			local height = asset.pos.z + getCliffHeightAt( x, y )
			
			-- Water rotation
			if lake and asset.uuid == water_asset_uuid then
				asset.rot = sm.quat.new( 0.7071067811865475, 0.0, 0.0, 0.7071067811865475 )
			else
				height = height + getElevationHeightAt( x, y )
				asset.rot = GetRotationQuat( cellX, cellY ) * asset.rot
			end

			-- Fix surface z-fighting
			height = WaterSurfaceGridOffset( asset, height )

			asset.pos = sm.vec3.new( rx, ry, height )

			local nor = getElevationNormalAt( x, y )
			if nor.z < 0.999848 then --Slope angle > 1 deg 
				asset.slopeNormal = nor
			end
		end

		-- if the cell is in the graphical padding area, remove the water asset from the list of assets
		if cellY < g_cellData.bounds.yMin + GRAPHICS_CELL_PADDING or cellY > g_cellData.bounds.yMax - GRAPHICS_CELL_PADDING or cellX < g_cellData.bounds.xMin + GRAPHICS_CELL_PADDING or cellX > g_cellData.bounds.xMax - GRAPHICS_CELL_PADDING then
			removeFromArray( assets, function( value ) return value.uuid == water_asset_uuid end )
		end

		return assets
	end
	return {}
end

----------------------------------------------------------------------------------------------------

function GetHarvestablesForCell( cellX, cellY, size )






	local prefabHarvestables = g_prefabHarvestables
	g_prefabHarvestables = {}

	local uid, tileCellOffsetX, tileCellOffsetY = GetCellTileUidAndOffset( cellX, cellY )
	if not uid:isNil() then
		-- Load harvestables from cell
		local harvestables = sm.terrainTile.getHarvestablesForCell( uid, tileCellOffsetX, tileCellOffsetY, size )
		concat( harvestables, prefabHarvestables )

		for _, harvestable in ipairs( harvestables ) do

			local rx, ry = RotateLocal( cellX, cellY, harvestable.pos.x, harvestable.pos.y )
			local x, y = GetXYPosition( harvestable, cellX, cellY, rx, ry )
	
			local height = harvestable.pos.z + getElevationHeightAt( x, y ) + getCliffHeightAt( x, y )
			harvestable.pos = sm.vec3.new( rx, ry, height )
			harvestable.rot = GetRotationQuat( cellX, cellY ) * harvestable.rot

			local nor = getElevationNormalAt( x, y )
			if nor.z < 0.999848 then --Slope angle > 1 deg
				harvestable.slopeNormal = nor
			end
		end
		
		return harvestables
	end
	return {}
end

----------------------------------------------------------------------------------------------------

function GetKinematicsForCell( cellX, cellY, size, terrainTaskTick )






	local prefabKinematics = g_prefabKinematics
	g_prefabKinematics = {}

	local uid, tileCellOffsetX, tileCellOffsetY = GetCellTileUidAndOffset( cellX, cellY )
	if not uid:isNil() then
		-- Load kinematics from cell
		local kinematics = sm.terrainTile.getKinematicsForCell( uid, tileCellOffsetX, tileCellOffsetY, size )

		concat( kinematics, prefabKinematics )

		local tileStorageKey
		local storageData
		if #kinematics > 0 then
			tileStorageKey = CalculateTileStorageKey( g_world.id, cellX, cellY ) or nil
			storageData = sm.terrainGeneration.loadGameStorage( tileStorageKey )
		end

		local syncKey = CalculateTileStorageKey( g_world.id, cellX, cellY ) .. SYNC_KEY
		local syncedTileStorage = sm.terrainGeneration.loadGameStorage( syncKey ) or {}
		FilterTileStorageTaggedObjects( kinematics, syncedTileStorage )
		for _, kinematic in ipairs( kinematics ) do
			local rx, ry = RotateLocal( cellX, cellY, kinematic.pos.x, kinematic.pos.y )
			local x, y = GetXYPosition( kinematic, cellX, cellY, rx, ry )
			
			local height = kinematic.pos.z + getElevationHeightAt( x, y ) + getCliffHeightAt( x, y )
			kinematic.pos = sm.vec3.new( rx, ry, height )
			kinematic.rot = GetRotationQuat( cellX, cellY ) * kinematic.rot

			LoadKinematicFromStorage( kinematic, storageData, sm.vec3.new( x, y, kinematic.pos.z ), terrainTaskTick, g_world, cellX, cellY )
		end
		
		return kinematics
	end
	return {}
end

----------------------------------------------------------------------------------------------------

function GetNodesForCell( cellX, cellY )






	local nodes = g_prefabNodes
	g_prefabNodes = {}

	local uid, tileCellOffsetX, tileCellOffsetY = GetCellTileUidAndOffset( cellX, cellY )
	if not uid:isNil() then
		local cellNodes = GetNodesForCellWithCache( uid, tileCellOffsetX, tileCellOffsetY )
		for _, node in ipairs( cellNodes ) do
			nodes[#nodes + 1] = shallowcopy( node )
		end

		local lake = isLake( cellX, cellY )
		for _, node in ipairs( nodes ) do
			local rx, ry = RotateLocal( cellX, cellY, node.pos.x, node.pos.y )
			local x, y = GetXYPosition( node, cellX, cellY, rx, ry )

			local height = node.pos.z + getCliffHeightAt( x, y )
			if not ( lake and ValueExists( node.tags, "WATER" ) ) then
				height = height + getElevationHeightAt( x, y )
			end
			-- Fix surface z-fighting
			height = WaterNodeGridOffset( node, height )

			node.pos = sm.vec3.new( rx, ry, height )
			node.rot = GetRotationQuat( cellX, cellY ) * node.rot
		end
		return nodes
	end
	return {}
end

----------------------------------------------------------------------------------------------------

function GetCreationsForCell( cellX, cellY )






	local prefabCreations = g_prefabCreations
	g_prefabCreations = {}

	local uid, tileCellOffsetX, tileCellOffsetY = GetCellTileUidAndOffset( cellX, cellY )
	if not uid:isNil() then
		local cellCreations = sm.terrainTile.getCreationsForCell( uid, tileCellOffsetX, tileCellOffsetY )
		concat( cellCreations, prefabCreations )
		for i,creation in ipairs( cellCreations ) do
			RandomizeCreation( creation, blueprintTable )

			local rx, ry = RotateLocal( cellX, cellY, creation.pos.x, creation.pos.y )
			local x, y = GetXYPosition( creation, cellX, cellY, rx, ry )

			local height = creation.pos.z + getCliffHeightAt( x, y )
			if isFlat( cellX, cellY ) then
				-- Get elevation at center
				height = height + getElevationHeightAt( ( cellX + 0.5 ) * CELL_SIZE, ( cellY + 0.5 ) * CELL_SIZE )
			else
				height = height + getElevationHeightAt( x, y )
			end
			creation.pos = sm.vec3.new( rx, ry, height )
			creation.rot = GetRotationQuat( cellX, cellY ) * creation.rot
		end

		return cellCreations
	end

	return {}
end

----------------------------------------------------------------------------------------------------

function GetDecalsForCell( cellX, cellY )






	local prefabDecals = g_prefabDecals
	g_prefabDecals = {}

	local uid, tileCellOffsetX, tileCellOffsetY = GetCellTileUidAndOffset( cellX, cellY )
	if not uid:isNil() then
		local cellDecals = sm.terrainTile.getDecalsForCell( uid, tileCellOffsetX, tileCellOffsetY )
		concat( cellDecals, prefabDecals )

		local key = CalculateTileStorageKey( g_world.id, cellX, cellY ) .. SYNC_KEY
		local tileStorage = sm.terrainGeneration.loadGameStorage( key ) or {}

		FilterTileStorageTaggedObjects( cellDecals, tileStorage )

		for i = 1, #cellDecals, 1 do
			local decal = cellDecals[i]

			local rx, ry = RotateLocal( cellX, cellY, decal.pos.x, decal.pos.y )
			local x, y = GetXYPosition( decal, cellX, cellY, rx, ry )

			local height = decal.pos.z + getElevationHeightAt( x, y ) + getCliffHeightAt( x, y )
			decal.pos = sm.vec3.new( rx, ry, height )
			decal.rot = GetRotationQuat( cellX, cellY ) * decal.rot
		end

		return cellDecals
	end

	return {}
end

----------------------------------------------------------------------------------------------------

local TypeTags = { "MEADOW", "FOREST", "DESERT", "FIELD", "BURNTFOREST", "AUTUMNFOREST", "", "LAKE" }

local PoiTags = {
	[POI_WAREHOUSE2_LARGE] = "WAREHOUSE2",
	[POI_WAREHOUSE3_LARGE] = "WAREHOUSE3",
	[POI_WAREHOUSE4_LARGE] = "WAREHOUSE4",
	[POI_WAREHOUSE4_QUEST_LARGE] = "WAREHOUSE4QUEST",
	[POI_BURNTFOREST_FARMBOTSCRAPYARD_LARGE] = "SCRAPYARD",

	[POI_MEADOW_GROWLAB_QUEST_LARGE] = "MINIDUNGEON",
	[POI_MEADOW_GROWLAB_SILODISTRICT_XL] = "MINIDUNGEON",
	[POI_BURNTFOREST_GROWLAB_FROZEN_LARGE] = "MINIDUNGEON",
	[POI_FOREST_GROWLAB_STATION_LARGE] = "MINIDUNGEON",
	[POI_LAKE_GROWLAB_ISLAND_XL] = "MINIDUNGEON",
	[POI_DESERT_GROWLAB_CLIFFTOP_LARGE] = "MINIDUNGEON",



}

function GetTagsForCell( cellX, cellY )





	local tags = {}

	local type = getCellType( cellX, cellY )
	if type >= 1 and type <= 8 then
		tags[#tags + 1] = TypeTags[type]
	end

	local uid = GetCellTileUidAndOffset( cellX, cellY )
	if not uid:isNil() then
		local poiType = GetPoiType( uid )
		if poiType then
			local tag = PoiTags[poiType]
			if tag then
				tags[#tags + 1] = tag
			end
			tags[#tags + 1] = "POI"
		end
		if isRoad( cellX, cellY ) then
			tags[#tags + 1] = "ROAD"
		end
	end

	if cellX >= -46 and cellX < -46 + 20 and cellY >= -46 and cellY < -46 + 16 then
		tags[#tags + 1] = "STARTAREA"
	end

	return tags
end

function GetVoxelTerrainForCell( cellX, cellY )
	
	local uid, xOffset, yOffset = GetCellTileUidAndOffset( cellX, cellY )
	if not uid:isNil() then
		local rotation = GetCellRotation( cellX, cellY )

		sm.voxelTerrainCell.copyTileCellVoxels( uid, xOffset, yOffset, rotation )
	end

end

----------------------------------------------------------------------------------------------------

function GetCellGroups()
	if g_cellData.groupId then
		return g_cellData.groupId
	end
	return {}
end

function GenerateOceanCells()
	if g_generatorIndex == 0 then
		local result = {}
		local lakeCells = {}
		for cellY = g_cellData.bounds.yMin + GRAPHICS_CELL_PADDING, g_cellData.bounds.yMax - GRAPHICS_CELL_PADDING do
			for cellX = g_cellData.bounds.xMin + GRAPHICS_CELL_PADDING, g_cellData.bounds.xMax - GRAPHICS_CELL_PADDING do
				if isLake(cellX, cellY) then
					table.insert(result, { cellX, cellY } )
				
					local cellKey = CellKey(cellX, cellY)
					lakeCells[cellKey] = true
				end
			end
		end
	
		sm.terrainGeneration.setGameStorageNoSave( "LakeData"..g_world.id, lakeCells )
	
		g_oceanCells = result
	end
end

function GetOceanCells()
	if g_oceanCells then
		return g_oceanCells
	end
	return {}
end

function GenerateHLodCells()
	local hLods = {}
	table.insert( hLods, { 43 * CELL_SIZE - 0.5 * CELL_SIZE + 527, 20 * CELL_SIZE - 0.5 * CELL_SIZE + 526, 117.5, "eff_lod_excavationmountain" } )
	g_hLodCells = hLods
end

function GetHLodCells()
	if g_hLodCells then
		return g_hLodCells
	end
	return {}
end

----------------------------------------------------------------------------------------------------
-- Tile Reader Path Getter
----------------------------------------------------------------------------------------------------

function GetTilePath( uid )

	local tilePath = GetPath( uid )
	if tilePath then
		return tilePath
	end
	-- Not found!
	return "$SURVIVAL_DATA/Terrain/Tiles/Error_1024.tile"
end
