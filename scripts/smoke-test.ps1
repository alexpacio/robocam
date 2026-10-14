[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Executable)
$ErrorActionPreference = 'Stop'
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$repository = Split-Path -Parent $PSScriptRoot
$smokeDirectory = Join-Path $repository ('.tools\smoke-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $smokeDirectory | Out-Null
$report = Join-Path $smokeDirectory 'result.txt'
$previousPath = $env:PATH
$previousAppData = $env:APPDATA
try {
    # Only app-local native DLLs and Windows are visible to this process.
    $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
    $env:APPDATA = $smokeDirectory
    $process = Start-Process -FilePath $Executable -WorkingDirectory (Split-Path -Parent $Executable) `
        -ArgumentList @('--smoke-test', '--smoke-report', ('"' + $report + '"')) -WindowStyle Hidden -PassThru
    if (!$process.WaitForExit(20000)) {
        Stop-Process -Id $process.Id -Force
        throw 'Native application smoke test did not finish within 20 seconds.'
    }
    if ($process.ExitCode -ne 0) { throw "Native application smoke test failed ($($process.ExitCode))." }
    if (!(Test-Path -LiteralPath $report) -or (Get-Content -LiteralPath $report -Raw) -notmatch 'clean shutdown OK') {
        throw 'Native application did not produce its successful startup/shutdown report.'
    }
    Write-Host (Get-Content -LiteralPath $report -Raw).Trim()
} finally {
    $env:PATH = $previousPath
    $env:APPDATA = $previousAppData
}
