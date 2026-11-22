#ifndef SimpleSttVersion
  #error "Pass /DSimpleSttVersion from Cargo.toml through the distribution builder"
#endif
[Setup]
AppId={{D638F724-8CC9-4D12-9E63-BEC9FA0D29E4}
AppName=SmallTalkAI
AppVersion={#SimpleSttVersion}
AppPublisher=SmallTalkAI
DefaultDirName={localappdata}\Programs\SmallTalkAI
DefaultGroupName=SmallTalkAI
DisableProgramGroupPage=yes
DisableDirPage=no
UsePreviousAppDir=yes
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

[InstallDelete]
Type: filesandordirs; Name: "{app}\ahk"
Type: files; Name: "{app}\runtime\simple-stt.exe"
Type: files; Name: "{app}\runtime\simple-stt.ahk"

[Files]
Source: "smalltalkai-portable\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\SmallTalkAI"; Filename: "{app}\SmallTalkAI.exe"; WorkingDir: "{app}"; IconFilename: "{app}\runtime\icons\smalltalkai.ico"
Name: "{autodesktop}\SmallTalkAI"; Filename: "{app}\SmallTalkAI.exe"; WorkingDir: "{app}"; IconFilename: "{app}\runtime\icons\smalltalkai.ico"; Tasks: desktopicon
Name: "{userstartup}\SmallTalkAI"; Filename: "{app}\SmallTalkAI.exe"; WorkingDir: "{app}"; IconFilename: "{app}\runtime\icons\smalltalkai.ico"; Tasks: startup

[Run]
Filename: "{app}\SmallTalkAI.exe"; WorkingDir: "{app}"; Description: "Launch SmallTalkAI"; Flags: postinstall nowait skipifsilent
