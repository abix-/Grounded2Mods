<#
.SYNOPSIS
Rebuild and hot-reload the mod into the RUNNING game.

.DESCRIPTION
The fast path: no game restart, no reloading a save. Steps:
  1. Build misery-mod (Rust release, via k3sc cargo-lock).
  2. Copy the built DLL to the mod directory as main-new.dll.
  3. The running mod's watcher notices the file, synthesizes
     Ctrl+R, and UE4SS reloads: shutdown runs (stopping this
     mod's pollers at order 50, then hooks at 100), the file is
     renamed over main.dll, and the new image loads.
  4. Wait until the control plane answers again.

Requires the game to be RUNNING and FOCUSED: the synthesized
Ctrl+R goes to the foreground window, so if focus is on another
window that window receives it and no reload happens.

Safe only because every background worker in this mod is a
stoppable poller (modforge::rpg::poller::spawn_interval). A raw
thread would keep running in freed code after the unload and
crash the game, which is why hot reload was previously banned.

.PARAMETER SkipBuild
Skip step 1 and deploy whatever is already built.

.PARAMETER TimeoutSeconds
How long to wait for the control plane to come back. Default 90.

.EXAMPLE
pwsh -NoProfile -File misery-mod/scripts/reload.ps1
#>
[CmdletBinding()]
param (
    [switch]$SkipBuild,
    [int]$TimeoutSeconds = 90
)

$ErrorActionPreference = "Stop"

$Repo = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$GameDir = "C:\Games\Steam\steamapps\common\MISERY"
$ModDir = Join-Path $GameDir "MISERY\Binaries\Win64\ue4ss\Mods\MiseryMod\dlls"
$BuildDll = Join-Path $Repo "target\x86_64-pc-windows-msvc\release\main.dll"
$LiveDll = Join-Path $ModDir "main.dll"
$NewDll = Join-Path $ModDir "main-new.dll"
$Port = 17176
$ProcessNames = @("MISERY-Win64-Shipping", "MISERY")

function Test-ControlPlane {
    try {
        $body = '{"op":"ping","args":{}}'
        $r = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/debug" -Method Post `
            -Body $body -ContentType "application/json" -TimeoutSec 3
        return $null -ne $r
    } catch {
        return $false
    }
}

# --- the game must already be running ---
$running = $ProcessNames | ForEach-Object { Get-Process -Name $_ -ErrorAction SilentlyContinue }
if (-not $running) {
    Write-Host "[reload] game is not running; use restart.ps1 instead" -ForegroundColor Yellow
    exit 1
}
if (-not (Test-ControlPlane)) {
    Write-Host "[reload] control plane is not answering; is a save loaded?" -ForegroundColor Yellow
    exit 1
}

# --- step 1: build ---
if (-not $SkipBuild) {
    Write-Host "[build] misery-mod (Rust release)" -ForegroundColor Cyan
    Push-Location $Repo
    try {
        & k3sc cargo-lock build --release -p misery-mod
        if ($LASTEXITCODE -ne 0) { throw "build failed" }
    } finally {
        Pop-Location
    }
}
if (-not (Test-Path $BuildDll)) { throw "no build output at $BuildDll" }
$built = Get-Item $BuildDll
Write-Host "[build] output: $($built.Length) bytes, $($built.LastWriteTime)"

# The live DLL's identity before the swap, so we can prove it
# actually changed rather than assuming the reload happened.
$beforeLen = (Get-Item $LiveDll).Length
$beforeTime = (Get-Item $LiveDll).LastWriteTime

# --- step 2: stage as main-new.dll ---
Copy-Item -Path $BuildDll -Destination $NewDll -Force
Write-Host "[stage] wrote $NewDll" -ForegroundColor Cyan
Write-Host "[focus] click the game window now; Ctrl+R goes to the foreground window" -ForegroundColor Yellow

# --- step 3: wait for the swap ---
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
$swapped = $false
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 2
    if (-not (Test-Path $NewDll)) {
        # The staged file is gone: shutdown renamed it into place.
        $swapped = $true
        break
    }
}
if (-not $swapped) {
    Write-Host "[reload] main-new.dll was never consumed (was the game focused?)" -ForegroundColor Red
    exit 1
}

$afterLen = (Get-Item $LiveDll).Length
$afterTime = (Get-Item $LiveDll).LastWriteTime
if ($afterLen -eq $beforeLen -and $afterTime -eq $beforeTime) {
    Write-Host "[reload] main.dll did not change; reload did not take" -ForegroundColor Red
    exit 1
}
Write-Host "[swap] main.dll now $afterLen bytes, $afterTime" -ForegroundColor Green

# --- step 4: control plane back up ---
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
while ((Get-Date) -lt $deadline) {
    if (Test-ControlPlane) {
        Write-Host "[ready] control plane answering on port $Port" -ForegroundColor Green
        $still = $ProcessNames | ForEach-Object { Get-Process -Name $_ -ErrorAction SilentlyContinue }
        if (-not $still) {
            Write-Host "[reload] the game died during reload" -ForegroundColor Red
            exit 1
        }
        Write-Host "[done] mod reloaded, game still running, save still loaded" -ForegroundColor Green
        exit 0
    }
    Start-Sleep -Seconds 2
}
Write-Host "[reload] control plane never came back; the game may have crashed" -ForegroundColor Red
exit 1
