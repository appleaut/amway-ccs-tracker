; Inno Setup script for Amway CCS Tracker (per-user, no admin).
; Build: run ..\build_installer.ps1 (or: iscc amway_ccs_tracker.iss)

#define MyAppName "Amway CCS Tracker"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "Amway CCS Tracker"
#define MyAppExeName "amway_ccs_tracker.exe"

[Setup]
AppId={{A1C7F3E2-9B4D-4E6A-8C21-3F5D7E9A1B2C}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={localappdata}\Programs\AmwayCCSTracker
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=AmwayCCSTracker-Setup
SetupIconFile=..\assets\icons\app.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
InfoBeforeFile=prerequisites.txt
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "..\target\release\amway_ccs_tracker.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\icons\app.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\app.ico"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\app.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent
