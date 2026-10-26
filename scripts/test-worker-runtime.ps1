param([Parameter(Mandatory)][string]$WorkDirectory,[Parameter(Mandatory)][string]$Fixture)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
. (Join-Path $PSScriptRoot 'lib/test-worker-runtime.ps1')
Assert-SessionDirectory $WorkDirectory
if(Test-Path -LiteralPath $WorkDirectory){throw 'New policy-test directory required'}
[IO.Directory]::CreateDirectory($WorkDirectory)|Out-Null
$installed=Join-Path $WorkDirectory 'production/winbridge-worker.exe'
$root=Join-Path $WorkDirectory 'test-runtime'
$sha=(Get-FileHash -LiteralPath $Fixture).Hash.ToLowerInvariant()
$checks=0;$slot=$null;$child=$null
function Refuses([scriptblock]$Action){
 $refused=$false;try{& $Action}catch{$refused=$true}
 if(-not $refused){throw 'Unsafe test runtime operation accepted'}
}
try{
 Refuses {Open-TestWorkerRuntime -Directory (Split-Path $installed -Parent) -InstalledWorker $installed -Image $Fixture -Sha256 $sha};$checks++
 $unknown=Join-Path $WorkDirectory 'unknown';[IO.Directory]::CreateDirectory($unknown)|Out-Null
 [IO.File]::WriteAllText((Join-Path $unknown 'manual.txt'),'preserve')
 Refuses {Open-TestWorkerRuntime $unknown $installed $Fixture $sha}
 if([IO.File]::ReadAllText((Join-Path $unknown 'manual.txt')) -cne 'preserve'){throw 'Manual data changed'};$checks++
 $slot=Open-TestWorkerRuntime $root $installed $Fixture $sha
 $path=$slot.executable
 Refuses {Open-TestWorkerRuntime $root $installed $Fixture $sha};$checks++
 Refuses {$f=[IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::Write);$f.Dispose()};$checks++
 Close-TestWorkerRuntime $slot;$slot=$null
 Refuses {Open-TestWorkerRuntime $root $installed $Fixture ('0'*64)}
 if((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $sha){throw 'Bad input changed prior binary'};$checks++
 $si=[Diagnostics.ProcessStartInfo]::new($path);$si.UseShellExecute=$false;$si.CreateNoWindow=$true
 $si.ArgumentList.Add('long')
 $child=[Diagnostics.Process]::Start($si)
 Refuses {Open-TestWorkerRuntime $root $installed $Fixture $sha};$checks++
 $child.Kill();if(-not $child.WaitForExit(15000)){throw 'Policy child exit unconfirmed'};$child.Dispose();$child=$null
 $slot=Open-TestWorkerRuntime $root $installed $Fixture $sha
 if($slot.executable -cne $path -or $slot.sha256 -cne $sha){throw 'Test path did not stay fixed'}
 Write-Output ('TEST_RUNTIME_POLICY '+(@{passed=$true;checksPassed=$checks;stablePath=$true}|ConvertTo-Json -Compress))
}finally{
 if($null -ne $child){if(-not $child.HasExited){$child.Kill()};[void]$child.WaitForExit(15000);$child.Dispose()}
 Close-TestWorkerRuntime $slot
}
