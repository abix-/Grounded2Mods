<#
.SYNOPSIS
Rebuild, redeploy, and restart The Boss Gangsters Nightlife with
the current mod.

.DESCRIPTION
The one path for getting the working tree into the live game.
Steps, in order:
  1. Build bossgangsters-mod (Rust, via k3sc cargo-lock).
  2. Build the Mono shim (dotnet, against the game's refs).
  3. Close the game if it is running and wait for it to exit.
  4. Deploy: shim dll + fresh base mod dll into
     BepInEx/plugins/bossgangsters-mod/, and delete stale
     hot-reload generation dlls (a restart resets to gen 0).
  5. Launch the game through Steam.
  6. Wait until the mod's control plane answers on port 17176.

.PARAMETER SkipBuild
Skip steps 1-2 and deploy whatever is already built.

.EXAMPLE
pwsh -NoProfile -File bossgangsters-mod/scripts/restart.ps1

.NOTES
Hot reload (Rust-only changes) does NOT need this script: run
build_and_deploy.ps1 -Hot instead. This script is for shim (C#)
changes or a wedged game.
#>
[CmdletBinding()]
param (
    [switch]$SkipBuild
)

$Repo = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$GameDir = "C:\Games\Steam\steamapps\common\The Boss Gangsters Nightlife"
$PluginDir = Join-Path $GameDir "BepInEx\plugins\bossgangsters-mod"
$AppId = "2774040"
$Port = 17176

if (-not $SkipBuild) {
    Write-Host "[build] bossgangsters-mod (Rust)" -ForegroundColor Cyan
    Push-Location $Repo
    k3sc cargo-lock build --release -p bossgangsters-mod
    if ($LASTEXITCODE -ne 0) { Pop-Location; throw "cargo build failed" }
    Pop-Location

    Write-Host "[build] Mono shim (C#)" -ForegroundColor Cyan
    Push-Location $Repo
    dotnet build -c Release `
        -p:BepInExDir="$GameDir\BepInEx" `
        -p:UnityDir="$GameDir\TheBossGangsters_Data\Managed" `
        (Join-Path $Repo "unityforge\cs-shim-mono\Unityforge.Shim.Mono.csproj")
    if ($LASTEXITCODE -ne 0) { Pop-Location; throw "shim build failed" }
    Pop-Location
}

$game = Get-Process "TheBossGangsters" -ErrorAction SilentlyContinue
if ($game) {
    Write-Host "[stop] closing TheBossGangsters (pid $($game.Id))" -ForegroundColor Cyan
    $game.CloseMainWindow() | Out-Null
    if (-not $game.WaitForExit(20000)) {
        Write-Warning "no clean exit after 20s; killing"
        Stop-Process -Id $game.Id -Force
        $game.WaitForExit()
    }
    Start-Sleep -Seconds 2  # let file locks drop
}

Write-Host "[deploy] shim + mod into BepInEx/plugins/bossgangsters-mod/" -ForegroundColor Cyan
if (-not (Test-Path $PluginDir)) {
    New-Item -ItemType Directory -Force -Path $PluginDir | Out-Null
}
Copy-Item (Join-Path $Repo "unityforge\cs-shim-mono\bin\Release\netstandard2.1\Unityforge.Shim.Mono.dll") `
    (Join-Path $PluginDir "Unityforge.Shim.Mono.dll") -Force
Copy-Item (Join-Path $Repo "target\x86_64-pc-windows-msvc\release\bossgangsters_mod.dll") `
    (Join-Path $PluginDir "bossgangsters_mod.unityforge.dll") -Force
Get-ChildItem $PluginDir -Filter "bossgangsters_mod.unityforge.gen*.dll" -ErrorAction SilentlyContinue | Remove-Item -Force

Write-Host "[launch] steam://rungameid/$AppId" -ForegroundColor Cyan
Start-Process "steam://rungameid/$AppId"

Write-Host "[wait] control plane on port $Port (up to 180s)" -ForegroundColor Cyan
$deadline = (Get-Date).AddSeconds(180)
while ((Get-Date) -lt $deadline) {
    try {
        Invoke-RestMethod -Uri "http://127.0.0.1:$Port/op" -Method Post `
            -Body '{"op":"ping","args":{}}' -ContentType "application/json" -TimeoutSec 2 | Out-Null
        Write-Host "[ready] control plane answering" -ForegroundColor Green
        exit 0
    }
    catch {
        Start-Sleep -Seconds 3
    }
}
Write-Warning "control plane not answering after 180s; check BepInEx\LogOutput.log"
exit 1
