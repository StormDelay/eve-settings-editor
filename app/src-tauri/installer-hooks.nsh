; Tauri includes this near the top of its NSIS template, before any page is
; declared, so the defines below apply to the pages that follow. Tauri's own
; defines (PRODUCTNAME, UNINSTKEY...) come after it, hence the literal key.

; Desktop shortcut checkbox on the finish page starts unchecked.
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED

; Upgrades: leaving the welcome page with a previous install present jumps
; past the "uninstall before installing?" page and the folder page (the folder
; is restored from the previous install) straight to installing over it, the
; way Tauri's updater does. Installing over also keeps the user's shortcuts.
; Page order: welcome, reinstall, directory, start menu (skipped), install.
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE SkipUpgradePages
Function SkipUpgradePages
  ReadRegStr $0 SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\EVE Settings Editor" "UninstallString"
  ${If} $0 != ""
    SendMessage $HWNDPARENT 0x408 3 ""
    Abort
  ${EndIf}
FunctionEnd
