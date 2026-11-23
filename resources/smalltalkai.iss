#ifndef SmallTalkAIVersion
  #error "Pass /DSmallTalkAIVersion from Cargo.toml through the distribution builder"
#endif
[Setup]
AppId={{E0408A3D-8B8B-42ED-B9E2-65D5C1F4282C}
AppName=SmallTalkAI
AppVersion={#SmallTalkAIVersion}
AppPublisher=SmallTalkAI
DefaultDirName={localappdata}\Programs\SmallTalkAI
DefaultGroupName=SmallTalkAI
DisableProgramGroupPage=yes
DisableDirPage=no
UsePreviousAppDir=no
UsePreviousGroup=no
OutputDir=dist
OutputBaseFilename=smalltalkai-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
UninstallDisplayName=SmallTalkAI
UninstallDisplayIcon={app}\runtime\icons\smalltalkai.ico
SetupIconFile=smalltalkai-portable\runtime\icons\smalltalkai.ico
LicenseFile=smalltalkai-portable\LICENSE

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked
Name: "startup"; Description: "Launch SmallTalkAI when I sign in"; GroupDescription: "Startup options:"; Flags: unchecked

[Files]
Source: "smalltalkai-portable\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\SmallTalkAI"; Filename: "{app}\SmallTalkAI.exe"; WorkingDir: "{app}"; IconFilename: "{app}\runtime\icons\smalltalkai.ico"
Name: "{autodesktop}\SmallTalkAI"; Filename: "{app}\SmallTalkAI.exe"; WorkingDir: "{app}"; IconFilename: "{app}\runtime\icons\smalltalkai.ico"; Tasks: desktopicon
Name: "{userstartup}\SmallTalkAI"; Filename: "{app}\SmallTalkAI.exe"; Parameters: "--background"; WorkingDir: "{app}"; IconFilename: "{app}\runtime\icons\smalltalkai.ico"; Tasks: startup; BeforeInstall: SaveStartupPreference

[Run]
Filename: "{app}\SmallTalkAI.exe"; WorkingDir: "{app}"; Description: "Launch SmallTalkAI"; Flags: postinstall nowait skipifsilent

[Code]
procedure SaveStartupPreference;
var
  ResultCode: Integer;
begin
  if not Exec(ExpandConstant('{app}\runtime\smalltalkai-ctl.exe'),
      'config set general.start_at_login true', ExpandConstant('{app}\runtime'),
      SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    RaiseException('Could not start the SmallTalkAI configuration tool: ' + SysErrorMessage(ResultCode));
  if ResultCode <> 0 then
    RaiseException('Could not save Start at login (exit code ' + IntToStr(ResultCode) + ').');
end;
