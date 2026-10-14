# Dot-source this script to configure the current PowerShell process only.
[CmdletBinding()]
param([string]$OpenCVRoot = $env:ROBOCAM_OPENCV_ROOT)
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
if (!$OpenCVRoot) {
    foreach ($candidate in @((Join-Path $repository '.tools\opencv'), 'C:\tools\opencv')) {
        if (Test-Path (Join-Path $candidate 'build\include\opencv2\core.hpp')) {
            $OpenCVRoot = $candidate
            break
        }
    }
}
if (!$OpenCVRoot -or !(Test-Path (Join-Path $OpenCVRoot 'build\x64\vc16\lib\opencv_world4100.lib'))) {
    throw 'OpenCV 4.10.0 x64 SDK missing. Run scripts\bootstrap.ps1 -OpenCV or set ROBOCAM_OPENCV_ROOT to its root.'
}
$env:ROBOCAM_OPENCV_ROOT = (Resolve-Path -LiteralPath $OpenCVRoot).Path
$env:OPENCV_INCLUDE_PATHS = Join-Path $env:ROBOCAM_OPENCV_ROOT 'build\include'
$env:OPENCV_LINK_PATHS = Join-Path $env:ROBOCAM_OPENCV_ROOT 'build\x64\vc16\lib'
$env:OPENCV_LINK_LIBS = 'opencv_world4100'
if (!$env:LIBCLANG_PATH) {
    foreach ($candidate in @('C:\Program Files\LLVM\bin', 'C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools\VC\Tools\Llvm\x64\bin')) {
        if (Test-Path (Join-Path $candidate 'libclang.dll')) {
            $env:LIBCLANG_PATH = $candidate
            break
        }
    }
}
if (!$env:LIBCLANG_PATH -or !(Test-Path (Join-Path $env:LIBCLANG_PATH 'libclang.dll'))) {
    throw 'LLVM libclang.dll missing. Set LIBCLANG_PATH to the directory containing this DLL.'
}
$nativeDirectory = Join-Path $env:ROBOCAM_OPENCV_ROOT 'build\x64\vc16\bin'
if (($env:PATH -split ';') -notcontains $nativeDirectory) { $env:PATH = "$nativeDirectory;$env:PATH" }
