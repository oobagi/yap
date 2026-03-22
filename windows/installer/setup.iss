; Yap - Inno Setup Installer Script
; https://jrsoftware.org/isinfo.php
;
; Usage:
;   iscc /DAppVersion=1.0.0 setup.iss
;   iscc /DAppVersion=1.0.0 /DSignTool="signtool" setup.iss
;
; The build output (from build.ps1) should be in ..\build\

#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif

#define AppName "Yap"
#define AppPublisher "Yap"
#define AppExeName "Yap.exe"
#define AppURL "https://github.com/oobagi/yap"
#define BuildDir "..\build"

[Setup]
AppId={{B8F3A2D1-7E4C-4F5A-9D6B-2C8E1F0A3D5E}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} v{#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}/issues
AppUpdatesURL={#AppURL}/releases
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
; License file — uses the repo LICENSE if it exists
LicenseFile=..\..\LICENSE
OutputDir=output
OutputBaseFilename=Yap-v{#AppVersion}-Setup
; Compression settings
Compression=lzma2/ultra64
SolidCompression=yes
; UI settings
WizardStyle=modern
WizardSizePercent=100
SetupIconFile=compiler:SetupClassicIcon.ico
; Uncomment once a custom icon is available:
; SetupIconFile=..\Yap\Resources\Icons\yap.ico
; Uncomment once banner images are created (164x314 and 55x58 pixels):
; WizardImageFile=assets\wizard-large.bmp
; WizardSmallImageFile=assets\wizard-small.bmp
; Require Windows 10+
MinVersion=10.0.19041
; Architecture
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Privileges — install for current user by default, allow elevation
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
; Uninstaller
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName={#AppName}
; Version info embedded in the setup exe
VersionInfoVersion={#AppVersion}
VersionInfoDescription={#AppName} Setup
VersionInfoProductName={#AppName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "startupentry"; Description: "Start {#AppName} when Windows starts"; GroupDescription: "Windows Startup:"; Flags: unchecked

[Files]
; Main application files from the build output
Source: "{#BuildDir}\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#BuildDir}\*.dll"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#BuildDir}\*.json"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#BuildDir}\*.pdb"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
; Sound assets
Source: "{#BuildDir}\Sounds\*"; DestDir: "{app}\Sounds"; Flags: ignoreversion skipifsourcedoesntexist recursesubdirs

[Icons]
; Start Menu shortcut
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
; Desktop shortcut (optional task)
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Registry]
; Windows startup entry (optional task)
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#AppName}"; ValueData: """{app}\{#AppExeName}"""; Flags: uninsdeletevalue; Tasks: startupentry

[Run]
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Clean up config directory on uninstall (user can choose)
Type: dirifempty; Name: "{userappdata}\yap"

[Code]
// Show a message about permissions after installation
procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssDone then
  begin
    // No special post-install steps needed currently
  end;
end;
