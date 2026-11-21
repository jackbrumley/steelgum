#ifndef SteelgumVersion
  #error SteelgumVersion is required
#endif

#ifndef OutputBase
  #error OutputBase is required
#endif

#ifndef SteelgumIcon
  #error SteelgumIcon is required
#endif

#ifndef SteelgumBinary
  #error SteelgumBinary is required
#endif

[Setup]
AppId={{CA30D8B3-ACB0-4951-B503-2F91D6DD3BD4}
AppName=Steelgum
AppVersion={#SteelgumVersion}
AppPublisher=Jack Brumley
UninstallDisplayName=Steelgum
UninstallDisplayIcon={app}\steelgum.ico
SetupIconFile={#SteelgumIcon}
DefaultDirName={localappdata}\Programs\Steelgum
DefaultGroupName=Steelgum
DisableDirPage=yes
UsePreviousAppDir=no
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
CloseApplications=no
RestartApplications=no
OutputDir=..\..\release-artifacts
OutputBaseFilename={#OutputBase}
Compression=lzma2
SolidCompression=yes

[Files]
Source: "{#SteelgumBinary}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SteelgumIcon}"; DestDir: "{app}"

[Icons]
Name: "{autoprograms}\Steelgum"; Filename: "{app}\steelgum.exe"; IconFilename: "{app}\steelgum.ico"
