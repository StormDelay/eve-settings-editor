# Copies EVE settings files from the live directory into testdata/corpus/.
# This is the ONLY code in the project allowed to touch the live directory,
# and it only ever reads from it (spec section 8).
param(
    [Parameter(Mandatory = $true)][string]$Label,
    [string]$Source = "$env:LOCALAPPDATA\CCP\EVE"
)
$stamp = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHHmmssZ")
$destRoot = Join-Path $PSScriptRoot "..\testdata\corpus\${stamp}_$Label"

$files = Get-ChildItem -Path $Source -Directory |
    ForEach-Object { Get-ChildItem $_.FullName -Directory -Filter "settings_*" -ErrorAction SilentlyContinue } |
    ForEach-Object { Get-ChildItem $_.FullName -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^core_(char|user|public)_.*\.(dat|yaml)$' -or $_.Name -eq 'prefs.ini' } }

if (-not $files) { Write-Error "No settings files found under $Source"; exit 1 }

foreach ($f in $files) {
    # <profile>/<settings folder>/<file>, e.g. c_eve_sharedcache_tq_tranquility/settings_Default/core_char_123.dat
    $settingsDir = $f.Directory
    $profileDir = $settingsDir.Parent
    $dest = Join-Path $destRoot (Join-Path $profileDir.Name $settingsDir.Name)
    New-Item -ItemType Directory -Force $dest | Out-Null
    Copy-Item $f.FullName -Destination $dest
}
$count = ($files | Measure-Object).Count
Write-Output "Copied $count files to $destRoot"
