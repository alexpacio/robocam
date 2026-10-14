[CmdletBinding()]
param([string]$Tag, [string]$NotesPath, [switch]$RequireMain)
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
Push-Location $repository
try {
    $manifest = Get-Content Cargo.toml -Raw
    if ($manifest -notmatch '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"') { throw 'Invalid package version' }
    $version = $Matches[1]
    if ($Tag -and $Tag -cne "v$version") { throw "Tag $Tag does not match package version $version" }
    [xml]$windowsManifest = Get-Content assets/app.manifest -Raw
    if ($windowsManifest.assembly.assemblyIdentity.version -ne "$version.0") { throw 'Windows manifest version mismatch' }
    if ((Get-Content installer/robocam.iss -Raw) -notmatch ('#define AppVersion "' + [regex]::Escape($version) + '"')) { throw 'Installer default version mismatch' }
    $metadata = & cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $metadata.packages[0].version -ne $version) { throw 'Cargo.lock/package metadata mismatch' }
    $changelog = Get-Content CHANGELOG.md -Raw
    $entry = [regex]::Match($changelog, ('(?ms)^## ' + [regex]::Escape($version) + ' - \d{4}-\d{2}-\d{2}\r?\n(.*?)(?=^## |\z)'))
    if (!$entry.Success -or !$entry.Groups[1].Value.Trim()) { throw 'Missing dated changelog entry' }
    if ($RequireMain) {
        & git fetch origin main
        if ($LASTEXITCODE -ne 0) { throw 'Cannot fetch main' }
        & git merge-base --is-ancestor HEAD origin/main
        if ($LASTEXITCODE -ne 0) { throw 'Release commit does not belong to main' }
    }
    if ($NotesPath) { [IO.File]::WriteAllText($NotesPath, $entry.Value.Trim() + [Environment]::NewLine) }
    Write-Host "Validated robocam $version"
} finally { Pop-Location }
