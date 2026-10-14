param([Parameter(Mandatory=$true)][string]$JobPath)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$root = Split-Path -Parent ([IO.Path]::GetFullPath($JobPath))
$resultPath = Join-Path $root 'result.json'
$handoffPath = Join-Path $root 'handoff.json'
$cancelPath = Join-Path $root 'cancel'
$backup = Join-Path $root 'previous-app'
$settingsBackup = Join-Path $root 'settings-before.json'
$parentExited = $false
$installAttempted = $false
$installerLock = $null
$mutex = $null
$parent = $null
$job = $null
$settingsHash = $null
$outcome = [ordered]@{state='preparing';error=$null;installer_exit=$null;settings_preserved=$false;restart_pid=$null}
function Write-Json([string]$Path, $Value) {
    $temporary = $Path + '.tmp'
    [IO.File]::WriteAllText($temporary, ($Value | ConvertTo-Json -Depth 8))
    Move-Item -LiteralPath $temporary -Destination $Path -Force
}
function Check-Cancel {
    if (Test-Path -LiteralPath $cancelPath) { throw 'Update cancelled.' }
}
function Get-ApplicationVersion([string]$Path) {
    # Compare numeric Windows resources with the release's X.Y.Z version.
    # Localized/display version strings are not installation identity.
    $info = (Get-Item -LiteralPath $Path).VersionInfo
    if ($info.ProductPrivatePart -ne 0) {
        throw 'Application Windows product revision must be zero.'
    }
    return ('{0}.{1}.{2}' -f $info.ProductMajorPart, $info.ProductMinorPart, $info.ProductBuildPart)
}
try {
    $job = Get-Content -LiteralPath $JobPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($job.to_version -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' -or
        [version]$job.to_version -le [version]$job.from_version) { throw 'The update is not a newer stable version.' }
    $executable = [IO.Path]::GetFullPath($job.executable)
    $installDirectory = Split-Path -Parent $executable
    if ([IO.Path]::GetFileName($executable) -ne 'robocam.exe' -or
        (Get-ApplicationVersion $executable) -ne $job.from_version) { throw 'The installed application changed.' }
    $installer = [IO.Path]::GetFullPath($job.installer)
    if ((Split-Path -Parent $installer) -ne $root -or
        [IO.Path]::GetFileName($installer) -cne ('robocam-' + $job.to_version + '-windows-x64-setup.exe')) { throw 'Invalid installer path.' }
    $created = $false
    $mutex = New-Object Threading.Mutex($true, 'Local\robocam.Updating', [ref]$created)
    if (!$created) { throw 'Another update is already in progress.' }
    $parent = Get-Process -Id $job.parent_id
    if ($parent.Path -ne $executable) { throw 'The running application changed.' }
    $null = $parent.Handle
    # Keep a read-only sharing lock from verification through installer completion.
    $installerLock = [IO.File]::Open($installer, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $actual = ([BitConverter]::ToString($sha.ComputeHash($installerLock))).Replace('-','') }
    finally { $sha.Dispose() }
    if ($actual -ine $job.sha256) { throw 'Installer checksum mismatch. Nothing was installed.' }
    Check-Cancel
    Copy-Item -LiteralPath $installDirectory -Destination $backup -Recurse
    Check-Cancel
    Write-Json $handoffPath @{state='ready'}
    $deadline = [DateTime]::UtcNow.AddSeconds(120)
    while (!$parent.WaitForExit(200)) {
        Check-Cancel
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Robocam did not close. Nothing was installed.' }
    }
    $parentExited = $true
    Check-Cancel
    $restartState = Join-Path $root 'restart-state.json'
    if (Test-Path -LiteralPath $restartState) {
        $state = Get-Content -LiteralPath $restartState -Raw -Encoding UTF8 | ConvertFrom-Json
        $job.restart_arguments = @(if ($state.enabled) { '--resume-clicks' } else { '--paused' })
        if ($state.full_paused) { $job.restart_arguments += '--fully-paused' }
        if ($state.minimized) { $job.restart_arguments += '--minimized' }
    }
    if (Test-Path -LiteralPath $job.settings) {
        Copy-Item -LiteralPath $job.settings -Destination $settingsBackup
        $settingsHash = (Get-FileHash -LiteralPath $settingsBackup).Hash
    }
    $outcome.state = 'installing'
    Write-Json $resultPath $outcome
    $arguments = @('/SP-', '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/RESTARTEXITCODE=3010',
        '/NOCLOSEAPPLICATIONS', '/NOFORCECLOSEAPPLICATIONS', '/NORESTARTAPPLICATIONS', '/ROBOCAMUPDATE=1',
        ('/DIR="' + $installDirectory + '"'), ('/LOG="' + (Join-Path $root 'install.log') + '"'))
    $installAttempted = $true
    $setup = Start-Process -FilePath $installer -ArgumentList $arguments -WindowStyle Hidden -PassThru
    $null = $setup.Handle
    # Do not terminate an installer in the middle of replacing application files.
    $setup.WaitForExit()
    $outcome.installer_exit = $setup.ExitCode
    if ($setup.ExitCode -ne 0 -and $setup.ExitCode -ne 3010) { throw ('Installer failed with exit code ' + $setup.ExitCode + '.') }
    if ($setup.ExitCode -eq 0 -and (Get-ApplicationVersion $executable) -ne $job.to_version) { throw 'The installed version does not match the release.' }
    if ($settingsHash -and (Get-FileHash -LiteralPath $job.settings).Hash -ne $settingsHash) { throw 'Saved settings changed during installation.' }
    $outcome.settings_preserved = $true
    $outcome.state = if ($setup.ExitCode -eq 3010) { 'restart_required' } else { 'installed' }
} catch {
    $outcome.state = 'failed'
    $outcome.error = $_.Exception.Message
    if ($installAttempted -and (Test-Path -LiteralPath $backup)) {
        try {
            # Copy only into the original installation directory; never recursively delete it.
            foreach ($entry in Get-ChildItem -LiteralPath $backup) {
                Copy-Item -LiteralPath $entry.FullName -Destination $installDirectory -Recurse -Force
            }
            if ($settingsHash) { Copy-Item -LiteralPath $settingsBackup -Destination $job.settings -Force }
            $outcome.settings_preserved = !$settingsHash -or (Get-FileHash -LiteralPath $job.settings).Hash -eq $settingsHash
        } catch { $outcome.error += ' Recovery failed: ' + $_.Exception.Message }
    }
    Write-Json $handoffPath @{state='failed';error=$outcome.error}
} finally {
    if ($installerLock) { $installerLock.Dispose() }
    if ($mutex) { $mutex.Dispose() }
}
Write-Json $resultPath $outcome
if ($parentExited) {
    try {
        $diagnostic = Join-Path $root 'restarted.json'
        $arguments = @($job.restart_arguments) + @('--update-result', $resultPath, '--diagnostic-report', $diagnostic)
        # All path arguments are Windows filenames (which cannot contain quotes).
        $quoted = @($arguments | ForEach-Object { '"' + $_ + '"' })
        $app = Start-Process -FilePath $executable -ArgumentList $quoted -WorkingDirectory $installDirectory -WindowStyle Hidden -PassThru
        $outcome.restart_pid = $app.Id
        $expectedVersion = if ($outcome.state -eq 'installed') { $job.to_version } else { Get-ApplicationVersion $executable }
        $ready = $false
        for ($attempt = 0; $attempt -lt 80; $attempt++) {
            if ($app.HasExited) { throw 'Robocam exited after the update.' }
            if (Test-Path -LiteralPath $diagnostic) {
                try { $live = Get-Content -LiteralPath $diagnostic -Raw -Encoding UTF8 | ConvertFrom-Json } catch { $live = $null }
                if ($live -and $live.running -and $live.process_id -eq $app.Id -and $live.version -eq $expectedVersion) { $ready = $true; break }
            }
            Start-Sleep -Milliseconds 250
        }
        if (!$ready) { throw 'Robocam did not confirm its restart. See the update log.' }
        if ($outcome.state -eq 'installed') { $outcome.state = 'complete' }
    } catch { $outcome.state = 'failed'; $outcome.error = $_.Exception.Message }
    Write-Json $resultPath $outcome
}
