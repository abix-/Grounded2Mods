# wwm-mod build + deploy. Run from repo root or anywhere.
#
# Builds:
#   1. wwm_mod.dll (Rust cdylib, release)
#   2. Unityforge.Shim.dll (C# BepInEx plugin, release)
#
# Deploys both into <WWM>/BepInEx/plugins/wwm-mod/.
#
# Prerequisites:
#   - Cargo + rustc (workspace toolchain via rust-toolchain.toml).
#   - .NET SDK with netstandard2.1 support.
#   - BepInEx 5.x already installed in WWM. If not, see the
#     README for the one-time bootstrap.

[CmdletBinding()]
param(
    [string]$WwmDir = 'C:\Games\Steam\steamapps\common\Wild West Miner Simulator',
    [string]$ShimBepInExDir = '',
    [switch]$NoCopy,
    [switch]$Hot
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $repoRoot

$dataDir = Get-ChildItem -Path $WwmDir -Directory -Filter '*_Data' | Select-Object -First 1
if (-not $dataDir) {
    Write-Host "No *_Data directory found in $WwmDir" -ForegroundColor Red
    exit 1
}
$wwmManaged = Join-Path $dataDir.FullName 'Managed'
$wwmBep     = Join-Path $WwmDir 'BepInEx'
$pluginDir  = Join-Path $wwmBep 'plugins\wwm-mod'

if (-not $ShimBepInExDir) {
    $ShimBepInExDir = $wwmBep
}

if (-not (Test-Path (Join-Path $ShimBepInExDir 'core\BepInEx.dll'))) {
    Write-Host "ShimBepInExDir does not contain core\BepInEx.dll: $ShimBepInExDir" -ForegroundColor Yellow
    Write-Host "Pass -ShimBepInExDir <path> to a BepInEx 5.x install to build the shim." -ForegroundColor Yellow
    exit 1
}

Write-Host "==> Build wwm_mod.dll (Rust release)" -ForegroundColor Cyan
& cargo build --release -p wwm-mod
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Build Unityforge.Shim.Mono.dll (C# release; WWM is Mono)" -ForegroundColor Cyan
& dotnet build -c Release `
    -p:BepInExDir="$ShimBepInExDir" `
    -p:UnityDir="$wwmManaged" `
    (Join-Path $repoRoot 'unityforge\cs-shim-mono\Unityforge.Shim.Mono.csproj')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if ($NoCopy) {
    Write-Host "==> Done (skipped deploy; -NoCopy set)" -ForegroundColor Green
    exit 0
}

if (-not (Test-Path $pluginDir)) {
    New-Item -ItemType Directory -Force -Path $pluginDir | Out-Null
}

$rustDll = Join-Path $repoRoot 'target\x86_64-pc-windows-msvc\release\wwm_mod.dll'
$shimDll = Join-Path $repoRoot 'unityforge\cs-shim-mono\bin\Release\netstandard2.1\Unityforge.Shim.Mono.dll'

if ($Hot) {
    # Generation-versioned hot reload. Find the highest
    # existing `<dll>.gen<N>.dll` in the plugin dir and stage
    # this build as N+1. The running shim's per-second watcher
    # picks it up, shuts down the active generation, loads the
    # new one, and switches active. Old generation's image
    # stays mapped (never FreeLibrary); old threads exit on
    # their stop signals and the OS unmaps the image once
    # nothing references it.
    #
    # See docs/unityforge-plan.md section 6.5 for the design.
    if (-not (Test-Path $pluginDir)) {
        New-Item -ItemType Directory -Force -Path $pluginDir | Out-Null
    }
    $maxGen = 0
    $existing = Get-ChildItem -Path $pluginDir -Filter 'wwm_mod.unityforge.gen*.dll' -ErrorAction SilentlyContinue
    foreach ($f in $existing) {
        if ($f.Name -match 'gen(\d+)\.dll$') {
            $n = [int]$Matches[1]
            if ($n -gt $maxGen) { $maxGen = $n }
        }
    }
    $newGen = $maxGen + 1
    $stagingDll = Join-Path $pluginDir "wwm_mod.unityforge.gen$newGen.dll"
    Copy-Item -Force $rustDll $stagingDll
    Write-Host "==> Staged Rust DLL as generation $newGen" -ForegroundColor Green
    Write-Host "      $stagingDll" -ForegroundColor Green
    Write-Host "    The running shim will pick it up within ~1s." -ForegroundColor Green
    Write-Host "    Tail BepInEx/LogOutput.log for 'hot reload generation' line." -ForegroundColor Green
    exit 0
}

Copy-Item -Force $rustDll (Join-Path $pluginDir 'wwm_mod.unityforge.dll')
Copy-Item -Force $shimDll (Join-Path $pluginDir 'Unityforge.Shim.Mono.dll')

Write-Host "==> Deployed:" -ForegroundColor Green
Get-ChildItem $pluginDir | Select-Object Name, Length | Format-Table -AutoSize

Write-Host "Launch the game; BepInEx log lands at:" -ForegroundColor Green
Write-Host "  $wwmBep\LogOutput.log" -ForegroundColor Green
Write-Host "Once running, curl http://localhost:17172/op -d '{`"op`":`"ping`"}'" -ForegroundColor Green
