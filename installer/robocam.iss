; Run scripts\package.ps1 to stage the application and compile this installer.
#ifndef AppVersion
  #define AppVersion "1.1.3"
#endif
#ifndef SourceDir
  #error SourceDir must name the staged application directory
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
AppId={{5B6EA626-8D46-4733-857F-93F136759B53}
AppName=robocam
AppVersion={#AppVersion}
VersionInfoVersion={#AppVersion}.0
VersionInfoTextVersion={#AppVersion}.0
VersionInfoProductVersion={#AppVersion}.0
AppPublisher=Alessandro Bolletta
DefaultDirName={autopf}\robocam
UsePreviousAppDir=no
DefaultGroupName=robocam
DisableProgramGroupPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
MinVersion=10.0.22000
WizardStyle=modern
SetupIconFile=..\assets\robocam.ico
LicenseFile={#SourceDir}\LICENSE
OutputDir={#OutputDir}
OutputBaseFilename=robocam-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
UninstallDisplayIcon={app}\robocam.exe
CloseApplications=yes
RestartApplications=no
SetupLogging=yes

[Tasks]
Name: autostart; Description: "Start robocam when I sign in to Windows"; GroupDescription: "Startup:"
Name: desktopicon; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#SourceDir}\enable-uiaccess.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\*"; DestDir: "{app}"; Excludes: "robocam.exe,enable-uiaccess.ps1"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#SourceDir}\robocam.exe"; DestDir: "{app}"; Flags: ignoreversion; AfterInstall: ReapplyUIAccess

[Registry]
; Elevated startup is registered as an interactive logon task by robocam.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "robocam"; Flags: deletevalue uninsdeletevalue

[Icons]
Name: "{userprograms}\robocam\robocam"; Filename: "{app}\robocam.exe"; WorkingDir: "{app}"
Name: "{userdesktop}\robocam"; Filename: "{app}\robocam.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\robocam.exe"; Parameters: "--configure-startup enable"; Tasks: autostart; Check: NotUpdating; Flags: shellexec runhidden waituntilterminated runascurrentuser
Filename: "{app}\robocam.exe"; Parameters: "--configure-startup disable"; Tasks: not autostart; Check: NotUpdating; Flags: shellexec runhidden waituntilterminated runascurrentuser
Filename: "{app}\robocam.exe"; Description: "Launch robocam"; Check: UIAccessReady; Flags: shellexec nowait postinstall skipifsilent

[UninstallRun]
Filename: "{app}\robocam.exe"; Parameters: "--configure-startup disable"; RunOnceId: "RemoveStartupTask"; Flags: shellexec runhidden waituntilterminated

[Code]
var
  UIAccessFailed: Boolean;

function UIAccessReady: Boolean;
begin
  Result := not UIAccessFailed;
end;

function GetCustomSetupExitCode: Integer;
begin
  Result := 0;
  if UIAccessFailed then
    Result := 10;
end;

procedure ReapplyUIAccess;
var
  ResultCode: Integer;
begin
  { Re-sign before startup configuration, application launch or updater restart.
    Inno's file callbacks swallow exceptions: explicitly fail the exit status
    and suppress launches so the updater can restore its application backup. }
  Log('Reapplying machine-local UIAccess configuration.');
  if not Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
    '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' +
    ExpandConstant('{app}\enable-uiaccess.ps1') + '" -Reapply -Executable "' +
    ExpandConstant('{app}\robocam.exe') + '"',
    ExpandConstant('{app}'), SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    UIAccessFailed := True;
  Log('UIAccess configuration exit code: ' + IntToStr(ResultCode));
  if ResultCode <> 0 then
    UIAccessFailed := True;
  if UIAccessFailed then
    SuppressibleMsgBox('Robocam UIAccess signing failed. Run enable-uiaccess.ps1 -Reapply as administrator to diagnose the machine signing certificate.', mbError, MB_OK, IDOK);
end;

function NotUpdating: Boolean;
var
  I: Integer;
begin
  Result := not UIAccessFailed;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/ROBOCAMUPDATE=1') = 0 then
      Result := False;
end;
