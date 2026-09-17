#define AppName "EvoHime"
#ifndef AppVersion
  #define AppVersion "0.0.000000"
#endif
#ifndef SourceDir
  #define SourceDir "bootstrap-source"
#endif
#ifndef UpdateRepository
  #define UpdateRepository "https://github.com/rkfsociety/EvoHime.git"
#endif
#ifndef UpdateBranch
  #define UpdateBranch "main"
#endif

[Setup]
AppId={{B4EA9A84-7F33-4D1A-9C74-1C1B6D8A8A4B}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=EvoHime
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
CloseApplicationsFilter=EvoHime.exe,EvoHimeUpdater.exe

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autodesktop}\EvoHime"; Filename: "{app}\updater\EvoHimeUpdater.exe"; Parameters: "--evohime-updater --install-dir ""{app}"""; WorkingDir: "{app}"; IconFilename: "{app}\updater\resources\evohime-agent.ico"
Name: "{group}\EvoHime"; Filename: "{app}\updater\EvoHimeUpdater.exe"; Parameters: "--evohime-updater --install-dir ""{app}"""; WorkingDir: "{app}"; IconFilename: "{app}\updater\resources\evohime-agent.ico"

[Run]
Filename: "{app}\updater\EvoHimeUpdater.exe"; Parameters: "--evohime-updater --install-dir ""{app}"""; Description: "Запустить установку EvoHime"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{localappdata}\EvoHime\source"
Type: filesandordirs; Name: "{localappdata}\EvoHime\update-staging"
Type: filesandordirs; Name: "{localappdata}\EvoHime\update-state"
Type: files; Name: "{localappdata}\EvoHime\update.json"

[Code]
procedure WriteUpdateConfig();
var
  Directory: String;
  Lines: TArrayOfString;
begin
  Directory := ExpandConstant('{localappdata}\EvoHime');
  if not ForceDirectories(Directory) then
    exit;
  SetArrayLength(Lines, 9);
  Lines[0] := '{';
  Lines[1] := '  "version": 2,';
  Lines[2] := '  "enabled": true,';
  Lines[3] := '  "repositoryUrl": "{#UpdateRepository}",';
  Lines[4] := '  "branch": "{#UpdateBranch}",';
  Lines[5] := '  "launchPolicy": "installer",';
  Lines[6] := '  "checkIntervalMinutes": 30,';
  Lines[7] := '  "moduleManifest": "evohime.components.json"';
  Lines[8] := '}';
  SaveStringsToUTF8FileWithoutBOM(Directory + '\update.json', Lines, False);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    WriteUpdateConfig();
end;
