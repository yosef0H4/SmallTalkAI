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
Name: "{userstartup}\SmallTalkAI"; Filename: "{app}\SmallTalkAI.exe"; WorkingDir: "{app}"; IconFilename: "{app}\runtime\icons\smalltalkai.ico"; Tasks: startup

[Run]
Filename: "{app}\SmallTalkAI.exe"; WorkingDir: "{app}"; Description: "Launch SmallTalkAI"; Flags: postinstall nowait skipifsilent
