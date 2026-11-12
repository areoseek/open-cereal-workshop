; Inno Setup script for the Windows installer. Build the program first (cargo build --release -p breakfast-cereal),
; then run:  iscc packaging\windows.iss      The installer lands in target\.
[Setup]
AppId={{6B1C5E0A-52A7-4C55-9D0B-7B3E2A1F4C90}
AppName=Breakfast Cereal DAW
AppVersion=0.1.0
AppPublisher=Breakfast Cereal DAW contributors
DefaultDirName={autopf}\Breakfast Cereal DAW
DefaultGroupName=Breakfast Cereal DAW
LicenseFile=..\LICENSE
OutputDir=..\target
OutputBaseFilename=Breakfast-Cereal-DAW-setup-x86_64
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequiredOverridesAllowed=dialog
ChangesAssociations=yes
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "Put a shortcut on the desktop"; Flags: unchecked
Name: "assoc"; Description: "Open .flp project files with Breakfast Cereal DAW"; Flags: unchecked

[Files]
Source: "..\target\release\breakfast-cereal.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\docs\*"; DestDir: "{app}\docs"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{group}\Breakfast Cereal DAW"; Filename: "{app}\breakfast-cereal.exe"
Name: "{autodesktop}\Breakfast Cereal DAW"; Filename: "{app}\breakfast-cereal.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\.flp\OpenWithProgids"; ValueType: string; ValueName: "BreakfastCereal.Project"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\BreakfastCereal.Project"; ValueType: string; ValueName: ""; ValueData: "Breakfast Cereal DAW project"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\BreakfastCereal.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\breakfast-cereal.exe"" ""%1"""; Tasks: assoc

[Run]
Filename: "{app}\breakfast-cereal.exe"; Description: "Start Breakfast Cereal DAW"; Flags: nowait postinstall skipifsilent
