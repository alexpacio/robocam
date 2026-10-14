[CmdletBinding()]
param(
    [switch]$OpenCV,
    [switch]$Installer
)
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
$toolsDirectory = Join-Path $repository '.tools'
New-Item -ItemType Directory -Force -Path $toolsDirectory | Out-Null
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

function Get-VerifiedDownload([string]$Uri, [string]$Name, [string]$Sha256) {
    $destination = Join-Path $toolsDirectory $Name
    if (!(Test-Path -LiteralPath $destination)) {
        Write-Host "Downloading $Name"
        Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $destination
    }
    if ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ne $Sha256) {
        throw "Checksum mismatch: $destination. Remove this download and retry."
    }
    return $destination
}

if ($OpenCV -and !(Test-Path (Join-Path $toolsDirectory 'opencv\build\include\opencv2\core.hpp'))) {
    $archive = Get-VerifiedDownload `
        'https://github.com/opencv/opencv/releases/download/4.10.0/opencv-4.10.0-windows.exe' `
        'opencv-4.10.0-windows.exe' `
        'BFF38466091C313DAC21A0B73EEA8278316A89C1D434C6F0B10697E087670168'
    $sevenZip = Get-Command 7z.exe -ErrorAction SilentlyContinue
    if (!$sevenZip) { throw '7-Zip is required to extract the OpenCV SDK; add 7z.exe to PATH.' }
    & $sevenZip.Source x -y "-o$toolsDirectory" $archive | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "OpenCV extraction failed ($LASTEXITCODE)." }
}

if ($Installer -and !(Test-Path (Join-Path $toolsDirectory 'innosetup\app\ISCC.exe'))) {
    $extractorArchive = Get-VerifiedDownload `
        'https://github.com/dscharrer/innoextract/releases/download/1.9/innoextract-1.9-windows.zip' `
        'innoextract-1.9-windows.zip' `
        '6989342C9B026A00A72A38F23B62A8E6A22CC5DE69805CF47D68AC2FEC993065'
    $compilerArchive = Get-VerifiedDownload `
        'https://github.com/jrsoftware/issrc/releases/download/is-6_2_2/innosetup-6.2.2.exe' `
        'innosetup-6.2.2.exe' `
        '8117D10D00A2AD33A1390978EA3872861C330E087914410A6377B22C4C5B8563'
    $extractorDirectory = Join-Path $toolsDirectory 'innoextract'
    Expand-Archive -LiteralPath $extractorArchive -DestinationPath $extractorDirectory -Force
    & (Join-Path $extractorDirectory 'innoextract.exe') -q -d (Join-Path $toolsDirectory 'innosetup') $compilerArchive | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup extraction failed ($LASTEXITCODE)." }
}
Write-Host "Dependencies are available in $toolsDirectory. No global settings were changed."
