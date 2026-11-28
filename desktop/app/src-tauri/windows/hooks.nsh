; What Pinrail's uninstaller does before it removes the app, which Tauri's
; installer runs as the first step of its Uninstall section.
;
; The app puts three things outside its own folder that the uninstaller
; knows nothing of: the terminal host, which runs from a copy in the data
; folder and so does not block the uninstaller; the copy of the CLI at
; %USERPROFILE%\.local\bin\pinrail.exe; and that folder's entry in the
; user's PATH. The app takes them back itself, when started with
; --before-uninstall (src/uninstall.rs), so that what is removed is decided
; by the code that put it there.
;
; None of it happens for an update ($UpdateMode, which Tauri sets from
; /UPDATE): an update must not end a running agent, and the CLI stays.
;
; The app is closed first, with the macro Tauri's own section uses right
; after this hook: a running app would start the host again as soon as it
; stopped. The macro asks before it closes the app, unless the uninstall is
; passive or silent, and ends the uninstall if the answer is no.

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
    ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      DetailPrint "Stopping the terminal host and removing the pinrail command"
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --before-uninstall' $0
      DetailPrint "${MAINBINARYNAME}.exe --before-uninstall: $0"
    ${EndIf}
  ${EndIf}
!macroend
