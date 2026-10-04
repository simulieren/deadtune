; DL-FOV-Fixer per-user installer (Inno Setup 6).
; Built by build-installer.ps1, which passes /DAppVersion, /DPublishDir and /DOutputDir.
; See docs/adr/0002-a-per-user-installer-replaces-the-portable-exe.md.

#ifndef AppVersion
  #error AppVersion must be supplied, for example /DAppVersion=2.0.0
#endif
#ifndef PublishDir
  #error PublishDir must be supplied, for example /DPublishDir=C:\path\to\publish
#endif
#ifndef OutputDir
  #error OutputDir must be supplied, for example /DOutputDir=C:\path\to\out
#endif

#define MyAppName "DL-FOV-Fixer"
#define MyAppPublisher "Lukáš Krejčí"
#define MyAppExeName "DL-FOV-Fixer.exe"
; The app owns this value and writes it from its own "start with Windows" setting.
#define MyRunValue "DL-FOV-Fixer"

[Setup]
; NEVER CHANGE THIS GUID. It is how Windows and every later installer recognize an existing
; install, so changing it would leave the old version behind as a second copy.
AppId={{93ED9F26-293C-4790-BAEE-C7C1D1B8F741}
AppName={#MyAppName}
AppVersion={#AppVersion}
AppVerName={#MyAppName} {#AppVersion}
AppPublisher={#MyAppPublisher}
; Version 1.0 may already have dl-fov-fixer.exe in this folder. Windows file names ignore case, so
; that is the same file as DL-FOV-Fixer.exe and the install replaces it. That is intended: a Run
; value 1.0 left behind then starts 2.0, and CloseApplications closes a running 1.0 first.
DefaultDirName={localappdata}\Programs\DL-FOV-Fixer
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=DL-FOV-Fixer-{#AppVersion}-setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
; The in-app updater starts this with /SILENT /UPDATE. The Restart Manager closes the running
; tray app first, and the [Run] section below starts it again.
CloseApplications=yes
CloseApplicationsFilter={#MyAppExeName}
RestartApplications=no
VersionInfoVersion={#AppVersion}.0
VersionInfoCompany={#MyAppPublisher}
VersionInfoProductName={#MyAppName}
LicenseFile=..\LICENSE.md

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "{#PublishDir}\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"

[Run]
; A normal install offers a checkbox on the last page.
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent
; After an in-place update the app starts the installer with /SILENT /UPDATE, so bring the app back.
Filename: "{app}\{#MyAppExeName}"; Flags: nowait; Check: IsSilentUpdate

[UninstallRun]
; Setup closes the running app through the Restart Manager (CloseApplications), but uninstall
; does not, so the app's exe and DLLs stayed behind, in use, with the app still in the tray.
; Stop only the copy that runs from this install folder, so a dev build elsewhere is left alone,
; and wait for it to exit before files are removed. Inno Setup turns {{ into {, but leaves } alone.
Filename: "{sys}\WindowsPowerShell\v1.0\powershell.exe"; Parameters: "-NoProfile -NonInteractive -ExecutionPolicy Bypass -Command ""$p = @(Get-Process -Name 'DL-FOV-Fixer' -ErrorAction SilentlyContinue | Where-Object {{ $_.Path -eq '{app}\{#MyAppExeName}' }); $p | Stop-Process -Force; $p | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue"""; Flags: runhidden waituntilterminated; RunOnceId: "StopRunningApp"

; User settings in %APPDATA%\DL-FOV-Fixer are never removed by the uninstaller, so a reinstall keeps
; the chosen FOV. The Run value is handled in the [Code] section because the installer does not
; create it.

[Code]
function HasCommandLineSwitch(const Name: String): Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), Name) = 0 then
    begin
      Result := True;
      Exit;
    end;
end;

function IsSilentUpdate(): Boolean;
begin
  Result := WizardSilent() and HasCommandLineSwitch('/UPDATE');
end;

// Remove the app's own "start with Windows" value, but only when it points at the installed exe.
// A value that points somewhere else (a 1.0 portable copy, for example) is not ours to delete.
procedure RemoveRunValueIfItPointsHere();
var
  RunKey: String;
  Current: String;
  InstalledExe: String;
begin
  RunKey := 'Software\Microsoft\Windows\CurrentVersion\Run';
  if not RegQueryStringValue(HKCU, RunKey,'{#MyRunValue}', Current) then
    Exit;
  InstalledExe := ExpandConstant('{app}\{#MyAppExeName}');
  if Pos(Lowercase(InstalledExe), Lowercase(Current)) > 0 then
    RegDeleteValue(HKCU, RunKey, '{#MyRunValue}');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    RemoveRunValueIfItPointsHere();
end;
