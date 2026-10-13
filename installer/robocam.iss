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
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Registry]
; Elevated startup is registered as an interactive logon task by robocam.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "robocam"; Flags: deletevalue uninsdeletevalue

[Icons]
Name: "{userprograms}\robocam\robocam"; Filename: "{app}\robocam.exe"; WorkingDir: "{app}"
Name: "{userdesktop}\robocam"; Filename: "{app}\robocam.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\robocam.exe"; Parameters: "--configure-startup enable"; Tasks: autostart; Flags: runhidden waituntilterminated runascurrentuser
Filename: "{app}\robocam.exe"; Parameters: "--configure-startup disable"; Tasks: not autostart; Flags: runhidden waituntilterminated runascurrentuser
Filename: "{app}\robocam.exe"; Description: "Launch robocam"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{app}\robocam.exe"; Parameters: "--configure-startup disable"; RunOnceId: "RemoveStartupTask"; Flags: runhidden waituntilterminated
