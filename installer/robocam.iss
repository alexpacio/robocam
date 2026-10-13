; Run scripts\package.ps1 to stage the application and compile this installer.
#ifndef AppVersion
  #define AppVersion "0.1.0"
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
AppPublisher=robocam contributors
DefaultDirName={localappdata}\Programs\robocam
DefaultGroupName=robocam
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
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
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Registry]
; Share one startup entry with the application's General settings page.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "robocam"; ValueData: """{app}\robocam.exe"" --minimized"; Tasks: autostart; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "robocam"; Tasks: not autostart; Flags: deletevalue uninsdeletevalue

[Icons]
Name: "{group}\robocam"; Filename: "{app}\robocam.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\robocam"; Filename: "{app}\robocam.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\robocam.exe"; Description: "Launch robocam"; Flags: nowait postinstall skipifsilent
