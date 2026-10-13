[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [string]$InnoCompiler,
    [string]$VCRuntimeDirectory
)
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'build-env.ps1')
if (!$SkipBuild) { & (Join-Path $PSScriptRoot 'build.ps1') -Configuration Release }
$manifest = Get-Content -LiteralPath (Join-Path $repository 'Cargo.toml') -Raw
if ($manifest -notmatch '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"') { throw 'Cannot read Cargo package version.' }
$version = $Matches[1]
$releaseExecutable = Join-Path $repository 'target\release\robocam.exe'
if (!(Test-Path -LiteralPath $releaseExecutable)) { throw "Release executable missing: $releaseExecutable" }
$distribution = Join-Path $repository 'dist'
$stage = Join-Path $distribution "robocam-$version-windows-x64"
$sourceStage = Join-Path $distribution "robocam-$version-source"
New-Item -ItemType Directory -Force -Path $distribution | Out-Null

function Reset-StagingDirectory([string]$Directory) {
    # Never recursively remove a path until its absolute location is checked.
    $resolved = [IO.Path]::GetFullPath($Directory)
    $allowed = [IO.Path]::GetFullPath($distribution).TrimEnd('\') + '\'
    if (!$resolved.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) { throw "Unsafe staging directory: $resolved" }
    foreach ($path in @($distribution, $resolved)) {
        if ((Test-Path -LiteralPath $path) -and ((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Staging paths must not be junctions or symbolic links: $path"
        }
    }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
    New-Item -ItemType Directory -Path $resolved -Force | Out-Null
}

function New-PackageZip([string]$Directory, [string]$Destination) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    for ($attempt = 1; $attempt -le 5; $attempt++) {
        try {
            if (Test-Path -LiteralPath $Destination) { Remove-Item -LiteralPath $Destination -Force -ErrorAction Stop }
            [IO.Compression.ZipFile]::CreateFromDirectory($Directory, $Destination, [IO.Compression.CompressionLevel]::Optimal, $true)
            return
        } catch {
            if ($attempt -eq 5) { throw }
            # Allow short-lived antivirus/indexer handles on newly written archives to close.
            Start-Sleep -Milliseconds (400 * $attempt)
        }
    }
}
Reset-StagingDirectory $stage
Reset-StagingDirectory $sourceStage
Copy-Item -LiteralPath $releaseExecutable -Destination $stage
$nativeDirectory = Join-Path $env:ROBOCAM_OPENCV_ROOT 'build\x64\vc16\bin'
foreach ($file in @('opencv_world4100.dll', 'opencv_videoio_msmf4100_64.dll')) {
    Copy-Item -LiteralPath (Join-Path $nativeDirectory $file) -Destination $stage
}

if (!$VCRuntimeDirectory) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path -LiteralPath $vswhere) {
        $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($vs) {
            $redistRoot = Join-Path $vs 'VC\Redist\MSVC'
            $versionDirectory = Get-ChildItem -LiteralPath $redistRoot -Directory |
                Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
                Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
            if ($versionDirectory) {
                $crt = Get-ChildItem -Path (Join-Path $versionDirectory.FullName 'x64\Microsoft.VC*.CRT') -Directory | Select-Object -First 1
                if ($crt) { $VCRuntimeDirectory = $crt.FullName }
            }
        }
    }
}
if (!$VCRuntimeDirectory -or !(Test-Path (Join-Path $VCRuntimeDirectory 'vcruntime140.dll')) -or !(Test-Path (Join-Path $VCRuntimeDirectory 'msvcp140.dll'))) {
    throw 'MSVC x64 redistributable CRT missing. Supply -VCRuntimeDirectory pointing to VC\Redist\MSVC\<version>\x64\Microsoft.VC*.CRT.'
}
Get-ChildItem -LiteralPath $VCRuntimeDirectory -Filter '*.dll' -File | Copy-Item -Destination $stage

