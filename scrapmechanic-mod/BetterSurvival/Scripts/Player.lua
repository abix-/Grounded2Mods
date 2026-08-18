dofile "$SURVIVAL_DATA/Scripts/game/SurvivalPlayer.lua"

Player = class( SurvivalPlayer )

function Player.server_onCreate( self )
    SurvivalPlayer.server_onCreate( self )
end

function Player.sv_e_respawn( self )
    if self.sv.spawnparams.respawn then
        if not self.sv.respawnTimeoutTimer then
            self.sv.respawnTimeoutTimer = Timer()
            self.sv.respawnTimeoutTimer:start( 60 * 40 )
        end
        return
    end
    if not self.sv.saved.isConscious then
        self.sv.spawnparams.respawn = true
        sm.event.sendToGame( "sv_e_respawn", { player = self.player } )
    end
end
