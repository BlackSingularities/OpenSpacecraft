; OpenSpacecraft – instalator Windows (NSIS 3, Unicode). Plik zapisany jako UTF-8 z BOM.
; makensis /DVERSION=1.0.0 /DEXE=..\target\release\OpenSpacecraft.exe /DOUT=..\release\OpenSpacecraft-Setup-1.0.0.exe build\openspacecraft.nsi
;
; Instalacja dla wszystkich użytkowników (Program Files, okno UAC). Tryb cichy: /S; po instalacji
; w trybie cichym program uruchamia się z opcją --force-run (tak wywołuje go aktualizator).

Unicode true
ManifestDPIAware true
SetCompressor /SOLID lzma
RequestExecutionLevel admin

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef EXE
  !define EXE "..\target\release\OpenSpacecraft.exe"
!endif
!ifndef OUT
  !define OUT "..\release\OpenSpacecraft-Setup-${VERSION}.exe"
!endif

!define NAME "OpenSpacecraft"
!define UNKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenSpacecraft"

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"

Name "${NAME}"
OutFile "${OUT}"
InstallDir "$PROGRAMFILES64\${NAME}"
InstallDirRegKey HKLM "${UNKEY}" "InstallLocation"
BrandingText "OpenSpacecraft ${VERSION} · redzikowski.com"

VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=1045 "ProductName" "${NAME}"
VIAddVersionKey /LANG=1045 "FileDescription" "OpenSpacecraft – atlas misji kosmicznych 3D (instalator)"
VIAddVersionKey /LANG=1045 "CompanyName" "BlackSingularities"
VIAddVersionKey /LANG=1045 "LegalCopyright" "© 2026 Adam Rędzikowski"
VIAddVersionKey /LANG=1045 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=1045 "ProductVersion" "${VERSION}"

!define MUI_ICON "icon.ico"
!define MUI_UNICON "icon.ico"
!define MUI_WELCOMEFINISHPAGE_BITMAP "installerSidebar.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "uninstallerSidebar.bmp"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "Uruchom OpenSpacecraft"
!define MUI_FINISHPAGE_RUN_FUNCTION RunApp

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Polish"

; uruchomienie bez uprawnień administratora (przez Eksploratora, jako zwykły użytkownik)
Function RunApp
  Exec '"$WINDIR\explorer.exe" "$INSTDIR\OpenSpacecraft.exe"'
FunctionEnd

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "OpenSpacecraft wymaga 64-bitowego systemu Windows 10 lub 11." /SD IDOK
    Abort
  ${EndIf}
  SetRegView 64
  ReadRegStr $0 HKLM "${UNKEY}" "InstallLocation"
  ${If} $0 != ""
    StrCpy $INSTDIR $0
  ${EndIf}
FunctionEnd

; program musi być zamknięty, zanim podmienimy plik
!macro CloseApp
  StrCpy $1 0
  ${Do}
    ClearErrors
    Delete "$INSTDIR\OpenSpacecraft.exe"
    ${IfNot} ${Errors}
      ${Break}
    ${EndIf}
    IntOp $1 $1 + 1
    ${If} $1 = 12
      nsExec::Exec 'taskkill /F /IM OpenSpacecraft.exe'
      Sleep 800
    ${EndIf}
    ${If} $1 > 20
      MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "Zamknij program OpenSpacecraft, żeby kontynuować." /SD IDCANCEL IDRETRY osc_retry
      Abort
      osc_retry:
      StrCpy $1 0
    ${EndIf}
    Sleep 500
  ${Loop}
!macroend

Section "OpenSpacecraft" SecMain
  SetShellVarContext all
  SetRegView 64
  SetOutPath "$INSTDIR"
  ${If} ${FileExists} "$INSTDIR\OpenSpacecraft.exe"
    !insertmacro CloseApp
  ${EndIf}

  File "/oname=OpenSpacecraft.exe" "${EXE}"
  WriteUninstaller "$INSTDIR\Odinstaluj.exe"

  CreateShortCut "$SMPROGRAMS\${NAME}.lnk" "$INSTDIR\OpenSpacecraft.exe" "" "$INSTDIR\OpenSpacecraft.exe" 0
  CreateShortCut "$DESKTOP\${NAME}.lnk" "$INSTDIR\OpenSpacecraft.exe" "" "$INSTDIR\OpenSpacecraft.exe" 0

  WriteRegStr HKLM "${UNKEY}" "DisplayName" "OpenSpacecraft – atlas misji kosmicznych 3D"
  WriteRegStr HKLM "${UNKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNKEY}" "DisplayIcon" "$INSTDIR\OpenSpacecraft.exe"
  WriteRegStr HKLM "${UNKEY}" "Publisher" "BlackSingularities"
  WriteRegStr HKLM "${UNKEY}" "URLInfoAbout" "https://redzikowski.com/portfolio/openspacecraft"
  WriteRegStr HKLM "${UNKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNKEY}" "UninstallString" '"$INSTDIR\Odinstaluj.exe"'
  WriteRegStr HKLM "${UNKEY}" "QuietUninstallString" '"$INSTDIR\Odinstaluj.exe" /S'
  WriteRegDWORD HKLM "${UNKEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNKEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${UNKEY}" "EstimatedSize" "$0"
SectionEnd

Function .onInstSuccess
  ${If} ${Silent}
    ${GetParameters} $R0
    ClearErrors
    ${GetOptions} $R0 "--force-run" $R1
    ${IfNot} ${Errors}
      Call RunApp
    ${EndIf}
  ${EndIf}
FunctionEnd

Section "Uninstall"
  SetShellVarContext all
  SetRegView 64
  nsExec::Exec 'taskkill /F /IM OpenSpacecraft.exe'
  Delete "$INSTDIR\OpenSpacecraft.exe"
  Delete "$INSTDIR\Odinstaluj.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${NAME}.lnk"
  Delete "$DESKTOP\${NAME}.lnk"
  DeleteRegKey HKLM "${UNKEY}"

  ; przy aktualizacji (--updated) albo w trybie cichym dane użytkownika zostają nietknięte
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "--updated" $R1
  ${If} ${Errors}
  ${AndIfNot} ${Silent}
    MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "Usunąć także ustawienia i pobrany katalog satelitów?$\r$\n$\r$\nWybierz „Nie”, aby zachować je na wypadek ponownej instalacji." IDNO keep
      SetShellVarContext current
      RMDir /r "$LOCALAPPDATA\OpenSpacecraft"
      RMDir /r "$APPDATA\OpenSpacecraft"
      SetShellVarContext all
    keep:
  ${EndIf}
SectionEnd
