param(
    [Parameter(Mandatory=$true)]
    [string]$GameDir
)

$ErrorActionPreference = 'Stop'
$modName = 'QuasimorphMod'
$projDir = Split-Path $PSScriptRoot -Parent
$outDir = Join-Path $projDir 'bin'
$modDir = Join-Path $GameDir "Mods\$modName"

Write-Host "building $modName..."
dotnet build "$projDir\QuasimorphMod.csproj" -c Release -o $outDir "-p:GameDir=$GameDir"
if ($LASTEXITCODE -ne 0) { throw 'build failed' }

if (-not (Test-Path $modDir)) {
    New-Item -ItemType Directory -Path $modDir | Out-Null
}

Copy-Item (Join-Path $outDir "$modName.dll") $modDir -Force

$manifest = Join-Path $modDir 'modmanifest.json'
if (-not (Test-Path $manifest)) {
    Write-Host "no modmanifest.json yet. generate one in-game with:"
    Write-Host "  mod_createmanifest $modName `"$modDir`""
}

Write-Host "deployed to $modDir"
