; Offline, per-user Windows installer. Only the EXE and the consolidated legal file.
#ifndef AppVersion
  #error AppVersion must be supplied by package-setup.ps1
#endif
[Setup]
AppId={{8BA91B8A-11AC-44EB-A6C9-889DAA532D57}
AppName=PDFinator
AppVersion={#AppVersion}
AppPublisher=Ergonatus
AppCopyright=Ergonatus
DefaultDirName={localappdata}\Programs\Ergonatus\PDFinator
DefaultGroupName=Ergonatus
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.10240
OutputDir=..\dist
OutputBaseFilename=PDFinator-{#AppVersion}-Setup
SetupIconFile=..\assets\pdfinator.ico
UninstallDisplayIcon={app}\PDFinator.exe
UninstallDisplayName=PDFinator
CreateUninstallRegKey=not IsSmokeTest
WizardStyle=modern
DisableProgramGroupPage=yes
DisableWelcomePage=no
Compression=lzma2/max
SolidCompression=yes
CloseApplications=no
RestartApplications=no
SetupLogging=yes
VersionInfoVersion={#AppVersion}
VersionInfoDescription=Installation de PDFinator

[Languages]
Name: french; MessagesFile: compiler:Languages\French.isl

[Tasks]
Name: desktopicon; Description: Créer un raccourci sur le Bureau; Flags: unchecked; Check: not IsSmokeTest

[Files]
Source: ..\runtime\pdfinator.exe; DestDir: {app}; DestName: PDFinator.exe; Flags: ignoreversion
Source: ..\licenses\LICENCES.txt; DestDir: {app}; Flags: ignoreversion

[Icons]
Name: {userprograms}\Ergonatus\PDFinator; Filename: {app}\PDFinator.exe; IconFilename: {app}\PDFinator.exe; IconIndex: 0; WorkingDir: {app}; Check: not IsSmokeTest
Name: {userdesktop}\PDFinator; Filename: {app}\PDFinator.exe; IconFilename: {app}\PDFinator.exe; IconIndex: 0; WorkingDir: {app}; Tasks: desktopicon; Check: not IsSmokeTest

[Run]
Filename: {app}\PDFinator.exe; Description: Ouvrir PDFinator; Flags: nowait postinstall skipifsilent

[Code]
function IsSmokeTest: Boolean;
begin
  Result := ExpandConstant('{param:TESTINSTALL|0}') = '1';
end;
