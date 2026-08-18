dofile( "$SURVIVAL_DATA/Scripts/util.lua" )
dofile( "$SURVIVAL_DATA/Scripts/terrain/overworld/overworld_util.lua" )
dofile( "$SURVIVAL_DATA/Scripts/terrain/overworld/processing.lua" )
dofile( "$SURVIVAL_DATA/Scripts/terrain/overworld/generate_roads.lua" )
dofile( "$SURVIVAL_DATA/Scripts/terrain/overworld/biome_roads.lua" )


ExcavationIsland = { x = 32, y = 16, worldFile = "$SURVIVAL_DATA/Terrain/Worlds/overworld_excavation_island.world", rotation = 0 }

----------------------------------------------------------------------------------------------------

function generateOverworldCelldata( xMin, xMax, yMin, yMax, seed, data, padding )

	sm.debugDraw.clear( "overworld_dd_" )

	math.randomseed( seed )

	print( "Initializing cell data" )
	initializeCellData( xMin, xMax, yMin, yMax, seed, padding ) --Zero everything

	local simplexSeed = seed % 32768

	-- Temp corner data during generate
	g_cornerTemp = {
		terrainType = {},
		gradC = {}, -- Center gradient
		forceFlat = {},
		lakeAdjacent = {},
		hillyness = {},
	}
	for y = yMin, yMax + 1 do
		g_cornerTemp.terrainType[y] = {}
		g_cornerTemp.gradC[y] = {}
		g_cornerTemp.forceFlat[y] = {}
		g_cornerTemp.lakeAdjacent[y] = {}
		g_cornerTemp.hillyness[y] = {}

		for x = xMin, xMax + 1 do
			g_cornerTemp.terrainType[y][x] = TYPE_LAKE
			g_cornerTemp.gradC[y][x] = 0
			g_cornerTemp.forceFlat[y][x] = false
			g_cornerTemp.lakeAdjacent[y][x] = false
			g_cornerTemp.hillyness[y][x] = 1
		end
	end

	local xSize = xMax + 1 - xMin
	local ySize = yMax + 1 - yMin
	local xHalfSize = xSize / 2
	local yHalfSize = ySize / 2
	local xCenter = xMin + xHalfSize
	local yCenter = yMin + yHalfSize
	print( "Size", xSize, ySize )
	print( "Center", xCenter, yCenter )

	local pois = {}

	------------------------------------------------------------------------------------------------

	for y = yMin, yMax + 1 do
		for x = xMin, xMax + 1 do
			local pad = padding + 4
			local gradC = 1 - math.min( math.max( math.abs( x - xCenter ) / ( xHalfSize - pad ), math.abs( y - yCenter ) / ( yHalfSize - pad ) ), 1 )
			assert( gradC >= 0 and gradC <= 1 )
			g_cornerTemp.gradC[y][x] = gradC
		end
	end

	local boxGrad = function( pos, center, inner, outer )
		return 1 - math.min( math.max( math.max( math.abs( pos.x - center.x ) - inner.w, 0 ) / ( outer.w - inner.w ), math.max( math.abs( pos.y - center.y ) - inner.h, 0 ) / ( outer.h - inner.h ) ), 1 )
	end

	local islandShape = function( x, y )
		local mainIsland = boxGrad( { x = x, y = y }, { x = 0, y = 0 }, { w = 70, h = 48 }, { w = 112, h = 96 } )
		local startAreaMoat = boxGrad( { x = x, y = y }, { x = -36, y = -38 }, { w = 8, h = 4 }, { w = 16, h = 16 } )
		local excavationMoat = boxGrad( { x = x, y = y }, { x = 48, y = 32 }, { w = 16, h = 16 }, { w = 24, h = 24 } )
		local mechanicStation = boxGrad( { x = x, y = y }, { x = -29, y = -26 }, { w = 3, h = 3 }, { w = 5, h = 5 } )
		local excavationBridge = boxGrad( { x = x, y = y }, { x = 27, y = 37 }, { w = 4, h = 4 }, { w = 12, h = 12 } )

		local island = mainIsland
		island = math.max( island - startAreaMoat, 0 )
		island = math.max( island - excavationMoat, 0 )
		island = math.min( island + mechanicStation, 1 )
		island = math.min( island + excavationBridge, 1 )
		assert( island >= 0 and island <= 1 )
		return island * 2 - 1
	end

	local islandNoise = function( x, y )
		local noise = sm.noise.simplexNoise2d( ( x + 256 * ( simplexSeed + 10 ) ) / 8, ( y + 256 * ( simplexSeed + 21 ) ) / 8 )
		return clamp( islandShape( x, y ) + noise, -1, 1 )
	end

	--ddBox( "world", -64, -48, 128, 96, "0000ff" )
	--ddBox( "startarea", -46, -46, 20, 16, "00ff00" )
	--ddBox( "beginning", -64, -48, 56, 40, "00ff00" )
	--ddBox( "excavation", 32, 16, 32, 32, "00ff00" )

	--ddBox( "mainIslandInner", 0, 0, 40, 24, "ff0080" )
	--ddBox( "mainIslandOuter", 0, 0, 64, 48, "ff0080" )
	--ddBox( "startAreaMoatInner", -36, -38, 9, 7, "ff0000" )
	--ddBox( "startAreaMoatOuter", -36, -38, 13, 9, "ff0000" )
	--ddBox( "excavationMoatInner", 48, 32, 16, 16, "ff0000" )
	--ddBox( "excavationMoatOuter", 48, 32, 24, 24, "ff0000" )

	local cliffNoise = function( x, y )
		--local grad1 = boxGrad( { x = x, y = y }, { x = 0, y = 5 }, { w = 50, h = 20 }, { w = 60, h = 30 } )
		--local grad2 = boxGrad( { x = x, y = y }, { x = 0, y = 5 }, { w = 40, h = 10 }, { w = 50, h = 20 } )
		--local grad3 = boxGrad( { x = x, y = y }, { x = 0, y = 5 }, { w = 30, h = 0 }, { w = 40, h = 10 } )
		local grad1 = boxGrad( { x = x, y = y }, { x = 0, y = -10 }, { w = 10, h = 25 }, { w = 15, h = 30 } )
		local grad2 = boxGrad( { x = x, y = y }, { x = 0, y = -10 }, { w = 5, h = 20 }, { w = 10, h = 25 } )
		local grad3 = boxGrad( { x = x, y = y }, { x = 0, y = -10 }, { w = 0, h = 0 }, { w = 5, h = 20 } )
		local v = 20
		local noise1 = math.abs( sm.noise.simplexNoise2d( ( x + 256 * ( simplexSeed + 123 ) ) / v, ( y + 256 * ( simplexSeed + 404 ) ) / v ) )
		local noise2 = sm.noise.simplexNoise2d( ( x + 256 * ( simplexSeed + 708 ) ) / v, ( y + 256 * ( simplexSeed + 712 ) ) / v ) * 0.5 + 0.5
		local noise3 = sm.noise.simplexNoise2d( ( x + 256 * ( simplexSeed + 240 ) ) / v, ( y + 256 * ( simplexSeed + 732 ) ) / v ) * 0.5 + 0.5
		return math.floor( grad1 + noise1 * 0.5 + 0.5 ) + math.floor( grad2 + noise2 * 0.5 + 0.5 ) + math.floor( grad3 + noise3 * 0.5 + 0.5 )
	end

	local cliffNoise = function( x, y )
		local grad1 = boxGrad( { x = x, y = y }, { x = 0, y = -15 }, { w = 0, h = 0 }, { w = 15, h = 30 } )
		local v1 = 10
		local noise1 = math.abs( sm.noise.simplexNoise2d( ( x + 256 * ( simplexSeed + 123 ) ) / v1, ( y + 256 * ( simplexSeed + 404 ) ) / v1 ) )

		local grad2 = boxGrad( { x = x, y = y }, { x = 0, y = 30 }, { w = 0, h = 0 }, { w = 20, h = 10 } )
		local v2 = 10
		local noise2 = math.abs( sm.noise.simplexNoise2d( ( x + 256 * ( simplexSeed + 123 ) ) / v2, ( y + 256 * ( simplexSeed + 404 ) ) / v2 ) )

		return math.floor( grad1 * 4 + noise1 * 1 ) + math.floor( grad2 * 4 + noise2 * 1 )
	end

	local cliffNoiseExtra = function( x, y )
		local v1 = 7
		local v2 = 11
		local noise1 = sm.noise.simplexNoise2d( ( x + 256 * ( simplexSeed + 73 ) ) / v1, ( y + 256 * ( simplexSeed + 75 ) ) / v1 )
		local noise2 = sm.noise.simplexNoise2d( ( x + 256 * ( simplexSeed + 66 ) ) / v2, ( y + 256 * ( simplexSeed + 274 ) ) / v2 )
		local noise3 = sm.noise.simplexNoise2d( ( x + 256 * ( simplexSeed + 127 ) ) / ( v2 * 2 ), ( y + 256 * ( simplexSeed + 421 ) ) / ( v2 * 2 ) )
		return noise1 > 0.5 and clamp( math.floor( math.abs( noise2 * 4 + noise3 * 2 ) ), 0, 3 ) or 0
	end

	local elevationNoise = function( x, y )
		local elevation = 0.1 + clamp( ( g_cornerTemp.gradC[y][x] * 3 - 1 ) * 0.1, 0, 1 )
		elevation = elevation + sm.noise.perlinNoise2d( x / 16, y / 16, seed + 7907 ) * clamp( ( g_cornerTemp.gradC[y][x] * 3 - 1 ), 0, 1 )
		elevation = elevation + sm.noise.perlinNoise2d( x / 8, y / 8, seed + 5527 ) * 0.5
		elevation = elevation + sm.noise.perlinNoise2d( x / 4, y / 4, seed + 8733 ) * 0.25
		elevation = elevation + sm.noise.perlinNoise2d( x / 2, y / 2, seed + 5442 ) * 0.125
		elevation = elevation * 250
		return elevation
	end




















	------------------------------------------------------------------------------------------------


	-- Crash site
	pois[#pois + 1] = { x = -36, y = -40, type = POI_CRASHSITE_AREA, size = 20, road = false, flat = false, cliffLevel = 0 } -- Only blocks random pois
	pois[#pois + 1] = { x = -35, y = -30, type = POI_ROAD_RANDOM, index = 3, rotation = 1, size = 1, road = true, flat = false, terrainType = TYPE_FOREST, cliffLevel = 0 }
	local crashSiteExit = pois[#pois]

	-- Quest
	pois[#pois + 1] = { x = -29, y = -26, type = POI_MECHANICSTATION_QUEST_MEDIUM, size = 2, road = true, flat = true, cliffLevel = 0, terrainType = TYPE_FOREST }
	local mechanicStation = pois[#pois]
	pois[#pois + 1] = { x = -16, y = -16, type = POI_HIDEOUT_XL, rotation = 0, size = 8, road = false, flat = true, cliffLevel = 0, forceFlat = true }
	pois[#pois + 1] = { x = -12, y = -29, type = POI_AUTUMNFOREST_CLEARRUINSQUEST_MEDIUM, size = 2, road = false, flat = true, cliffLevel = 0 }
	pois[#pois + 1] = { x = -33, y = 23, type = POI_BUNK_BURIAL_QUEST_MEDIUM, size = 2, road = false, flat = true }
	pois[#pois + 1] = { x = -38, y = -17, type = POI_MEADOW_GROWLAB_QUEST_LARGE, rotation = 1, size = 4, road = false, flat = true, elevationSmoothing = 4 }
	pois[#pois + 1] = { x = 32, y = -12, type = POI_WAREHOUSE4_QUEST_LARGE, size = 4, road = true, flat = true, elevationSmoothing = 4 }
	local warehouseQuest = pois[#pois]

	-- Unique
	pois[#pois + 1] = { x = -17, y = -23, type = POI_PACKINGSTATIONVEG_MEDIUM, size = 2, road = true, flat = true, cliffLevel = 0 }
	local packingStation1 = pois[#pois]
	pois[#pois + 1] = { x = 10, y = 10, type = POI_PACKINGSTATIONFRUIT_MEDIUM, size = 2, road = true, flat = true }
	local packingStation2 = pois[#pois]
	pois[#pois + 1] = { x = -23, y = -25, type = POI_BUILDERQUEST_WOCHOUSE, rotation = 0, size = 1, road = true, flat = false }
	local wocHouse = pois[#pois]
	pois[#pois + 1] = { x = 0, y = 15, type = POI_RUINCITY_XL, rotation = 0, size = 8, road = true, flat = true, forceFlat = true }
	local ruinCity = pois[#pois]
	pois[#pois + 1] = { x = 8, y = -2, type = POI_MEADOW_GROWLAB_SILODISTRICT_XL, rotation = 2, size = 8, road = true, flat = true, forceFlat = true }
	pois[#pois + 1] = { x = -41, y = 18, type = POI_BURNTFOREST_GROWLAB_FROZEN_LARGE, size = 4, road = false, flat = true, elevationSmoothing = 4 }
	pois[#pois + 1] = { x = -13, y = 29, type = POI_FOREST_GROWLAB_STATION_LARGE, size = 4, road = false, flat = true, elevationSmoothing = 4 }
	pois[#pois + 1] = { x = -60, y = 44, type = POI_LAKE_GROWLAB_ISLAND_XL, size = 8, road = false, flat = true, forceFlat = true }
	pois[#pois + 1] = { x = 5, y = -23, type = POI_DESERT_GROWLAB_CLIFFTOP_LARGE, size = 4, road = false, flat = true, elevationSmoothing = 4 }
	pois[#pois + 1] = { x = -30, y = -20, type = POI_BUILDERQUEST_RESOURCECAR, size = 1, road = true, flat = true }
	local resourceCar = pois[#pois]
	pois[#pois + 1] = { x = -48, y = 36, type = POI_CRASHEDSHIP_LARGE, rotation = 3, size = 4, road = false, flat = true }
	pois[#pois + 1] = { x = -31, y = 0, type = POI_CAMP_LARGE, rotation = 3, size = 4, road = true, flat = true, elevationSmoothing = 4 }
	local campLarge = pois[#pois]
	pois[#pois + 1] = { x = 24, y = -20, type = POI_LABYRINTH_MEDIUM, size = 2, road = false, flat = true, terrainType = TYPE_FIELD }
	pois[#pois + 1] = { x = 12, y = 24, type = POI_MECHANICSTATION_MEDIUM, size = 2, road = true, flat = true }
	pois[#pois + 1] = { x = -26, y = -22, type = POI_SERVICE_ELEVATOR, rotation = 2, size = 1, road = false, flat = true }

	pois[#pois + 1] = { x = ExcavationIsland.x + 16, y = ExcavationIsland.y + 16, type = POI_EXCAVATION, size = 32, flat = true, cliffLevel = 0 }
	pois[#pois + 1] = { x = ExcavationIsland.x - 1, y = ExcavationIsland.y + 18, type = POI_EXCAVATION_BRIDGE, rotation = 0, size = 1, road = true, flat = true, cliffLevel = 0 }
	local excavationBridge = pois[#pois]

	local largePois = {}
	largePois[#largePois + 1] = { type = POI_WAREHOUSE2_LARGE, index = 1, size = 4, road = true, flat = true, elevationSmoothing = 4 }
	largePois[#largePois + 1] = { type = POI_WAREHOUSE2_LARGE, index = 2, size = 4, road = true, flat = true, elevationSmoothing = 4 }
	largePois[#largePois + 1] = { type = POI_WAREHOUSE2_LARGE, index = 3, size = 4, road = true, flat = true, elevationSmoothing = 4 }
	largePois[#largePois + 1] = { type = POI_BURNTFOREST_FARMBOTSCRAPYARD_LARGE, index = 1, size = 4, road = false, flat = true, elevationSmoothing = 4 }
	largePois[#largePois + 1] = { type = POI_WAREHOUSE2_LARGE, index = 4, size = 4, road = true, flat = true, elevationSmoothing = 4 }
	largePois[#largePois + 1] = { type = POI_WAREHOUSE3_LARGE, index = 1, size = 4, road = true, flat = true, elevationSmoothing = 4 }
	largePois[#largePois + 1] = { type = POI_BURNTFOREST_FARMBOTSCRAPYARD_LARGE, index = 2, size = 4, road = false, flat = true, elevationSmoothing = 4 }
	largePois[#largePois + 1] = { type = POI_WAREHOUSE3_LARGE, index = 1, size = 4, road = true, flat = true, elevationSmoothing = 4 }
	largePois[#largePois + 1] = { type = POI_WAREHOUSE4_LARGE, index = 1, size = 4, road = true, flat = true, elevationSmoothing = 4 }
	largePois[#largePois + 1] = { type = POI_WAREHOUSE4_LARGE, index = 1, size = 4, road = true, flat = true, elevationSmoothing = 4 }

	local chemIndexes = { 1, 2, 3 }
	shuffle( chemIndexes )
	local mustHavePois = {}
	mustHavePois[#mustHavePois + 1] = { type = POI_BUILDERQUEST_CARDBOARDPOOP, size = 1, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_BURNTFOREST_BUILDERQUEST_TOTEBOTKEY, size = 1, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_FIELD_BUILDERQUEST_CORNHEART, size = 1, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_CHEMLAKE_MEDIUM, index = chemIndexes[1], size = 2, road = false, flat = true }
	mustHavePois[#mustHavePois + 1] = { type = POI_FIELD_BUILDERQUEST_COZYBED, size = 1, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_BUILDERQUEST_XYLOPHONE, size = 1, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_BUILDERQUEST_BEESUIT, size = 1, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_DESERT_BUILDERQUEST_BIGFAN, size = 1, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_CHEMLAKE_MEDIUM, index = chemIndexes[2], size = 2, road = false, flat = true }
	mustHavePois[#mustHavePois + 1] = { type = POI_BUILDERQUEST_CAROUSEL, size = 1, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_BURNTFOREST_BUILDERQUEST_CATAPULT_MEDIUM, size = 2, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_BUILDERQUEST_CROWBAR, size = 1, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_BUILDERQUEST_COMPASS, size = 1, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_DESERT_BUILDERQUEST_GARDEN, size = 1, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_CHEMLAKE_MEDIUM, index = chemIndexes[3], size = 2, road = false, flat = true }
	mustHavePois[#mustHavePois + 1] = { type = POI_FOREST_BUILDERQUEST_SAWBLADEARM, size = 1, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_AUTUMNFOREST_BUILDERQUEST_POPCORN, size = 1, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_AUTUMNFOREST_BUILDERQUEST_MUSICBOX_MEDIUM, size = 2, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_BUILDERQUEST_NICEHOUSE_MEDIUM, size = 2, rotation = 0, road = false, flat = false }
	mustHavePois[#mustHavePois + 1] = { type = POI_BUILDERQUEST_SLEDGEHAMMER_MEDIUM, size = 2, rotation = 0, road = false, flat = true }
	mustHavePois[#mustHavePois + 1] = { type = POI_BUILDERQUEST_STEELBRIDGE_MEDIUM, size = 2, rotation = 0, road = false, flat = true }
	mustHavePois[#mustHavePois + 1] = { type = POI_BUILDERQUEST_BAGUETTE_MEDIUM, size = 2, rotation = 0, road = false, flat = true }
	mustHavePois[#mustHavePois + 1] = { type = POI_OILLAKE_MEDIUM, size = 2, road = false, flat = true }

	------------------------------------------------------------------------------------------------

	for _,poi in ipairs( pois ) do
		assert( poi.x + math.floor( poi.size / 2 ) > xMin, "out of bounds"..poi.type.." x="..poi.x.."y="..poi.y )
		assert( poi.x - math.floor( poi.size / 2 ) < xMax, "out of bounds"..poi.type.." x="..poi.x.."y="..poi.y )
		assert( poi.y + math.floor( poi.size / 2 ) > yMin, "out of bounds"..poi.type.." x="..poi.x.."y="..poi.y )
		assert( poi.y - math.floor( poi.size / 2 ) < yMax, "out of bounds"..poi.type.." x="..poi.x.."y="..poi.y )
	end

	------------------------------------------------------------------------------------------------

	local TypeTags = { "MEADOW", "FOREST", "DESERT", "FIELD", "BURNTFOREST", "AUTUMNFOREST", "", "LAKE" }
	local terrainTypes = { TYPE_MEADOW, TYPE_FOREST, TYPE_DESERT, TYPE_FIELD, TYPE_BURNTFOREST, TYPE_AUTUMNFOREST, TYPE_LAKE }
	local typeRatios = { [TYPE_MEADOW] = 2, [TYPE_FOREST] = 2, [TYPE_DESERT] = 1, [TYPE_FIELD] = 1, [TYPE_BURNTFOREST] = 1, [TYPE_AUTUMNFOREST] = 1, [TYPE_LAKE] = 2 }
	local typeRatioSum = 0
	for i = 1,#terrainTypes do
		typeRatioSum = typeRatioSum + typeRatios[terrainTypes[i]]
	end
	for i = 1,#terrainTypes do
		typeRatios[terrainTypes[i]] = typeRatios[terrainTypes[i]] / typeRatioSum
	end

	local typeCounts = { [TYPE_MEADOW] = 0, [TYPE_FOREST] = 0, [TYPE_DESERT] = 0, [TYPE_FIELD] = 0, [TYPE_BURNTFOREST] = 0, [TYPE_AUTUMNFOREST] = 0, [TYPE_LAKE] = 0 }
	local typeCountSum = 0
	
	local selectTerrainType = function( forcedType )
		local selectedTerrainType
		if forcedType then
			selectedTerrainType = forcedType
		elseif typeCountSum > 0 then
			local min = 1
			for i = 1,#terrainTypes do
				local terrainType = terrainTypes[i]
				local rel = typeCounts[terrainType] / typeCountSum / typeRatios[terrainType]
				if rel < min then
					selectedTerrainType = terrainType
					min = rel
				end
			end
			assert( selectedTerrainType ~= nil )
		else
			selectedTerrainType = TYPE_FOREST
		end
		typeCounts[selectedTerrainType] = typeCounts[selectedTerrainType] + 1
		typeCountSum = typeCountSum + 1
		return selectedTerrainType
	end

	------------------------------------------------------------------------------------------------

	-- Set missing poi terrain type from poi type for fixed pois
	for _,poi in ipairs( pois ) do
		if poi.terrainType == nil then
			poi.terrainType = math.max( math.floor( poi.type / 100 ), 1 )
		end
		selectTerrainType( poi.terrainType ) -- Adds terrainType to counter
	end

	------------------------------------------------------------------------------------------------

	-- Large randomly placed pois
	local largePoiSpots = {}
	for i = 1,5 do
		for j = 1,5 + ( i - 1 ) % 2 do
			local spot = {
				x = j * 20 - 70 + i % 2 * 10,
				y = i * 16 - 48
			}
			if ( i > 2 or j > 3 ) and not collides( spot.x, spot.y, 8, pois ) then
				largePoiSpots[#largePoiSpots + 1] = spot
			end
		end
	end
	print( "#largePoiSpots", #largePoiSpots )

	table.sort( largePoiSpots, function(a, b) return ( a.x + a.y + sm.noise.intNoise2d( a.x, a.y, seed + 345 ) % 75 ) < ( b.x + b.y + sm.noise.intNoise2d( b.x, b.y, seed + 345 ) % 75 ) end )
	--table.sort( largePoiSpots, function(a, b) return ( a.x + a.y ) < ( b.x + b.y ) end )

	assert( #largePoiSpots >= #largePois )
	print( "#largePois", #largePois )

	while #largePoiSpots > #largePois do
		table.remove( largePoiSpots, math.random( #largePoiSpots ) )
	end

	for i = 1, #largePois do
		local large = largePois[i]
		local spot = largePoiSpots[i]
		local poi = {
			x = spot.x,
			y = spot.y,
			type = large.type,
			index = large.index,
			rotation = large.rotation,
			size = large.size,
			road = large.road,
			flat = large.flat,
			forceFlat = large.forceFlat,
			elevationSmoothing = large.elevationSmoothing,
			terrainType = selectTerrainType( math.floor( large.type / 100 ) ) -- Adds terrainType to counter
		}
		--ddBox( "largepoi", poi.x - 2, poi.y - 2, 4, 4, "ff8000" )
		pois[#pois + 1] = poi
	end

	-- Connect a road to all unique and large pois tagged with road
	-- Exclude pois that are going to have explicit road generation
	local roadDestinations = {}
	for _, poi in ipairs( pois ) do
		if poi.road and not isAnyOf( poi, { crashSiteExit, mechanicStation, wocHouse, resourceCar, packingStation1, packingStation2, warehouseQuest, excavationBridge } ) then
			roadDestinations[#roadDestinations + 1] = poi
		end
	end

	------------------------------------------------------------------------------------------------

	-- Medium poi spots
	local mustHaveSpots = {}
	local smallAndMediumPoiSpots = {}
	for i = 1,15 do
		for j = 1,21 do
			local noise = sm.noise.intNoise2d( j, i, seed + 557 )
			local x = j * 6 - 66 + noise % 3 - 1
			local y = i * 6 - 50 + ( j % 2 ) * 3
			if not collides( x, y, 8, pois ) then
				local spot = { x = x, y = y }
				local beginning = x < -8 and y < -8
				if islandShape( x , y ) > 0.25 and not beginning then
					mustHaveSpots[#mustHaveSpots + 1] = spot
				else
					smallAndMediumPoiSpots[#smallAndMediumPoiSpots + 1] = spot
				end
			end
		end
	end
	print( "#mustHaveSpots", #mustHaveSpots )
	print( "#smallAndMediumPoiSpots", #smallAndMediumPoiSpots )

	table.sort( mustHaveSpots, function( a, b ) return ( a.x + a.y + sm.noise.intNoise2d( a.x, a.y, seed + 653 ) % 24 ) < ( b.x + b.y + sm.noise.intNoise2d( b.x, b.y, seed + 653 ) % 24 ) end )
	--table.sort( mustHaveSpots, function( a, b ) return ( a.x + a.y ) < ( b.x + b.y ) end )

	print( "#mustHavePois", #mustHavePois )

	local usedMustHaveSpotIndices = {}
	do
		local step = #mustHaveSpots / #mustHavePois
		local first = math.random( math.ceil( step ) )
		for i = 1, #mustHavePois do
			usedMustHaveSpotIndices[#usedMustHaveSpotIndices + 1] = math.floor( ( i - 1 ) * step ) + first
		end
	end

	local smallAndMediumMustHavePoiCount = 0
	for i = 1, #mustHavePois do
		local mustHave = mustHavePois[i]
		local spot = mustHaveSpots[usedMustHaveSpotIndices[i]]
		local xOffset = mustHave.size == 1 and -( sm.noise.intNoise2d( spot.x, spot.y, g_cellData.seed + 852 ) % 2 ) or 0
		local yOffset = mustHave.size == 1 and -( sm.noise.intNoise2d( spot.x, spot.y, g_cellData.seed + 299 ) % 2 ) or 0
		local poi = {
			x = spot.x + xOffset,
			y = spot.y + yOffset,
			type = mustHave.type,
			index = mustHave.index,
			rotation = mustHave.rotation,
			size = mustHave.size,
			road = mustHave.road,
			flat = mustHave.flat,
			forceFlat = mustHave.forceFlat,
			elevationSmoothing = mustHave.elevationSmoothing,
			terrainType = selectTerrainType( math.floor( mustHave.type / 100 ) ) -- Adds terrainType to counter
		}
		if poi.size == 1 then
			--ddBox( "musthavepoi", poi.x, poi.y, 1, 1, "0000ff" )
		else
			--ddBox( "musthavepoi", poi.x - 1, poi.y - 1, 2, 2, "0000ff" )
		end
		pois[#pois + 1] = poi
		smallAndMediumMustHavePoiCount = smallAndMediumMustHavePoiCount + 1
	end
	print( "Small/medium must have pois:", smallAndMediumMustHavePoiCount )

	-- Transfers unused mustHaveSpots to smallAndMediumPoiSpots
	do
		local index = 1
		for i = 1, #mustHaveSpots do
			if index <= #usedMustHaveSpotIndices and i == usedMustHaveSpotIndices[index] then
				index = index + 1
			else
				smallAndMediumPoiSpots[#smallAndMediumPoiSpots + 1] = mustHaveSpots[i]
			end
		end
		print( "#smallAndMediumPoiSpots", #smallAndMediumPoiSpots )
	end

	print("-------------------------")
	print( "Prefined poi terrain type ratios:" )
	for i = 1,#terrainTypes do
		print( TypeTags[terrainTypes[i]], string.format( "%.2f%% / %.2f%%", typeCounts[terrainTypes[i]] / typeCountSum * 100, typeRatios[terrainTypes[i]] * 100 ) )
	end
	print( typeCountSum, "samples" )
	print("-------------------------")

	do
		local smallAndMediumPoiCount = 0
		table.sort( smallAndMediumPoiSpots, function( a, b ) return ( a.x + a.y + sm.noise.intNoise2d( a.x, a.y, seed + 603 ) % 24 ) < ( b.x + b.y + sm.noise.intNoise2d( b.x, b.y, seed + 603 ) % 24 ) end )
		--table.sort( smallAndMediumPoiSpots, function( a, b ) return ( a.x + a.y ) < ( b.x + b.y ) end )

		local beginningCounter = 0
		for _, spot in ipairs( smallAndMediumPoiSpots ) do
			local poi = {
				x = spot.x,
				y = spot.y,
				type = POI_RANDOM_PLACEHOLDER,
				size = 2, -- Can change to 1 later
				road = true, -- Can change to false later
				flat = false, -- Can change to true later
				terrainType = nil
			}
			if islandNoise( poi.x, poi.y ) >= 0 then
				local beginning = poi.x < -8 and poi.y < -8
				local forcedType = nil
				if beginning then
					local beginningTypes = { TYPE_MEADOW, TYPE_FOREST, TYPE_MEADOW, TYPE_FIELD }
					forcedType = beginningTypes[beginningCounter % #beginningTypes + 1]
					beginningCounter = beginningCounter + 1
				end
				poi.terrainType = selectTerrainType( forcedType )
			else
				poi.terrainType = TYPE_LAKE
			end
			pois[#pois + 1] = poi
			smallAndMediumPoiCount = smallAndMediumPoiCount + 1
		end
		print( "Small/medium random pois:", smallAndMediumPoiCount )
	end

	print("-------------------------")
	print( "Poi terrain type ratio after balancing:" )
	for i = 1,#terrainTypes do
		print( TypeTags[terrainTypes[i]], string.format( "%.2f%% / %.2f%%", typeCounts[terrainTypes[i]] / typeCountSum * 100, typeRatios[terrainTypes[i]] * 100 ) )
	end
	print( typeCountSum, "samples" )
	print("-------------------------")

	for _,poi in ipairs( pois ) do
		assert( poi.x + math.floor( poi.size / 2 ) > xMin, "out of bounds"..poi.type.." x="..poi.x.."y="..poi.y )
		assert( poi.x - math.floor( poi.size / 2 ) < xMax, "out of bounds"..poi.type.." x="..poi.x.."y="..poi.y )
		assert( poi.y + math.floor( poi.size / 2 ) > yMin, "out of bounds"..poi.type.." x="..poi.x.."y="..poi.y )
		assert( poi.y - math.floor( poi.size / 2 ) < yMax, "out of bounds"..poi.type.." x="..poi.x.."y="..poi.y )
	end

	------------------------------------------------------------------------------------------------

	-- Set poi cliff height
	for _,poi in ipairs( pois ) do
		if not poi.cliffLevel then
			poi.cliffLevel = cliffNoise( poi.x, poi.y )
		end
	end

	------------------------------------------------------------------------------------------------

	-- Get terrain type and cliff level from closest poi
	forEveryCorner( function( x, y )
		if islandNoise( x, y ) >= 0 then
			local poi = closestPoi( pois, x, y )
			g_cornerTemp.terrainType[y][x] = ( poi.terrainType == TYPE_LAKE and islandShape( x, y ) < 1 ) and TYPE_MEADOW or poi.terrainType
			g_cellData.cliffLevel[y][x] = poi.cliffLevel or 0







		else
			g_cornerTemp.terrainType[y][x] = TYPE_LAKE
			g_cellData.cliffLevel[y][x] = 0
		end
	end, padding )

	------------------------------------------------------------------------------------------------

	-- Compute graph of possible roads
	local roadPois = {}
	preparePoiRoadGraph( pois, roadPois )

	-- Find road paths through graph
	local roadEdges = {}
	findRoadPath( roadPois, roadEdges, crashSiteExit, mechanicStation )
	findRoadPath( roadPois, roadEdges, mechanicStation, wocHouse )
	findRoadPath( roadPois, roadEdges, wocHouse, packingStation1 )
	findRoadPath( roadPois, roadEdges, wocHouse, resourceCar )
	findRoadPath( roadPois, roadEdges, resourceCar, campLarge )
	findRoadPath( roadPois, roadEdges, packingStation1, warehouseQuest )
	findRoadPath( roadPois, roadEdges, warehouseQuest, ruinCity )
	findRoadPath( roadPois, roadEdges, ruinCity, packingStation2 )
	findRoadPath( roadPois, roadEdges, packingStation2, excavationBridge )

	shuffle( roadDestinations )
	local from = packingStation2
	for _,to in ipairs( roadDestinations ) do
		findRoadPath( roadPois, roadEdges, from, to )
		from = to
	end

	------------------------------------------------------------------------------------------------

	-- Draw roads between pois and convert placeholder pois to road pois
	for _,poi in ipairs( roadPois ) do
		poi.dist = math.huge -- Requred by writeDistancesInNodes
	end
	writeDistancesInNodes( nil, crashSiteExit ) -- Calculates distances from crash site exit
	table.sort( roadEdges, function( a, b ) return math.min( a.a.dist, a.b.dist ) < math.min( b.a.dist, b.b.dist ) end )

	local roadNodes = drawRoads( roadEdges, pois )

	------------------------------------------------------------------------------------------------

	-- Process placeholder pois not converted to road pois
	convertPlaceholderPois( pois )

	for _,poi in ipairs( pois ) do
		assert( poi.type ~= POI_RANDOM_PLACEHOLDER )
		assert( poi.type ~= nil )
	end

	------------------------------------------------------------------------------------------------

	-- Add all pois to cell data
	for _,poi in ipairs( pois ) do
		if poi.type ~= POI_CRASHSITE_AREA then
			flattenPoiCliff( poi )
			writePoi( poi, padding )
		end
	end

	-- Crash site and nearby pois
	writeStartArea( pois, roadNodes )

	injectExcavation( ExcavationIsland )

	------------------------------------------------------------------------------------------------

	local isTerrainType = function( x, y, terrainType )
		if insideCornerBounds( x, y ) then
			return g_cornerTemp.terrainType[y][x] == terrainType
		end
		return false
	end

	------------------------------------------------------------------------------------------------

	-- Max 3 cliff level difference on a cell, 1 on roads
	enforceCliffRoadLimitations( roadNodes )

	-- Extra cliffs
	forEveryCorner( function( x, y )
		if not collides( x, y, 2, pois ) then
			local allPass = true
			local minCliff = g_cellData.cliffLevel[y][x]
			for y1 = y - 1, y + 1 do
				for x1 = x - 1, x + 1 do
					if not isTerrainType( x1, y1, TYPE_MEADOW ) then
						allPass = false
						break
					end
					minCliff = g_cellData.cliffLevel[y1][x1] < minCliff and g_cellData.cliffLevel[y1][x1] or minCliff
				end
			end
			for y1 = y - 1, y do
				for x1 = x - 1, x do
					if hasRoad( x1, y1, roadNodes ) then
						allPass = false
						break
					end
				end
			end

			if allPass then
				local extra = cliffNoiseExtra( x, y )
				g_cellData.cliffLevel[y][x] = minCliff + extra











			end
		end
	end, padding )

	-- Evaluate road and cliff cells
	evaluateRoadsAndCliffs( roadNodes )

	














	-- Cliffs as TYPE_MEADOW
	forEveryCell( function( cellX, cellY )
		if bit.band( g_cellData.flags[cellY][cellX], MASK_CLIFF ) ~= 0 then
			g_cornerTemp.terrainType[cellY][cellX] = TYPE_MEADOW
			g_cornerTemp.terrainType[cellY][cellX + 1] = TYPE_MEADOW
			g_cornerTemp.terrainType[cellY + 1][cellX] = TYPE_MEADOW
			g_cornerTemp.terrainType[cellY + 1][cellX + 1] = TYPE_MEADOW
		end
	end )

	-- All corner types must be adjacent to meadow
	addBorderingMeadows()

	local potentialRoadPoiSpot = {}
	do











		local startNode = roadNodes[crashSiteExit.y][crashSiteExit.x]



		local nodesToVisit = { startNode }
		local visited = { [startNode] = true }

		local backtrack = {}
		local ends = {}

		local function reverseBacktrack( node, collision )
			local oldParent = backtrack[node].p
			if backtrack[oldParent] then
				backtrack[oldParent].childs = backtrack[oldParent].childs - 1
			end
			backtrack[collision].childs = backtrack[collision].childs + 1
			backtrack[node].p = collision
			backtrack[node].backtrackId = backtrack[collision].backtrackId

			local function updateChildBacktrackIds( node )
				local backtrackId = backtrack[node].backtrackId
				for _,edge in ipairs( node.edges ) do
					if backtrack[edge.n] and backtrack[edge.n].p == node and backtrack[edge.n].backtrackId ~= backtrackId then
						backtrack[edge.n].backtrackId = backtrackId
						updateChildBacktrackIds( edge.n )
					end
				end
			end
			updateChildBacktrackIds( node )
			if oldParent then
				reverseBacktrack( oldParent, node )
			else
				backtrack[node].childs = backtrack[node].childs + 1 -- end counts as child
				ends[#ends + 1] = node
			end
		end

		local backtrackCount = 0

		while #nodesToVisit > 0 do
			local node = nodesToVisit[#nodesToVisit]
			nodesToVisit[#nodesToVisit] = nil

			for _,edge in ipairs( node.edges ) do
				if ( edge.road or edge.shortcut ) then
					if not visited[edge.n] then
						visited[edge.n] = true
						if not getBiomeRoadTile( edge.n ) then -- Change the far edge to meadow and try again
							local dx = edge.n.x - node.x
							local dy = edge.n.y - node.y
							if dx == -1 and dy == 0 then
								g_cornerTemp.terrainType[edge.n.y][edge.n.x] = TYPE_MEADOW
								g_cornerTemp.terrainType[edge.n.y + 1][edge.n.x] = TYPE_MEADOW
							elseif dx == 1 and dy == 0 then
								g_cornerTemp.terrainType[edge.n.y][edge.n.x + 1] = TYPE_MEADOW
								g_cornerTemp.terrainType[edge.n.y + 1][edge.n.x + 1] = TYPE_MEADOW
							elseif dx == 0 and dy == -1 then
								g_cornerTemp.terrainType[edge.n.y][edge.n.x] = TYPE_MEADOW
								g_cornerTemp.terrainType[edge.n.y][edge.n.x + 1] = TYPE_MEADOW
							elseif dx == 0 and dy == 1 then
								g_cornerTemp.terrainType[edge.n.y + 1][edge.n.x] = TYPE_MEADOW
								g_cornerTemp.terrainType[edge.n.y + 1][edge.n.x + 1] = TYPE_MEADOW
							end
						end
						if getBiomeRoadTile( edge.n ) then
							assert( backtrack[edge.n] == nil )
							if backtrack[node] then
								local backtrackId = backtrack[node].backtrackId
								backtrack[node].childs = backtrack[node].childs + 1
								backtrack[edge.n] = { p = node, childs = 0, p0 = node, backtrackId = backtrackId }
							else -- Start
								backtrackCount = backtrackCount + 1
								backtrack[edge.n] = { p = nil, childs = 0, p0 = node, backtrackId = backtrackCount }
							end
						else



							g_cornerTemp.terrainType[edge.n.y][edge.n.x + 1] = TYPE_MEADOW
							g_cornerTemp.terrainType[edge.n.y][edge.n.x] = TYPE_MEADOW
							g_cornerTemp.terrainType[edge.n.y + 1][edge.n.x + 1] = TYPE_MEADOW
							g_cornerTemp.terrainType[edge.n.y + 1][edge.n.x] = TYPE_MEADOW
							if backtrack[node] then
								backtrack[node].childs = backtrack[node].childs + 1 -- end counts as child
								ends[#ends + 1] = node
							end
							local roadCliffBits = bit.band( g_cellData.flags[edge.n.y][edge.n.x], MASK_ROADCLIFF )
							if roadCliffBits == MASK_ROADS_SN or roadCliffBits == MASK_ROADS_WE then
								potentialRoadPoiSpot[#potentialRoadPoiSpot + 1] = { x = edge.n.x, y = edge.n.y }
							end
						end

						nodesToVisit[#nodesToVisit + 1] = edge.n
					else
						if backtrack[node] then
							if backtrack[edge.n] and backtrack[node].backtrackId ~= backtrack[edge.n].backtrackId then
								reverseBacktrack( node, edge.n )
								backtrack[node].childs = backtrack[node].childs + 1 -- end counts as child
								ends[#ends + 1] = node
							elseif edge.n ~= backtrack[node].p0 then
								backtrack[node].childs = backtrack[node].childs + 1 -- end counts as child
								ends[#ends + 1] = node
							end
						end
					end
				end
			end
		end

		local biomeRoadCount = 0
		for _,start in ipairs( ends ) do
			backtrack[start].childs = backtrack[start].childs - 1

			local node = start
			while node do
				if backtrack[node].childs > 0 then
					break -- Wait for other backtracked roads
				end
				local biomeRoadTile = getBiomeRoadTile( node )
				if biomeRoadTile then
					local noise = sm.noise.intNoise2d( node.x, node.y, g_cellData.seed + 499 )
					g_cellData.uid[node.y][node.x] = biomeRoadTile.tiles[noise % #biomeRoadTile.tiles + 1]
					g_cellData.rotation[node.y][node.x] = biomeRoadTile.rotation
					g_cellData.xOffset[node.y][node.x] = 0
					g_cellData.yOffset[node.y][node.x] = 0
					if biomeRoadTile.terrainType then
						g_cellData.flags[node.y][node.x] = bit.bor( bit.band( g_cellData.flags[node.y][node.x], bit.bnot( MASK_TERRAINTYPE ) ), bit.band( bit.lshift( biomeRoadTile.terrainType, SHIFT_TERRAINTYPE ), MASK_TERRAINTYPE ) )
					end
					biomeRoadCount = biomeRoadCount + 1
				else
					g_cornerTemp.terrainType[node.y][node.x] = TYPE_MEADOW
					g_cornerTemp.terrainType[node.y][node.x + 1] = TYPE_MEADOW
					g_cornerTemp.terrainType[node.y + 1][node.x] = TYPE_MEADOW
					g_cornerTemp.terrainType[node.y + 1][node.x + 1] = TYPE_MEADOW
					local roadCliffBits = bit.band( g_cellData.flags[node.y][node.x], MASK_ROADCLIFF )
					if roadCliffBits == MASK_ROADS_SN or roadCliffBits == MASK_ROADS_WE then
						potentialRoadPoiSpot[#potentialRoadPoiSpot + 1] = { x = node.x, y = node.y }
					end
				end



				local parent = backtrack[node].p
				backtrack[node] = nil
				if parent then
					assert( backtrack[parent], "Parent not found in backtrack list at: "..parent.x.." "..parent.y )
					backtrack[parent].childs = backtrack[parent].childs - 1
				else
					break
				end
				node = parent
			end
		end
		print( "Biome road count:", biomeRoadCount )







	end

	shuffle( potentialRoadPoiSpot )
	local randomRoadPoiCount = 0
	for _, spot in ipairs( potentialRoadPoiSpot ) do
		if not collides( spot.x, spot.y, 3, pois ) then
			local roadBits = bit.band( g_cellData.flags[spot.y][spot.x], MASK_ROADS )
			local poi = {
				x = spot.x,
				y = spot.y,
				type = POI_ROAD_RANDOM,
				size = 1,
				rotation = ( roadBits == MASK_ROADS_SN and 1 or 0 ) + ( sm.noise.intNoise2d( spot.x, spot.y, g_cellData.seed + 211 ) % 2 ) * 2,
				road = true,
				flat = false,
				terrainType = TYPE_MEADOW,
				edges = {}
			}
			writePoi( poi, padding )
			pois[#pois + 1] = poi
			randomRoadPoiCount = randomRoadPoiCount + 1
			--ddBox( "roadpoi", spot.x, spot.y, 1, 1, "ffffff" )
		end
		if randomRoadPoiCount >= 20 then
			break -- Limit the number of random road pois
		end
	end
	print( "Random road pois:", randomRoadPoiCount )

	------------------------------------------------------------------------------------------------

	-- Elevation (hills)

	forEveryCorner( function( x, y )
		if isTerrainType( x - 1, y - 1, TYPE_LAKE ) or isTerrainType( x, y - 1, TYPE_LAKE ) or isTerrainType( x + 1, y - 1, TYPE_LAKE )
		or isTerrainType( x - 1, y, TYPE_LAKE ) or isTerrainType( x, y, TYPE_LAKE ) or isTerrainType( x + 1, y, TYPE_LAKE )
		or isTerrainType( x - 1, y + 1, TYPE_LAKE ) or isTerrainType( x, y + 1, TYPE_LAKE ) or isTerrainType( x + 1, y + 1, TYPE_LAKE ) then
			g_cornerTemp.lakeAdjacent[y][x] = true
			g_cornerTemp.hillyness[y][x] = -0.2
		else
			g_cornerTemp.lakeAdjacent[y][x] = false
		end
	end, padding )

	for _,poi in ipairs( pois ) do
		if poi.type ~= POI_CRASHSITE_AREA then
			setForcedAndLakeAdjacentPoiHillynessToZero( poi )
		end
	end

	for y = yMin, yMax + 1 do
		-- Sweep west to east
		for x = xMin + 1, xMax + 1 do
			local hillyness = g_cornerTemp.hillyness[y][x]
			hillyness = math.min( hillyness, g_cornerTemp.hillyness[y][x - 1] + 0.2 )
			if y > yMin then
				hillyness = math.min( hillyness, g_cornerTemp.hillyness[y - 1][x - 1] + 0.2 )
			end
			if y < yMax + 1 then
				hillyness = math.min( hillyness, g_cornerTemp.hillyness[y + 1][x - 1] + 0.2 )
			end
			g_cornerTemp.hillyness[y][x] = hillyness
		end
		-- Sweep east to west
		for x = xMax, xMin + 1, -1 do
			local hillyness = g_cornerTemp.hillyness[y][x]
			hillyness = math.min( hillyness, g_cornerTemp.hillyness[y][x + 1] + 0.2 )
			if y > yMin then
				hillyness = math.min( hillyness, g_cornerTemp.hillyness[y - 1][x + 1] + 0.2 )
			end
			if y < yMax + 1 then
				hillyness = math.min( hillyness, g_cornerTemp.hillyness[y + 1][x + 1] + 0.2 )
			end
			g_cornerTemp.hillyness[y][x] = hillyness
		end
	end
	for x = xMin, xMax + 1 do
		-- Sweep south to north
		for y = yMin + 1, yMax + 1 do
			local hillyness = g_cornerTemp.hillyness[y][x]
			hillyness = math.min( hillyness, g_cornerTemp.hillyness[y - 1][x] + 0.2 )
			if x > xMin then
				hillyness = math.min( hillyness, g_cornerTemp.hillyness[y - 1][x - 1] + 0.2 )
			end
			if x < xMax + 1 then
				hillyness = math.min( hillyness, g_cornerTemp.hillyness[y - 1][x + 1] + 0.2 )
			end
			g_cornerTemp.hillyness[y][x] = hillyness
		end
		-- Sweep north to south
		for y = yMax, yMin + 1, -1 do
			local hillyness = g_cornerTemp.hillyness[y][x]
			hillyness = math.min( hillyness, g_cornerTemp.hillyness[y + 1][x] + 0.2 )
			if x > xMin then
				hillyness = math.min( hillyness, g_cornerTemp.hillyness[y + 1][x - 1] + 0.2 )
			end
			if x < xMax + 1 then
				hillyness = math.min( hillyness, g_cornerTemp.hillyness[y + 1][x + 1] + 0.2 )
			end
			g_cornerTemp.hillyness[y][x] = hillyness
		end
	end

	forEveryCorner( function( x, y )
		local hillyness = clamp( g_cornerTemp.hillyness[y][x], 0, 1 )
		g_cellData.elevation[y][x] = elevationNoise( x, y ) * hillyness
	end, padding )

	for _,poi in ipairs( pois ) do
		if poi.type ~= POI_CRASHSITE_AREA then
			smoothPoiElevation( poi )
		end
	end

	forEveryCorner( function( x, y )
		local hillyness = clamp( g_cornerTemp.hillyness[y][x], 0, 1 )
		g_cellData.elevation[y][x] = g_cellData.elevation[y][x] * hillyness
	end, padding )

	for _,poi in ipairs( pois ) do
		if poi.type ~= POI_CRASHSITE_AREA then
			flattenPoiElevation( poi )
		end
	end

	------------------------------------------------------------------------------------------------
















	-- Add additional medium/small pois
	addExtraPois( pois, padding )

	------------------------------------------------------------------------------------------------

	-- Evaluate standard terrain cells
	evaluateType( TYPE_MEADOW, getMeadowTileIdAndRotation )
	evaluateType( TYPE_FOREST, getForestTileIdAndRotation )
	evaluateType( TYPE_DESERT, getDesertTileIdAndRotation )
	evaluateType( TYPE_FIELD, getFieldTileIdAndRotation )
	evaluateType( TYPE_BURNTFOREST, getBurntForestTileIdAndRotation )
	evaluateType( TYPE_AUTUMNFOREST, getAutumnForestTileIdAndRotation )
	evaluateType( TYPE_LAKE, getLakeTileIdAndRotation )


	for y = yMin, yMax do
		for x = xMin, xMax do
			if g_cellData.uid[y][x]:isNil() then
				sm.log.error( "Nil cell at: "..x.." "..y )
				ddDownArrow( "nilcell", x, y, "ff0000" )
			end
		end
	end























































































	------------------------------------------------------------------------------------------------













































































































	------------------------------------------------------------------------------------------------

	-- Clear temps
	g_cornerTemp = nil
end
