; Trame — script d'installation Inno Setup 6
;
; Installation PAR UTILISATEUR (aucun droit administrateur) : tout va sous
; %LOCALAPPDATA%\Programs\Trame, et la desinstallation passe par « Applications ».
; Le profil du navigateur (%LOCALAPPDATA%\Trame) n'est JAMAIS touche : ni l'installation,
; ni la desinstallation ne doivent detruire des onglets, des reglages ou une connexion.

#define AppName "Trame"
; Version : celle de Cargo.toml, passee par faire-setup.mjs (/DAppVersion=…).
#ifndef AppVersion
  #define AppVersion "0.2.0"
#endif
#define AppPublisher "Trame"
#define AppExeName "trame.exe"
; Identite d'application Windows : la meme que `APP_USER_MODEL_ID` dans src/main.rs.
#define AppUserModelId "Trame.Navigateur"

[Setup]
AppId={{8F3C1D42-5B7E-4F1A-9C3D-7E2B4A6C8D01}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\dist
OutputBaseFilename=Trame-{#AppVersion}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\ressources\trame.ico
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName={#AppName}
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
; Trame se declare navigateur (protocoles http/https, fichiers .html) : Windows doit relire les
; associations a la fin de l'installation et de la desinstallation.
ChangesAssociations=yes
RestartApplications=no
AllowNoIcons=yes
ShowLanguageDialog=no
#if FileExists(AddBackslash(SourcePath) + "signature.txt")
; (signature geree hors d'Inno : voir outils/faire-setup.mjs)
#endif

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "Raccourci sur le Bureau"; GroupDescription: "Raccourcis :"
Name: "lancement"; Description: "Lancer {#AppName} a la fin de l'installation"; GroupDescription: "Fin de l'installation :"

[Files]
; L'executable EMBARQUE est celui de `dist\`, c'est-a-dire la copie SIGNEE que
; `faire-setup.mjs` a faite a l'etape 3 — pas `target\release\trame.exe`, que le script relirait
; une seconde fois. Mesure du 2026-11-22 : une compilation concurrente a remplace
; `target\release\trame.exe` entre la signature et Inno Setup, et l'installation a embarque un
; executable NON SIGNE sous un setup signe. En lisant la copie signee, cet intervalle disparait.
Source: "..\dist\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\ui\*"; DestDir: "{app}\ui"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\extensions\*"; DestDir: "{app}\extensions"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\docs\CONCEPT.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "..\docs\ARCHITECTURE.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "..\docs\FORMAT-EXTENSION.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "..\docs\EVOLUTIONS.md"; DestDir: "{app}\docs"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "LISEZMOI-install.txt"; DestDir: "{app}"; DestName: "LISEZMOI.txt"; Flags: ignoreversion isreadme

[Registry]
; --- Trame navigateur : ce qui le fait apparaitre dans Parametres > Applications par defaut.
; Tout est sous HKCU (installation par utilisateur, aucun droit administrateur) et tout part a la
; desinstallation. Choisir Trame comme navigateur par defaut reste un geste de l'utilisateur :
; Windows l'interdit a un programme.
Root: HKCU; Subkey: "Software\Classes\TrameURL"; ValueType: string; ValueName: ""; ValueData: "Page web Trame"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\TrameURL"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\TrameURL\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKCU; Subkey: "Software\Classes\TrameURL\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\TrameHTML"; ValueType: string; ValueName: ""; ValueData: "Document HTML Trame"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\TrameHTML\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKCU; Subkey: "Software\Classes\TrameHTML\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Trame"; ValueType: string; ValueName: ""; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Trame\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#AppExeName},0"
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Trame\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"""
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Trame\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#AppName}"
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Trame\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Navigateur a onglets tisses : fils, veille, coup d'oeil."
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Trame\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\{#AppExeName},0"
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Trame\Capabilities\StartMenu"; ValueType: string; ValueName: "StartMenuInternet"; ValueData: "Trame"
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Trame\Capabilities\URLAssociations"; ValueType: string; ValueName: "http"; ValueData: "TrameURL"
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Trame\Capabilities\URLAssociations"; ValueType: string; ValueName: "https"; ValueData: "TrameURL"
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Trame\Capabilities\FileAssociations"; ValueType: string; ValueName: ".htm"; ValueData: "TrameHTML"
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Trame\Capabilities\FileAssociations"; ValueType: string; ValueName: ".html"; ValueData: "TrameHTML"
Root: HKCU; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Trame"; ValueData: "Software\Clients\StartMenuInternet\Trame\Capabilities"; Flags: uninsdeletevalue
; --- Ce qui manquait pour que Windows PROPOSE Trame (2026-11-22). Avec seulement ce qui precede,
; Trame n'apparaissait ni dans la liste des applications par defaut, ni dans le choix du
; navigateur pour HTTPS (« parcourir le Store » seulement). Compare a Opera GX (installe par
; utilisateur, lui propose) : il s'inscrit dans « Ouvrir avec » des pages .htm/.html, et Windows
; le compte parmi les candidats de http/https/.htm/.html (ApplicationAssociationToasts). Ces
; valeurs ne choisissent rien : elles disent seulement a Windows que Trame sait ouvrir ces liens.
Root: HKCU; Subkey: "Software\Classes\.htm\OpenWithProgids"; ValueType: string; ValueName: "TrameHTML"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\.html\OpenWithProgids"; ValueType: string; ValueName: "TrameHTML"; ValueData: ""; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\Applications\{#AppExeName}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Applications\{#AppExeName}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
Root: HKCU; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".htm"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".html"; ValueData: ""
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\ApplicationAssociationToasts"; ValueType: dword; ValueName: "TrameURL_http"; ValueData: 0; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\ApplicationAssociationToasts"; ValueType: dword; ValueName: "TrameURL_https"; ValueData: 0; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\ApplicationAssociationToasts"; ValueType: dword; ValueName: "TrameHTML_.htm"; ValueData: 0; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\ApplicationAssociationToasts"; ValueType: dword; ValueName: "TrameHTML_.html"; ValueData: 0; Flags: uninsdeletevalue
; --- Identite d'application (AppUserModelID), comme Chrome (`ChromeHTML\Application`) et Opera
; (raccourci) : sans elle, Windows connaissait Trame par le seul chemin de son executable, et les
; Parametres l'ecartaient de « Applications par defaut » et du choix du navigateur, alors que
; l'API des associations le proposait deja (verifie le 2026-11-22). Meme identifiant que celui que
; trame.exe s'attribue au demarrage et que celui du raccourci du menu Demarrer.
Root: HKCU; Subkey: "Software\Classes\TrameURL\Application"; ValueType: string; ValueName: "AppUserModelId"; ValueData: "{#AppUserModelId}"
Root: HKCU; Subkey: "Software\Classes\TrameURL\Application"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#AppName}"
Root: HKCU; Subkey: "Software\Classes\TrameURL\Application"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\{#AppExeName},0"
Root: HKCU; Subkey: "Software\Classes\TrameURL\Application"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Navigateur a onglets tisses"
Root: HKCU; Subkey: "Software\Classes\TrameURL\Application"; ValueType: string; ValueName: "ApplicationCompany"; ValueData: "{#AppPublisher}"
Root: HKCU; Subkey: "Software\Classes\TrameHTML\Application"; ValueType: string; ValueName: "AppUserModelId"; ValueData: "{#AppUserModelId}"
Root: HKCU; Subkey: "Software\Classes\TrameHTML\Application"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#AppName}"
Root: HKCU; Subkey: "Software\Classes\TrameHTML\Application"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\{#AppExeName},0"
Root: HKCU; Subkey: "Software\Classes\TrameHTML\Application"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Navigateur a onglets tisses"
Root: HKCU; Subkey: "Software\Classes\TrameHTML\Application"; ValueType: string; ValueName: "ApplicationCompany"; ValueData: "{#AppPublisher}"

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExeName}"; AppUserModelID: "{#AppUserModelId}"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; AppUserModelID: "{#AppUserModelId}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Lancer {#AppName}"; Flags: nowait postinstall skipifsilent; Tasks: lancement

[UninstallDelete]
Type: filesandordirs; Name: "{app}\ui"
Type: filesandordirs; Name: "{app}\extensions"
