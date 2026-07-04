# survivalist-mod build + deploy. Run from repo root or anywhere.
#
# Builds:
#   1. survivalist_mod.dll (Rust cdylib, release)
#   2. Unityforge.Shim.Survivalist.dll (C#, release)
#
# Deploys into the mod folder the operator created ONCE in the
# game's in-game editor ("Is Mod" ticked, Harmony 2.0.4 workshop
# dependency added in Story Settings):
#   <game>/<ModName>/DLLs/Unityforge.Shim.Survivalist.dll
#   <game>/<ModName>/survivalist_mod.unityforge.dll
#
# The Rust cdylib sits in the MOD ROOT, not DLLs/: the game's
# Story.LoadDLLs Assembly.LoadFroms every *.dll in DLLs/ and a
# native image there would log a BadImageFormat error on every
# story load. The shim locates *.unityforge.dll one level above
# its own directory.
#
# Prerequisites:
#   - Cargo + rustc (workspace toolchain via rust-toolchain.toml).
#   - .NET SDK (shim targets net472).
#   - The mod folder created in-game (see above).
#   - Steam Workshop "Harmony 2.0.4" (id 2366696532) subscribed
#     and declared as the mod's dependency.

[CmdletBinding()]
param(
    [string]$GameDir = 'C:\Games\Steam\steamapps\common\Survivalist Invisible Strain',
    [string]$ModName = 'SurvivalistMod',
    [switch]$NoCopy,
    [switch]$Hot
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $repoRoot

# Created stories/mods land under StreamingAssets, not the game
# root (verified from Player.log "Saved .../StreamingAssets/
# SurvivalistTweaks/Settings.xml" lines, 2026-07-04; the setup
# guide's "subfolder in the game's directory" is imprecise).
$managed = Join-Path $GameDir 'Survivalist Invisible Strain_Data\Managed'
$modDir  = Join-Path $GameDir "Survivalist Invisible Strain_Data\StreamingAssets\$ModName"
$dllsDir = Join-Path $modDir 'DLLs'

if (-not (Test-Path $managed)) {
    Write-Host "Game Managed dir not found: $managed" -ForegroundColor Yellow
    Write-Host "Pass -GameDir <path> to the Survivalist install." -ForegroundColor Yellow
    exit 1
}

Write-Host "==> Build survivalist_mod.dll (Rust release)" -ForegroundColor Cyan
& cargo build --release -p survivalist-mod
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Build Unityforge.Shim.Survivalist.dll (C# release)" -ForegroundColor Cyan
& dotnet build -c Release `
    -p:UnityDir="$managed" `
    (Join-Path $repoRoot 'unityforge\cs-shim-survivalist\Unityforge.Shim.Survivalist.csproj')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if ($NoCopy) {
    Write-Host "==> Done (skipped deploy; -NoCopy set)" -ForegroundColor Green
    exit 0
}

if (-not (Test-Path $modDir)) {
    Write-Host "Mod folder not found: $modDir" -ForegroundColor Yellow
    Write-Host "Create the mod ONCE in the game's editor (name it '$ModName'," -ForegroundColor Yellow
    Write-Host "tick 'Is Mod', add the Harmony 2.0.4 workshop dependency in" -ForegroundColor Yellow
    Write-Host "Story Settings), or pass -ModName to match an existing folder." -ForegroundColor Yellow
    exit 1
}

$rustDll = Join-Path $repoRoot 'target\x86_64-pc-windows-msvc\release\survivalist_mod.dll'
$shimDll = Join-Path $repoRoot 'unityforge\cs-shim-survivalist\bin\Release\net472\Unityforge.Shim.Survivalist.dll'

if ($Hot) {
    # Generation-versioned hot reload. Find the highest existing
    # `survivalist_mod.unityforge.gen<N>.dll` in the MOD ROOT and
    # stage this build as N+1. The running shim's per-second
    # watcher picks it up, shuts down the active generation, loads
    # the new one, and switches active.
    #
    # See docs/unityforge-plan.md section 6.5 for the design.
    $maxGen = 0
    $existing = Get-ChildItem -Path $modDir -Filter 'survivalist_mod.unityforge.gen*.dll' -ErrorAction SilentlyContinue
    foreach ($f in $existing) {
        if ($f.Name -match 'gen(\d+)\.dll$') {
            $n = [int]$Matches[1]
            if ($n -gt $maxGen) { $maxGen = $n }
        }
    }
    $newGen = $maxGen + 1
    $stagingDll = Join-Path $modDir "survivalist_mod.unityforge.gen$newGen.dll"
    Copy-Item -Force $rustDll $stagingDll
    Write-Host "==> Staged Rust DLL as generation $newGen" -ForegroundColor Green
    Write-Host "      $stagingDll" -ForegroundColor Green
    Write-Host "    The running shim will pick it up within ~1s." -ForegroundColor Green
    Write-Host "    Tail the player log for the 'hot reload generation' line." -ForegroundColor Green
    exit 0
}

if (-not (Test-Path $dllsDir)) {
    New-Item -ItemType Directory -Force -Path $dllsDir | Out-Null
}

Copy-Item -Force $rustDll (Join-Path $modDir 'survivalist_mod.unityforge.dll')
Copy-Item -Force $shimDll (Join-Path $dllsDir 'Unityforge.Shim.Survivalist.dll')

Write-Host "==> Deployed:" -ForegroundColor Green
Write-Host "  $modDir\survivalist_mod.unityforge.dll"
Write-Host "  $dllsDir\Unityforge.Shim.Survivalist.dll"

Write-Host "Launch the game and load a story with the mod active." -ForegroundColor Green
Write-Host "Shim + loader lines land in the Unity player log:" -ForegroundColor Green
Write-Host "  $env:USERPROFILE\AppData\LocalLow\Ginormocorp Industries\Survivalist Invisible Strain\Player.log" -ForegroundColor Green
Write-Host "Once in-game, curl http://localhost:17173/op -d '{`"op`":`"ping`"}'" -ForegroundColor Green
