# Run with Windows PowerShell 5.1. All executables and profiles here are disposable fixtures.
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
$root = Join-Path $repository ('.tools\updater-tests-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $root | Out-Null
$compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
$applicationSource = @'
using System;
using System.IO;
using System.Diagnostics;
using System.Threading;
using System.Reflection;
[assembly:AssemblyVersion("__VERSION__.__REVISION__")]
[assembly:AssemblyFileVersion("__VERSION__.__REVISION__")]
[assembly:AssemblyInformationalVersion("__VERSION__.__REVISION__")]
class Fixture {
 static void Main(string[] args) {
  if(args.Length > 0 && args[0] == "--parent") {
   for(int i=0;i<1200 && !File.Exists(args[1]);i++) Thread.Sleep(100);
   return;
  }
  for(int i=0;i<args.Length-1;i++) if(args[i]=="--diagnostic-report") {
   File.WriteAllText(args[i+1],"{\"running\":true,\"version\":\"__VERSION__\",\"process_id\":"+Process.GetCurrentProcess().Id+"}");
  }
  Thread.Sleep(120000);
 }
}
'@
$installerSource = @'
using System;
using System.IO;
using System.Reflection;
class Installer {
 static int Main(string[] args) {
  string root=Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location);
  File.WriteAllText(Path.Combine(root,"installer-ran.txt"),"yes");
  string payload=Path.Combine(root,"success-payload.exe");
  if(File.Exists(payload)) {
   File.Copy(payload,Path.Combine(root,"app","robocam.exe"),true);
   return 0;
  }
  File.WriteAllText(Path.Combine(root,"app","robocam.exe"),"broken binary");
  File.WriteAllText(Path.Combine(root,"profile","settings.json"),"broken profile");
  return 9;
 }
}
'@
$appSource = Join-Path $root 'app.cs'
$setupSource = Join-Path $root 'setup.cs'
[IO.File]::WriteAllText($setupSource,$installerSource)
$scenarios = @('checksum','cancel','failure','success','mismatch','nonzero-revision')
foreach ($scenario in $scenarios) {
    $directory=Join-Path $root $scenario
    $appDirectory=Join-Path $directory 'app'
    New-Item -ItemType Directory -Force $appDirectory,(Join-Path $directory 'profile') | Out-Null
    $exe=Join-Path $appDirectory 'robocam.exe'
    $installer=Join-Path $directory 'robocam-2.0.0-windows-x64-setup.exe'
    [IO.File]::WriteAllText($appSource,$applicationSource.Replace('__VERSION__','1.0.0').Replace('__REVISION__','0'))
    & $compiler /nologo /target:winexe ('/out:'+$exe) $appSource
    if($LASTEXITCODE -ne 0){throw 'Fixture application compilation failed'}
    & $compiler /nologo /target:winexe ('/out:'+$installer) $setupSource
    if($LASTEXITCODE -ne 0){throw 'Fixture installer compilation failed'}
    $successful = $scenario -eq 'success'
    $replacementHash = $null
    if ($successful -or $scenario -in @('mismatch','nonzero-revision')) {
        $newVersion = if ($scenario -eq 'mismatch') { '2.0.1' } else { '2.0.0' }
        $revision = if ($scenario -eq 'nonzero-revision') { '1' } else { '0' }
        $replacementSource = Join-Path $directory 'new-app.cs'
        $replacement = Join-Path $directory 'success-payload.exe'
        [IO.File]::WriteAllText($replacementSource,$applicationSource.Replace('__VERSION__',$newVersion).Replace('__REVISION__',$revision))
        & $compiler /nologo /target:winexe ('/out:'+$replacement) $replacementSource
        if($LASTEXITCODE -ne 0){throw 'Replacement fixture compilation failed'}
        $replacementHash = (Get-FileHash -LiteralPath $replacement).Hash
    }
    $settings=Join-Path $directory 'profile\settings.json'
    [IO.File]::WriteAllText($settings,'{"active_profile":"Robo","custom_dwell":350}')
    $exeHash=(Get-FileHash $exe).Hash
    $settingsHash=(Get-FileHash $settings).Hash
    $shutdown=Join-Path $directory 'shutdown'
    $parent=Start-Process -FilePath $exe -ArgumentList @('--parent',('"'+$shutdown+'"')) -WindowStyle Hidden -PassThru
    $helper=$null
    $result=$null
    try {
        $sha=(Get-FileHash $installer).Hash
        if($scenario -eq 'checksum'){$sha='0'*64}
        $job=@{parent_id=$parent.Id;executable=$exe;from_version='1.0.0';to_version='2.0.0';installer=$installer;sha256=$sha;settings=$settings;restart_arguments=@('--paused')}
        $jobPath=Join-Path $directory 'job.json'
        [IO.File]::WriteAllText($jobPath,($job|ConvertTo-Json -Depth 5))
        $helperScript = Join-Path $repository 'assets\apply-update.ps1'
        $helper=Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',('"'+$helperScript+'"'),'-JobPath',('"'+$jobPath+'"')) -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $directory 'stderr.txt')
        $null=$helper.Handle
        if($scenario -ne 'checksum') {
            $handoff=Join-Path $directory 'handoff.json'
            for($i=0;$i -lt 100 -and !(Test-Path $handoff) -and !$helper.HasExited;$i++){Start-Sleep -Milliseconds 100}
            if(!(Test-Path $handoff) -or (Get-Content $handoff -Raw|ConvertFrom-Json).state -ne 'ready'){throw "Helper never became ready: $(Get-Content (Join-Path $directory 'stderr.txt') -Raw)"}
            [IO.File]::WriteAllText($(if($scenario -eq 'cancel'){Join-Path $directory 'cancel'}else{$shutdown}),'')
        }
        if(!$helper.WaitForExit(30000)){throw 'Update helper test timed out'}
        $result=Get-Content (Join-Path $directory 'result.json') -Raw|ConvertFrom-Json
        if ($successful) {
            if($result.state -ne 'complete' -or $result.installer_exit -ne 0 -or !$result.settings_preserved -or !$result.restart_pid){throw 'Successful installation/restart was not confirmed'}
            if((Get-FileHash $exe).Hash -ne $replacementHash -or (Get-FileHash $settings).Hash -ne $settingsHash){throw 'Installed binary or preserved settings mismatch'}
            if(!(Get-Process -Id $result.restart_pid -ErrorAction SilentlyContinue)){throw 'Updated application is not running'}
        } elseif ($scenario -in @('failure','mismatch','nonzero-revision')) {
            if($result.state -ne 'failed'){throw 'Failure was not reported'}
            if((Get-FileHash $exe).Hash -ne $exeHash -or (Get-FileHash $settings).Hash -ne $settingsHash){throw 'Application/profile was not preserved'}
            $expectedError = if ($scenario -eq 'mismatch') { 'The installed version does not match the release.' } elseif ($scenario -eq 'nonzero-revision') { 'Application Windows product revision must be zero.' } else { 'Installer failed with exit code 9.' }
            $expectedExit = if ($scenario -in @('mismatch','nonzero-revision')) { 0 } else { 9 }
            if($result.error -ne $expectedError -or $result.installer_exit -ne $expectedExit -or !$result.settings_preserved -or !$result.restart_pid){throw 'Failed installation did not recover and restart with the original error'}
            if(!(Get-Process -Id $result.restart_pid -ErrorAction SilentlyContinue)){throw 'Recovered application is not running'}
        } else {
            if($result.state -ne 'failed'){throw 'Failure was not reported'}
            if((Get-FileHash $exe).Hash -ne $exeHash -or (Get-FileHash $settings).Hash -ne $settingsHash){throw 'Application/profile was not preserved'}
            if(Test-Path (Join-Path $directory 'installer-ran.txt')){throw 'Unverified or cancelled installer ran'}
            if($parent.HasExited){throw 'Application was closed before a verified installation'}
        }
        Write-Host "Updater helper $scenario test passed"
    } finally {
        [IO.File]::WriteAllText($shutdown,'')
        if($result -and $result.restart_pid){Stop-Process -Id $result.restart_pid -ErrorAction SilentlyContinue}
        if($helper -and !$helper.HasExited){Stop-Process -Id $helper.Id -ErrorAction SilentlyContinue}
        if(!$parent.HasExited){$null=$parent.WaitForExit(3000)}
        if(!$parent.HasExited){Stop-Process -Id $parent.Id -ErrorAction SilentlyContinue}
    }
}
