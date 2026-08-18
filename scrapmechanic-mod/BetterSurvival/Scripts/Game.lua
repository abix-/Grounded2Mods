dofile "$CONTENT_DATA/Scripts/SurvivalGame.lua"

Game = class( SurvivalGame )

Game.enableRestrictions = true
Game.defaultInventorySize = 1000

function Game.server_onCreate( self )
    SurvivalGame.server_onCreate( self )
end
