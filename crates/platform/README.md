# Platform adapters

`ManagedChild`: Windows suspended launch, назначение в Job Object с kill-on-close, resume через documented thread APIs, tree termination и проверка active process count. Unix process group — development/test adapter. `require_local_disk` отклоняет HGFS и Windows network/removable volumes для state/build roots. Без task orchestration/journal/Tauri. Windows Rust cross-check проходит; real Job Object execution проверяет `scripts/validate-windows.ps1` на host.
