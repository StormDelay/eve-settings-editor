# Snapshot the live EVE settings directory under a label, then show what moved
# since a previous label. This is the read-out half of an in-game capture: act
# in the client, quit (EVE writes its settings on logout), then run this.
#
#   tools\capture-diff.ps1 -Label baseline
#   tools\capture-diff.ps1 -Label after-session-a -Against baseline
#
# Only the files whose bytes actually changed are decoded. A live settings
# directory is ~600 files and decoded text runs about 20x the binary size, so
# decoding everything would cost a gigabyte per snapshot to show the two or
# three files a capture touched.
#
# Snapshots land in testdata/corpus/, decoded text in testdata/dumps/, both
# gitignored — they are real personal data and must never be committed.
#
# The live directory is only ever read, and only through sync-corpus.ps1, which
# is the sole code in this project allowed to touch it (spec section 8).
#
# Exits 1 when the diff is non-empty. That is git's own convention for
# `diff --no-index`, not a failure.
param(
    [Parameter(Mandatory = $true)][string]$Label,
    [string]$Against,
    # Compare two labels that were already snapshotted, without taking a new
    # one — for re-reading a capture later, or diffing two historical labels.
    [switch]$NoSnapshot
)
$ErrorActionPreference = "Stop"

$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
$corpus = Join-Path $repo "testdata\corpus"
$dumps = Join-Path $repo "testdata\dumps"

# Newest snapshot carrying a label. sync-corpus names directories
# <ISO-stamp>_<label>, so lexical order is chronological order.
function Find-Snapshot([string]$label) {
    if (-not (Test-Path $corpus)) { return $null }
    Get-ChildItem $corpus -Directory -Filter "*_$label" | Select-Object -Last 1
}

# relative path -> FileInfo, so two snapshots can be compared by path.
function Index-Snapshot($snap) {
    $map = @{}
    foreach ($f in Get-ChildItem $snap.FullName -Recurse -File) {
        $map[$f.FullName.Substring($snap.FullName.Length + 1)] = $f
    }
    return $map
}

# Write one file into a side of the comparison: .dat decoded to text, anything
# else (prefs.ini, .yaml) copied as-is so a change there shows up too.
function Write-Side($file, [string]$root, [string]$rel, [string]$bmdump) {
    $dest = Join-Path $root $rel
    New-Item -ItemType Directory -Force (Split-Path $dest) | Out-Null
    if ($file.Extension -ne ".dat") { Copy-Item $file.FullName $dest; return }
    # dump-inline, not dump: the client renumbers its shared-object slots on
    # every write, so a raw dump diffs hundreds of `shared[114]` -> `shared[143]`
    # lines where no value changed. Run `bmdump dump <file>` by hand if you do
    # want to see the sharing layout.
    $text = & $bmdump dump-inline $file.FullName
    # An undecodable file is a real case (the app shows those read-only as hex).
    # Say so rather than writing an empty file that reads as "no data".
    if ($LASTEXITCODE -ne 0) { $text = "<undecodable>" }
    # WriteAllLines is UTF-8 with no BOM; Out-File would prepend one and put a
    # stray marker on the first line of every diff.
    [IO.File]::WriteAllLines("$dest.txt", [string[]]$text)
}

cargo build -q -p blue-marshal --bin bmdump
$bmdump = Join-Path $repo "target\debug\bmdump.exe"
if (-not (Test-Path $bmdump)) { throw "bmdump did not build at $bmdump" }

if (-not $NoSnapshot) { & (Join-Path $PSScriptRoot "sync-corpus.ps1") -Label $Label | Write-Host }
$newSnap = Find-Snapshot $Label
if (-not $newSnap) { throw "no snapshot labelled '$Label' under $corpus" }
if (-not $Against) { Write-Host "snapshot only; pass -Against <label> to diff"; return }

$oldSnap = Find-Snapshot $Against
if (-not $oldSnap) { throw "no snapshot labelled '$Against' under $corpus" }

$old = Index-Snapshot $oldSnap
$new = Index-Snapshot $newSnap
$changed = @($old.Keys + $new.Keys | Sort-Object -Unique | Where-Object {
    $a = $old[$_]; $b = $new[$_]
    if (-not $a -or -not $b) { return $true }   # added or removed
    if ($a.Length -ne $b.Length) { return $true }
    (Get-FileHash $a.FullName -Algorithm MD5).Hash -ne (Get-FileHash $b.FullName -Algorithm MD5).Hash
})

if ($changed.Count -eq 0) {
    Write-Host "no files changed between '$Against' and '$Label'"
    return
}
Write-Host "$($changed.Count) of $($new.Count) files changed; decoding those"

# Derived from the two labels and rebuilt each run, so a rerun never shows a
# stale side.
$out = Join-Path $dumps "$($newSnap.Name)__vs__$($oldSnap.Name)"
if (Test-Path $out) { Remove-Item $out -Recurse -Force }
foreach ($rel in $changed) {
    Write-Host "  $rel"
    if ($old[$rel]) { Write-Side $old[$rel] (Join-Path $out "before") $rel $bmdump }
    if ($new[$rel]) { Write-Side $new[$rel] (Join-Path $out "after") $rel $bmdump }
}

# Diff from inside the comparison directory so the paths in the diff header
# read before/... and after/... rather than two absolute Windows paths.
Push-Location $out
# autocrlf off: these are throwaway derived files, and git otherwise warns about
# line-ending conversion on every non-.dat file it copies through.
try { git -c core.autocrlf=false --no-pager diff --no-index -- before after }
finally { Pop-Location }
