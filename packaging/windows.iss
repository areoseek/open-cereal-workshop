; Inno Setup script for the Windows installer. Build the program first (cargo build --release -p open-cereal-workshop),
; then run:  iscc packaging\windows.iss      The installer lands in target\.
[Setup]
AppId={{6B1C5E0A-52A7-4C55-9D0B-7B3E2A1F4C90}
AppName=Open Cereal Workshop
AppVersion=0.1.0
AppPublisher=Open Cereal Workshop contributors
DefaultDirName={autopf}\Open Cereal Workshop
DefaultGroupName=Open Cereal Workshop
LicenseFile=..\LICENSE
OutputDir=..\target
OutputBaseFilename=Open-Cereal-Workshop-setup-x86_64
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequiredOverridesAllowed=dialog
ChangesAssociations=yes
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "Put a shortcut on the desktop"; Flags: unchecked
Name: "assoc"; Description: "Open .flp project files with Open Cereal Workshop"; Flags: unchecked

[Files]
Source: "..\target\release\open-cereal-workshop.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{group}\Open Cereal Workshop"; Filename: "{app}\open-cereal-workshop.exe"
Name: "{autodesktop}\Open Cereal Workshop"; Filename: "{app}\open-cereal-workshop.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\.flp\OpenWithProgids"; ValueType: string; ValueName: "OpenCerealWorkshop.Project"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\OpenCerealWorkshop.Project"; ValueType: string; ValueName: ""; ValueData: "Open Cereal Workshop project"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\OpenCerealWorkshop.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\open-cereal-workshop.exe"" ""%1"""; Tasks: assoc

[Run]
Filename: "{app}\open-cereal-workshop.exe"; Description: "Start Open Cereal Workshop"; Flags: nowait postinstall skipifsilent
