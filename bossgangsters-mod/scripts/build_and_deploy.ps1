# bossgangsters-mod build + deploy. Run from repo root or anywhere.
#
# Builds:
#   1. bossgangsters_mod.dll (Rust cdylib, release)
#   2. Unityforge.Shim.Mono.dll (C# BepInEx plugin, release)
#
# Deploys both into <game>/BepInEx/plugins/bossgangsters-mod/.
#
# Prerequisites:
#   - Cargo + rustc (workspace toolchain via rust-toolchain.toml).
#   - .NET SDK with netstandard2.1 support.
#   - BepInEx 5.x already installed in the game folder. If not,
#     copy winhttp.dll, doorstop_config.ini, and BepInEx\core
#     from a BepInEx 5.4.x x64 Mono package.

[CmdletBinding()]
param(
    [string]$GameDir = 'C:\Games\Steam\steamapps\common\The Boss Gangsters Nightlife',
    [string]$ShimBepInExDir = '',
    [switch]$NoCopy,
    [switch]$Hot
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $repoRoot

$dataDir = Get-ChildItem -Path $GameDir -Directory -Filter '*_Data' | Select-Object -First 1
if (-not $dataDir) {
    Write-Host "No *_Data directory found in $GameDir" -ForegroundColor Red
    exit 1
}
$gameManaged = Join-Path $dataDir.FullName 'Managed'
$gameBep     = Join-Path $GameDir 'BepInEx'
$pluginDir   = Join-Path $gameBep 'plugins\bossgangsters-mod'

if (-not $ShimBepInExDir) {
    $ShimBepInExDir = $gameBep
}

if (-not (Test-Path (Join-Path $ShimBepInExDir 'core\BepInEx.dll'))) {
    Write-Host "ShimBepInExDir does not contain core\BepInEx.dll: $ShimBepInExDir" -ForegroundColor Yellow
    Write-Host "Pass -ShimBepInExDir <path> to a BepInEx 5.x install to build the shim." -ForegroundColor Yellow
    exit 1
}

Write-Host "==> Build bossgangsters_mod.dll (Rust release)" -ForegroundColor Cyan
& cargo build --release -p bossgangsters-mod
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Build Unityforge.Shim.Mono.dll (C# release; the game is Mono)" -ForegroundColor Cyan
& dotnet build -c Release `
    -p:BepInExDir="$ShimBepInExDir" `
    -p:UnityDir="$gameManaged" `
    (Join-Path $repoRoot 'unityforge\cs-shim-mono\Unityforge.Shim.Mono.csproj')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if ($NoCopy) {
    Write-Host "==> Done (skipped deploy; -NoCopy set)" -ForegroundColor Green
    exit 0
}

if (-not (Test-Path $pluginDir)) {
    New-Item -ItemType Directory -Force -Path $pluginDir | Out-Null
}

$rustDll = Join-Path $repoRoot 'target\x86_64-pc-windows-msvc\release\bossgangsters_mod.dll'
$shimDll = Join-Path $repoRoot 'unityforge\cs-shim-mono\bin\Release\netstandard2.1\Unityforge.Shim.Mono.dll'

if ($Hot) {
    # Generation-versioned hot reload. Find the highest
    # existing `<dll>.gen<N>.dll` in the plugin dir and stage
    # this build as N+1. The running shim's per-second watcher
    # picks it up. See docs/unityforge-plan.md section 6.5.
    $maxGen = 0
    $existing = Get-ChildItem -Path $pluginDir -Filter 'bossgangsters_mod.unityforge.gen*.dll' -ErrorAction SilentlyContinue
    foreach ($f in $existing) {
        if ($f.Name -match 'gen(\d+)\.dll$') {
            $n = [int]$Matches[1]
            if ($n -gt $maxGen) { $maxGen = $n }
        }
    }
    $newGen = $maxGen + 1
    $stagingDll = Join-Path $pluginDir "bossgangsters_mod.unityforge.gen$newGen.dll"
    Copy-Item -Force $rustDll $stagingDll
    Write-Host "==> Staged Rust DLL as generation $newGen" -ForegroundColor Green
    Write-Host "      $stagingDll" -ForegroundColor Green
    Write-Host "    The running shim will pick it up within ~1s." -ForegroundColor Green
    Write-Host "    Tail BepInEx/LogOutput.log for 'hot reload generation' line." -ForegroundColor Green
    exit 0
}

Copy-Item -Force $rustDll (Join-Path $pluginDir 'bossgangsters_mod.unityforge.dll')
Copy-Item -Force $shimDll (Join-Path $pluginDir 'Unityforge.Shim.Mono.dll')

Write-Host "==> Deployed:" -ForegroundColor Green
Get-ChildItem $pluginDir | Select-Object Name, Length | Format-Table -AutoSize

Write-Host "Launch the game; BepInEx log lands at:" -ForegroundColor Green
Write-Host "  $gameBep\LogOutput.log" -ForegroundColor Green
Write-Host "Once running, curl http://localhost:17176/op -d '{`"op`":`"ping`"}'" -ForegroundColor Green
