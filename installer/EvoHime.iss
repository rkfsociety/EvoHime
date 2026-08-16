#define AppName "EvoHime"
#ifndef AppVersion
  #define AppVersion "0.0.000033"
#endif
#define AppPublisher "EvoHime"
#define AppExeName "EvoHime.exe"
#ifndef SourceDir
  #define SourceDir "native-package"
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

[Tasks]
; Установщик CI уже собран и проверен на зелёном коммите. Клиент скачивает
; этот установщик из постоянного GitHub Release и применяет его в фоне.
Name: "autoupdate"; Description: "Обновлять автоматически из GitHub Release"; GroupDescription: "Обновления"

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autodesktop}\EvoHime"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\resources\evohime-agent.ico"

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Запустить EvoHime"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Рабочая копия исходников и собранный пакет принадлежат обновлению, а не
; пользователю: данные, база и логи в %LOCALAPPDATA%\EvoHime остаются.
Type: filesandordirs; Name: "{localappdata}\EvoHime\source"
Type: filesandordirs; Name: "{localappdata}\EvoHime\update-staging"
Type: filesandordirs; Name: "{localappdata}\EvoHime\update-state"
Type: files; Name: "{localappdata}\EvoHime\update.json"

[Code]
{ Конфигурация обновления читается клиентом при запуске.                    }
{ Значения пишутся установщиком, чтобы репозиторий и ветка не были зашиты в }
{ бинарник и переустановка могла их изменить.                               }
procedure WriteUpdateConfig();
var
  Directory: String;
  Lines: TArrayOfString;
  Enabled: String;
begin
  Directory := ExpandConstant('{localappdata}\EvoHime');
  if not ForceDirectories(Directory) then
    exit;

  if IsTaskSelected('autoupdate') then
    Enabled := 'true'
  else
    Enabled := 'false';

  SetArrayLength(Lines, 10);
  Lines[0] := '{';
  Lines[1] := '  "version": 2,';
  Lines[2] := '  "enabled": ' + Enabled + ',';
  Lines[3] := '  "repositoryUrl": "{#UpdateRepository}",';
  Lines[4] := '  "branch": "{#UpdateBranch}",';
  Lines[5] := '  "launchPolicy": "installer",';
  Lines[6] := '  "checkIntervalMinutes": 30,';
  { Пересборка идёт на машине пользователя, поэтому красный коммит }
  { собирать нельзя: клиент ждёт зелёной сборки.                   }
  Lines[7] := '  "requireGreenCommit": true,';
  Lines[8] := '  "greenCommitDepth": 10';
  Lines[9] := '}';
  SaveStringsToUTF8File(Directory + '\update.json', Lines, False);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    WriteUpdateConfig();
end;
