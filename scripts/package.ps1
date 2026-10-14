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
$versionInfo = (Get-Item -LiteralPath $releaseExecutable).VersionInfo
if ($versionInfo.ProductVersion -cne "$version.0") {
    throw 'Application ProductVersion text must match X.Y.Z.0.'
}
$numericProductVersion = '{0}.{1}.{2}.{3}' -f $versionInfo.ProductMajorPart, $versionInfo.ProductMinorPart, $versionInfo.ProductBuildPart, $versionInfo.ProductPrivatePart
if ($numericProductVersion -ne "$version.0" -or $versionInfo.FileVersion -ne "$version.0") {
    throw 'Application Windows version resources must match X.Y.Z.0.'
}
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
foreach ($file in @('LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md', 'README.md', 'CHANGELOG.md')) {
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
    $dependencyIndex = [System.Collections.Generic.List[string]]::new()
    $dependencyIndex.Add('# Rust dependency licenses')
    $dependencyIndex.Add('')
    $dependencyIndex.Add('Generated from the exact Cargo.lock resolution used for this package, including transitive/build dependencies and other resolved targets. This list does not imply that every crate is linked into the Windows executable. Original texts control; OR denotes alternatives and AND denotes combined requirements. winsplit 0.1.0 uses its Apache-2.0 option.')
    $dependencyIndex.Add('')
    $dependencyIndex.Add('| Dependency | Declared license | Original license / notice files | Upstream |')
    $dependencyIndex.Add('| --- | --- | --- | --- |')
    foreach ($package in ($metadata.packages | Sort-Object name, version)) {
        if ($package.name -eq 'robocam') { continue }
        $crateRoot = Split-Path -Parent $package.manifest_path
        $crateDirectory = '{0}-{1}' -f $package.name, $package.version
        $crateDestination = Join-Path $licenses ("rust\$crateDirectory")
        New-Item -ItemType Directory -Force -Path $crateDestination | Out-Null
        # Some crates (including ring) reference license files in source subdirectories.
        $noticePattern = '^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT|AUTHORS)([._-]|$)'
        foreach ($file in (Get-ChildItem -LiteralPath $crateRoot -Recurse -File | Where-Object { $_.Name -match $noticePattern })) {
            $relative = $file.FullName.Substring($crateRoot.Length).TrimStart('\', '/')
            $destination = Join-Path $crateDestination $relative
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
            Copy-Item -LiteralPath $file.FullName -Destination $destination
        }
        Get-ChildItem -LiteralPath $crateRoot -File | Where-Object { $_.Name -match '^README([._-]|$)' } |
            Copy-Item -Destination $crateDestination
        if ($package.license_file) {
            $explicitLicense = Join-Path $crateRoot $package.license_file
            if (!(Test-Path -LiteralPath $explicitLicense)) { throw "Missing explicit license file for $crateDirectory" }
            Copy-Item -LiteralPath $explicitLicense -Destination $crateDestination -Force
        }
        $supplement = Join-Path $repository "assets\licenses\rust\$crateDirectory"
        if (Test-Path -LiteralPath $supplement) {
            Copy-Item -Path (Join-Path $supplement '*') -Destination $crateDestination -Recurse -Force
        }
        $notices = @(Get-ChildItem -LiteralPath $crateDestination -Recurse -File | Where-Object {
            $_.Name -match $noticePattern -or ($package.license_file -and $_.Name -eq (Split-Path -Leaf $package.license_file))
        })
        if ($notices.Count -eq 0) { throw "No license notice collected for $crateDirectory; review its upstream license before packaging." }
        $licenseFiles = @($notices | Sort-Object FullName | ForEach-Object {
            'rust/' + $crateDirectory + '/' + $_.FullName.Substring($crateDestination.Length).TrimStart('\', '/').Replace('\', '/')
        })
        $noticeLinks = ($licenseFiles | ForEach-Object {
            $url = (($_ -split '/') | ForEach-Object { [Uri]::EscapeDataString($_) }) -join '/'
            '[' + $_.Substring(('rust/' + $crateDirectory + '/').Length) + '](' + $url + ')'
        }) -join ', '
        $crateUrl = 'https://crates.io/crates/' + $package.name + '/' + $package.version
        $upstream = '[crate](' + $crateUrl + ')'
        if ($package.repository) { $upstream += ', [repository](' + $package.repository + ')' }
        $dependencyIndex.Add('| ' + $crateDirectory + ' | ' + $package.license + ' | ' + $noticeLinks + ' | ' + $upstream + ' |')
        $dependencies += [pscustomobject]@{
            name=$package.name; version=$package.version; license=$package.license
            repository=$package.repository; authors=@($package.authors); license_files=$licenseFiles
        }
    }
    $dependencies | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $licenses 'rust-dependencies.json') -Encoding UTF8
    $dependencyIndex | Set-Content -LiteralPath (Join-Path $licenses 'RUST_DEPENDENCIES.md') -Encoding UTF8
} finally { Pop-Location }

Copy-Item -LiteralPath (Join-Path $repository 'assets\licenses\README.md') -Destination (Join-Path $licenses 'SUPPLEMENTS.md')

# Ship the exact robocam source used for this build, including local uncommitted edits.
foreach ($entry in @('.cargo', '.github', 'assets', 'docs', 'installer', 'scripts', 'src', 'tests', 'Cargo.toml', 'Cargo.lock', 'build.rs', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md', 'README.md', 'AGENTS.md', 'CHANGELOG.md', 'plan.md', '.gitignore')) {
    $path = Join-Path $repository $entry
    if (Test-Path -LiteralPath $path) { Copy-Item -LiteralPath $path -Destination $sourceStage -Recurse }
}
& (Join-Path $PSScriptRoot 'smoke-test.ps1') -Executable (Join-Path $stage 'robocam.exe')

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
$installerLicense = Join-Path (Split-Path -Parent $InnoCompiler) 'license.txt'
if (!(Test-Path -LiteralPath $installerLicense)) { throw 'Inno Setup license.txt missing beside compiler.' }
$installerNotices = Join-Path $licenses 'installer'
New-Item -ItemType Directory -Force -Path $installerNotices | Out-Null
Copy-Item -LiteralPath $installerLicense -Destination $installerNotices

$nativeIndex = [System.Collections.Generic.List[string]]::new()
$nativeIndex.Add('# Dependency license collection')
$nativeIndex.Add('')
$nativeIndex.Add('See [Rust dependency licenses](RUST_DEPENDENCIES.md) for every resolved crate and [third-party notices](../THIRD_PARTY_NOTICES.md) for attribution, license summaries and upstream citations. Intel IPP and the Microsoft runtime retain their separate proprietary terms.')
$nativeIndex.Add('')
$nativeIndex.Add('The following original files are preserved from the OpenCV SDK, face model and installer engine. The complete SDK notice collection includes notices for modules/plugins not distributed by Robocam; inclusion of a notice does not imply that its component is shipped.')
$nativeIndex.Add('')
foreach ($directory in @($opencvLicenses, (Join-Path $licenses 'face-models'), $installerNotices)) {
    foreach ($file in (Get-ChildItem -LiteralPath $directory -Recurse -File | Sort-Object FullName)) {
        $relative = $file.FullName.Substring($licenses.Length).TrimStart('\', '/').Replace('\', '/')
        $url = (($relative -split '/') | ForEach-Object { [Uri]::EscapeDataString($_) }) -join '/'
        $nativeIndex.Add('- [' + $relative + '](' + $url + ')')
    }
}
$nativeIndex | Set-Content -LiteralPath (Join-Path $licenses 'DEPENDENCIES.md') -Encoding UTF8
Copy-Item -LiteralPath $licenses -Destination (Join-Path $sourceStage 'licenses') -Recurse
$sourceArchive = Join-Path $distribution "robocam-$version-source.zip"
New-PackageZip $sourceStage $sourceArchive
Copy-Item -LiteralPath $sourceArchive -Destination $stage
$portableArchive = Join-Path $distribution "robocam-$version-windows-x64.zip"
New-PackageZip $stage $portableArchive

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