# Preserve all notices distributed with the OpenCV binary, even for unused modules.
$licenses = Join-Path $stage 'licenses'
$opencvLicenses = Join-Path $licenses 'opencv'
New-Item -ItemType Directory -Force -Path $opencvLicenses | Out-Null
Copy-Item -LiteralPath (Join-Path $env:ROBOCAM_OPENCV_ROOT 'build\LICENSE') -Destination (Join-Path $opencvLicenses 'LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $env:ROBOCAM_OPENCV_ROOT 'build\etc\licenses') -Destination (Join-Path $opencvLicenses 'third-party') -Recurse
foreach ($file in @('LICENSE', 'THIRD_PARTY_NOTICES.md', 'README.md', 'CHANGELOG.md')) {
    if (Test-Path (Join-Path $repository $file)) { Copy-Item -LiteralPath (Join-Path $repository $file) -Destination $stage }
}
if (Test-Path (Join-Path $repository 'docs')) { Copy-Item -LiteralPath (Join-Path $repository 'docs') -Destination $stage -Recurse }
if (Test-Path (Join-Path $repository 'assets\models')) { Copy-Item -LiteralPath (Join-Path $repository 'assets\models') -Destination (Join-Path $licenses 'face-models') -Recurse }

# Include resolved dependency metadata and any license/notice files in every crate.
Push-Location $repository
try {
    $metadataJson = & cargo metadata --locked --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Could not collect Cargo dependency licenses.' }
    $metadata = $metadataJson | ConvertFrom-Json
    $dependencies = @()
    foreach ($package in $metadata.packages) {
        if ($package.name -eq 'robocam') { continue }
        $crateRoot = Split-Path -Parent $package.manifest_path
        $crateDestination = Join-Path $licenses ("rust\{0}-{1}" -f $package.name, $package.version)
        New-Item -ItemType Directory -Force -Path $crateDestination | Out-Null
        Get-ChildItem -LiteralPath $crateRoot -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)' } |
            Copy-Item -Destination $crateDestination
        if ($package.license_file) {
            $explicitLicense = Join-Path $crateRoot $package.license_file
            if (Test-Path -LiteralPath $explicitLicense) { Copy-Item -LiteralPath $explicitLicense -Destination $crateDestination -Force }
        }
        $dependencies += [pscustomobject]@{ name=$package.name; version=$package.version; license=$package.license; repository=$package.repository }
    }
    $dependencies | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $licenses 'rust-dependencies.json') -Encoding UTF8
} finally { Pop-Location }

# Ship the exact robocam source used for this build, including local uncommitted edits.
foreach ($entry in @('.cargo', '.github', 'assets', 'docs', 'installer', 'scripts', 'src', 'tests', 'Cargo.toml', 'Cargo.lock', 'build.rs', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'README.md', 'AGENTS.md', 'CHANGELOG.md', 'plan.md', '.gitignore')) {
    $path = Join-Path $repository $entry
    if (Test-Path -LiteralPath $path) { Copy-Item -LiteralPath $path -Destination $sourceStage -Recurse }
}
$sourceArchive = Join-Path $distribution "robocam-$version-source.zip"
New-PackageZip $sourceStage $sourceArchive
Copy-Item -LiteralPath $sourceArchive -Destination $stage
& (Join-Path $PSScriptRoot 'smoke-test.ps1') -Executable (Join-Path $stage 'robocam.exe')
$portableArchive = Join-Path $distribution "robocam-$version-windows-x64.zip"
New-PackageZip $stage $portableArchive

if (!$InnoCompiler) {
    $localCompiler = Join-Path $repository '.tools\innosetup\app\ISCC.exe'
    if (Test-Path -LiteralPath $localCompiler) { $InnoCompiler = $localCompiler }
    else {
        $availableCompiler = Get-Command ISCC.exe -ErrorAction SilentlyContinue
        if ($availableCompiler) { $InnoCompiler = $availableCompiler.Source }
        else {
            & (Join-Path $PSScriptRoot 'bootstrap.ps1') -Installer
            $InnoCompiler = $localCompiler
        }
    }
}
& $InnoCompiler "/DAppVersion=$version" "/DSourceDir=$stage" "/DOutputDir=$distribution" (Join-Path $repository 'installer\robocam.iss')
if ($LASTEXITCODE -ne 0) { throw "Installer compilation failed ($LASTEXITCODE)." }
$installer = Join-Path $distribution "robocam-$version-windows-x64-setup.exe"
@($installer, $portableArchive, $sourceArchive) | ForEach-Object {
    $hash = Get-FileHash -LiteralPath $_ -Algorithm SHA256
    '{0}  {1}' -f $hash.Hash.ToLowerInvariant(), [IO.Path]::GetFileName($_)
} | Set-Content -LiteralPath (Join-Path $distribution 'SHA256SUMS.txt') -Encoding ASCII
Write-Host "Installer: $installer"
Write-Host "Portable:  $portableArchive"
Write-Host "Source:    $sourceArchive"
