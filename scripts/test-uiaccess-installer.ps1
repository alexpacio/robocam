# Exercise the production Inno callback and launch guard without certificates,
# elevation, camera access, input injection or changing the real installation.
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
$root = Join-Path $repository ('.tools\uiaccess-installer-tests-' + [Guid]::NewGuid().ToString('N'))
$stage = Join-Path $root 'stage'
New-Item -ItemType Directory -Path $stage -Force | Out-Null
$appId = [Guid]::NewGuid().ToString().ToUpperInvariant()
$fixture = Get-Content -LiteralPath (Join-Path $repository 'installer\robocam.iss') -Raw
$fixture = $fixture.Replace('AppId={{5B6EA626-8D46-4733-857F-93F136759B53}', ('AppId={{' + $appId + '}'))
$fixture = $fixture.Replace('PrivilegesRequired=admin', 'PrivilegesRequired=lowest').Replace('MinVersion=10.0.22000', 'MinVersion=10.0')
$fixture = $fixture.Replace('SetupIconFile=..\assets\robocam.ico', ('SetupIconFile=' + (Join-Path $repository 'assets\robocam.ico')))
# Keep the actual file callback, custom exit status and UIAccessReady function.
# Replace user-specific integration with a harmless, observable launch action.
$fixture = [regex]::Replace($fixture, '(?ms)^\[(Registry|Icons|UninstallRun)\]\r?\n.*?(?=^\[|\z)', '')
$fixture = [regex]::Replace($fixture, '(?ms)^\[Run\]\r?\n.*?(?=^\[|\z)', "[Run]`r`nFilename: `"{app}\robocam.exe`"; Check: UIAccessReady; Flags: runhidden waituntilterminated`r`n`r`n")
$fixturePath = Join-Path $root 'fixture.iss'
[IO.File]::WriteAllText($fixturePath, $fixture)
Copy-Item -LiteralPath (Join-Path $repository 'LICENSE') -Destination $stage
# Simulate the signing helper's process boundary, including a terminating error.
[IO.File]::WriteAllText((Join-Path $stage 'enable-uiaccess.ps1'), @'
param([switch]$Reapply, [string]$Executable)
$ErrorActionPreference = 'Stop'
if ($Executable -like '*\failure\*') { throw 'Simulated missing machine signing key.' }
exit 0
'@)
$source = Join-Path $root 'fixture.cs'
[IO.File]::WriteAllText($source, @'
using System;
using System.IO;
using System.Reflection;
class Fixture {
    static void Main() {
        File.WriteAllText(Path.Combine(Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location), "launched.txt"), "yes");
    }
}
'@)
$compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
& $compiler /nologo /target:winexe (('/out:') + (Join-Path $stage 'robocam.exe')) $source
if ($LASTEXITCODE -ne 0) { throw 'Fixture application compilation failed.' }
$inno = Join-Path $repository '.tools\innosetup\app\ISCC.exe'
& $inno /Q ("/DSourceDir=$stage") ("/DOutputDir=$root") $fixturePath
if ($LASTEXITCODE -ne 0) { throw 'Fixture installer compilation failed.' }
$installer = (Get-ChildItem -LiteralPath $root -Filter '*-setup.exe').FullName
try {
    foreach ($scenario in @('failure', 'success')) {
        $destination = Join-Path $root $scenario
        $arguments = @('/SP-', '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOCLOSEAPPLICATIONS',
            ('/DIR="' + $destination + '"'), ('/LOG="' + (Join-Path $root ($scenario + '.log')) + '"'))
        $setup = Start-Process -FilePath $installer -ArgumentList $arguments -WindowStyle Hidden -PassThru
        if (!$setup.WaitForExit(30000)) { throw 'Fixture installer timed out.' }
        $expected = if ($scenario -eq 'failure') { 10 } else { 0 }
        if ($setup.ExitCode -ne $expected) { throw "Unexpected $scenario exit status $($setup.ExitCode); expected $expected." }
        $launched = Test-Path -LiteralPath (Join-Path $destination 'launched.txt')
        if ($launched -ne ($scenario -eq 'success')) { throw "Application launch gating failed for $scenario." }
        Write-Host "UIAccess installer $scenario test passed (exit $expected)."
    }
} finally {
    # Only remove this test's randomly generated, per-user uninstall registration.
    $registration = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{$appId}_is1"
    if (Test-Path -LiteralPath $registration) { Remove-Item -LiteralPath $registration -Recurse -Force }
}
