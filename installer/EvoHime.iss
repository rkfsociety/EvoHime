#define AppName "EvoHime"
#ifndef AppVersion
  #define AppVersion "0.0.000030"
#endif
#define AppPublisher "EvoHime"
#define AppExeName "EvoHime.exe"
#ifndef SourceDir
  #define SourceDir "native-package"
#endif

[Setup]
AppId={{B4EA9A84-7F33-4D1A-9C74-1C1B6D8A8A4B}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={localappdata}\Programs\EvoHime
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=installer-output
OutputBaseFilename=EvoHime-Setup
Compression=lzma2
SolidCompression=yes
PrivilegesRequired=lowest
UninstallDisplayName={#AppName}
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
CloseApplicationsFilter=EvoHime.exe

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autodesktop}\EvoHime"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Запустить EvoHime"; Flags: nowait postinstall skipifsilent
