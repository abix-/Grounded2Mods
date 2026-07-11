# Quality-variant generator (docs/status.md "Quality system",
# design in faction-war.md "The quality system").
#
# Reads every vanilla weapon and armor definition from the game's
# story folders, applies per-tier MULTIPLIERS on the base stats,
# and writes the variant XMLs into survivalist-mod/story/Equipment
# (the deploy script copies them into the mod folder; the game
# loads them at story load like any story content).
#
# DRY: nothing is hand-authored. Rerun after a game patch or a
# knob change; generated files (name pattern <Base>_<Tier><N>.xml)
# are deleted and rebuilt every run. Hand-authored files (e.g.
# ColonelsRifle.xml) are untouched.
#
# Each tier ships several statistical SIBLINGS with a small
# deterministic jitter on the power stats, all sharing the display
# name, so two Rare rifles are usually not exactly the same.

[CmdletBinding()]
param(
    [string]$GameDir = 'C:\Games\Steam\steamapps\common\Survivalist Invisible Strain'
)

$ErrorActionPreference = 'Stop'

# ---- THE KNOBS -------------------------------------------------------------
# Stat  = multiplier on the power stats (damage, skill bonuses,
#         accuracy, absorption, insulation)
# Price = multiplier on BasePrice (no jitter: a clean curve)
# Recoil= multiplier on Recoil (quality shoots steadier)
$Tiers = @(
    @{ Name = 'Uncommon';  Stat = 1.10; Price = 2;  Recoil = 0.95 },
    @{ Name = 'Rare';      Stat = 1.20; Price = 4;  Recoil = 0.90 },
    @{ Name = 'Epic';      Stat = 1.35; Price = 8;  Recoil = 0.85 },
    @{ Name = 'Legendary'; Stat = 1.50; Price = 16; Recoil = 0.80 }
)

# Statistical siblings per tier (same display name, jittered
# stats) and the jitter range in percent on the Stat multiplier.
$Siblings  = 3
$JitterPct = 5.0

# What counts as a weapon or armor: the game's own categories.
$Categories = @(
    '2:Weapons/Melee',
    '2:Weapons/Ranged',
    '4:Clothing/Armor/Helmets',
    '4:Clothing/Armor/Vest',
    '4:Clothing/Armor/Legs'
)

# The stats each multiplier touches (element scaled only if the
# base item has it).
$PowerStats  = @('Damage', 'DamageBonusPerSkillLevel', 'AccurateRange', 'AccurateRangeBonusPerSkillLevel', 'DamageAbsorption', 'Insulation')
$IntStats    = @('Insulation')   # engine field is an int; round
$RecoilStats = @('Recoil')
$PriceStats  = @('BasePrice')
# -----------------------------------------------------------------------------

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$outDir = Join-Path $repoRoot 'survivalist-mod\story\Equipment'
$streaming = Join-Path $GameDir 'Survivalist Invisible Strain_Data\StreamingAssets'

if (-not (Test-Path $streaming)) {
    Write-Host "Game StreamingAssets not found: $streaming" -ForegroundColor Yellow
    exit 1
}
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Force -Path $outDir | Out-Null
}

# Deterministic jitter in [-JitterPct, +JitterPct]: an MD5 of
# base|tier|sibling, so a rerun regenerates identical files.
$md5 = [System.Security.Cryptography.MD5]::Create()
function Get-Jitter([string]$key) {
    $bytes = $md5.ComputeHash([System.Text.Encoding]::ASCII.GetBytes($key))
    $n = [System.BitConverter]::ToUInt32($bytes, 0)
    $unit = ($n % 10001) / 10000.0          # 0..1
    return (2.0 * $unit - 1.0) * $JitterPct # -J..+J
}

function Scale-Element([xml]$doc, [string]$name, [double]$mult, [bool]$asInt) {
    $node = $doc.EquipmentPrototype.SelectSingleNode($name)
    if ($null -eq $node) { return }
    $v = [double]::Parse($node.InnerText, [System.Globalization.CultureInfo]::InvariantCulture)
    $scaled = $v * $mult
    if ($asInt) {
        $node.InnerText = ([math]::Round($scaled)).ToString([System.Globalization.CultureInfo]::InvariantCulture)
    } else {
        $node.InnerText = ([math]::Round($scaled, 4)).ToString([System.Globalization.CultureInfo]::InvariantCulture)
    }
}

# Clean prior generated output (by the tier name pattern).
$removed = 0
foreach ($tier in $Tiers) {
    $old = Get-ChildItem -Path $outDir -Filter ("*_" + $tier.Name + "*.xml") -ErrorAction SilentlyContinue
    foreach ($f in $old) { Remove-Item -Force $f.FullName; $removed++ }
}

# Collect base items across every vanilla story folder (first one
# wins on duplicate names).
$bases = [ordered]@{}
foreach ($story in @('BaseStory', 'Common', 'MainStory', 'Sandbox')) {
    $dir = Join-Path $streaming "$story\Equipment"
    if (-not (Test-Path $dir)) { continue }
    foreach ($file in Get-ChildItem -Path $dir -Filter '*.xml') {
        $name = [System.IO.Path]::GetFileNameWithoutExtension($file.Name)
        if ($bases.Contains($name)) { continue }
        [xml]$doc = Get-Content -Raw $file.FullName
        $cat = $doc.EquipmentPrototype.Category
        if ($Categories -contains $cat) {
            $bases[$name] = $file.FullName
        }
    }
}

$writerSettings = New-Object System.Xml.XmlWriterSettings
$writerSettings.Indent = $true
$writerSettings.Encoding = New-Object System.Text.UTF8Encoding($false)

$written = 0
foreach ($base in $bases.Keys) {
    foreach ($tier in $Tiers) {
        for ($k = 1; $k -le $Siblings; $k++) {
            [xml]$doc = Get-Content -Raw $bases[$base]
            $proto = $doc.EquipmentPrototype

            # The display name all siblings share.
            $nameNode = $proto.SelectSingleNode('NativeName')
            if ($null -eq $nameNode) { continue }
            $nameNode.InnerText = $tier.Name + ' ' + $nameNode.InnerText

            # Edge-only: quality never spawns from vanilla loot.
            $loot = $proto.SelectSingleNode('LootableFromLocations')
            if ($null -ne $loot) { [void]$proto.RemoveChild($loot) }

            # The multipliers (power stats jittered per sibling).
            $jitter = Get-Jitter "$base|$($tier.Name)|$k"
            $statMult = $tier.Stat * (1.0 + $jitter / 100.0)
            foreach ($s in $PowerStats)  { Scale-Element $doc $s $statMult ($IntStats -contains $s) }
            foreach ($s in $RecoilStats) { Scale-Element $doc $s $tier.Recoil $false }
            foreach ($s in $PriceStats)  { Scale-Element $doc $s $tier.Price $true }

            $marker = $doc.CreateComment(' GENERATED by scripts/generate_quality.ps1; do not hand-edit ')
            [void]$doc.InsertAfter($marker, $doc.FirstChild)

            $outPath = Join-Path $outDir ($base + '_' + $tier.Name + $k + '.xml')
            $writer = [System.Xml.XmlWriter]::Create($outPath, $writerSettings)
            $doc.Save($writer)
            $writer.Close()
            $written++
        }
    }
}

Write-Host "==> Quality variants: $written written ($removed old removed), $($bases.Count) base items x $($Tiers.Count) tiers x $Siblings siblings" -ForegroundColor Green
Write-Host "    Output: $outDir" -ForegroundColor Green
