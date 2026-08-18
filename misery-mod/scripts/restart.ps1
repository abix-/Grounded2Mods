<#
.SYNOPSIS
Rebuild, redeploy, and restart MISERY with the current mod.

.DESCRIPTION
The one path for getting the working tree into the live game.
Steps, in order:
  1. Build misery-mod (Rust release, via k3sc cargo-lock).
  2. Close the game if it is running and wait for it to exit.
  3. Deploy: copy the built DLL to the UE4SS mod directory,
     removing any stale main-new.dll. Validate file size and
     timestamp match the build output.
  4. Launch the game through Steam.
  5. Wait until the mod's control plane answers on port 17176.

.PARAMETER SkipBuild
Skip step 1 and deploy whatever is already built.

.EXAMPLE
pwsh -NoProfile -File misery-mod/scripts/restart.ps1

.EXAMPLE
pwsh -NoProfile -File misery-mod/scripts/restart.ps1 -SkipBuild
#>
[CmdletBinding()]
param (
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$Repo = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$GameDir = "C:\Games\Steam\steamapps\common\MISERY"
$ModDir = Join-Path $GameDir "MISERY\Binaries\Win64\ue4ss\Mods\MiseryMod\dlls"
$BuildDll = Join-Path $Repo "target\x86_64-pc-windows-msvc\release\main.dll"
$TargetDll = Join-Path $ModDir "main.dll"
$StaleDll = Join-Path $ModDir "main-new.dll"
$AppId = "2119830"
$Port = 17176
$ProcessNames = @("MISERY-Win64-Shipping", "MISERY")

# --- step 1: build ---
if (-not $SkipBuild) {
    Write-Host "[build] misery-mod (Rust release)" -ForegroundColor Cyan
    Push-Location $Repo
    try {
        k3sc cargo-lock build --release -p misery-mod
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }
    }
    finally { Pop-Location }
}

if (-not (Test-Path $BuildDll)) {
    throw "build output not found: $BuildDll"
}
$buildInfo = Get-Item $BuildDll
Write-Host "[build] output: $($buildInfo.Length) bytes, $($buildInfo.LastWriteTime)" -ForegroundColor Gray

# --- step 2: stop game ---
foreach ($pname in $ProcessNames) {
    $procs = Get-Process $pname -ErrorAction SilentlyContinue
    foreach ($proc in $procs) {
        Write-Host "[stop] closing $pname (pid $($proc.Id))" -ForegroundColor Cyan
        try { $proc.CloseMainWindow() | Out-Null } catch {}
        if (-not $proc.WaitForExit(15000)) {
            Write-Warning "no clean exit after 15s; killing pid $($proc.Id)"
            Stop-Process -Id $proc.Id -Force -Confirm:$false
            $proc.WaitForExit()
        }
    }
}
Start-Sleep -Seconds 2

# --- step 3: deploy + validate ---
Write-Host "[deploy] copying to $TargetDll" -ForegroundColor Cyan

if (Test-Path $StaleDll) {
    Remove-Item $StaleDll -Force -Confirm:$false
    Write-Host "[deploy] removed stale main-new.dll" -ForegroundColor Gray
}

Copy-Item $BuildDll $TargetDll -Force

$deployed = Get-Item $TargetDll
if ($deployed.Length -ne $buildInfo.Length) {
    throw "[deploy] VALIDATION FAILED: size mismatch (build=$($buildInfo.Length), deployed=$($deployed.Length))"
}
if ($deployed.LastWriteTime -lt $buildInfo.LastWriteTime.AddSeconds(-5)) {
    throw "[deploy] VALIDATION FAILED: deployed file is older than build output"
}
Write-Host "[deploy] validated: $($deployed.Length) bytes, $($deployed.LastWriteTime)" -ForegroundColor Green

# --- step 4: launch ---
Write-Host "[launch] steam://rungameid/$AppId" -ForegroundColor Cyan
Start-Process "steam://rungameid/$AppId"

# --- step 5: wait for control plane ---
Write-Host "[wait] control plane on port $Port (up to 180s)" -ForegroundColor Cyan
$deadline = (Get-Date).AddSeconds(180)
$cpReady = $false
while ((Get-Date) -lt $deadline) {
    try {
        $r = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/debug" -Method Post `
            -Body '{"op":"list_ops","args":{}}' -ContentType "application/json" -TimeoutSec 2
        if ($r.ok -eq $true) {
            Write-Host "[ready] control plane answering on port $Port" -ForegroundColor Green
            $cpReady = $true
            break
        }
    }
    catch {
        Start-Sleep -Seconds 3
    }
}
if (-not $cpReady) {
    Write-Warning "control plane not answering after 180s; check UE4SS console"
    exit 1
}

Write-Host "[done] MISERY running with mod loaded" -ForegroundColor Green
exit 0
