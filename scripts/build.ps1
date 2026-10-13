[CmdletBinding()]
param(
    [ValidateSet('Debug', 'Release')][string]$Configuration = 'Release',
    [switch]$SkipTests,
    [switch]$BootstrapDependencies
)
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
if ($BootstrapDependencies) { & (Join-Path $PSScriptRoot 'bootstrap.ps1') -OpenCV }
. (Join-Path $PSScriptRoot 'build-env.ps1')
Push-Location $repository
try {
    $cargoArguments = @('--locked')
    if ($Configuration -eq 'Release') { $cargoArguments += '--release' }
    if (!$SkipTests) {
        & cargo test @cargoArguments
        if ($LASTEXITCODE -ne 0) { throw "Tests failed ($LASTEXITCODE)." }
    }
    & cargo build @cargoArguments
    if ($LASTEXITCODE -ne 0) { throw "Build failed ($LASTEXITCODE)." }
} finally { Pop-Location }
