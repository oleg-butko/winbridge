# Compiler options for private native fixtures. Public release keeps its original arguments.
function Get-UpdateDesktopBuildArguments {
    param(
        [Parameter(Mandatory)][string]$TargetDirectory,
        [switch]$NativeValidation
    )
    $arguments=@()
    if ($NativeValidation) {
        foreach ($package in @('winbridge-desktop','winbridge-core','winbridge-protocol','winbridge-platform','winbridge-transport')) {
            $arguments+=@('--config',"profile.release.package.$package.opt-level=0")
        }
    }
    return $arguments+@('build','--manifest-path','apps/desktop/src-tauri/Cargo.toml','--features','custom-protocol',
        '--release','--locked','--target-dir',$TargetDirectory)
}
