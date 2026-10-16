param([string]$ProbePath = 'src-tauri/target/debug/examples/single_instance_probe.exe')
$ErrorActionPreference = 'Stop'
$probeExecutable = (Resolve-Path -LiteralPath $ProbePath).Path
$probeRoot = Join-Path (Resolve-Path -LiteralPath 'src-tauri/target').Path ('single-instance-test-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $probeRoot | Out-Null
$savedProbeDir = $env:THEARCHIVE_PROBE_DIR
$savedProbeId = $env:THEARCHIVE_PROBE_ID
$env:THEARCHIVE_PROBE_DIR = $probeRoot
$env:THEARCHIVE_PROBE_ID = 'com.thearchive.probe.' + [Guid]::NewGuid().ToString('N')
$probeChildren = [Collections.Generic.List[Diagnostics.Process]]::new()
$sequence = 0
function Start-Probe([int]$DelayMilliseconds = 0) {
    $savedProbeDelay = $env:THEARCHIVE_PROBE_DELAY_MS
    $env:THEARCHIVE_PROBE_DELAY_MS = [string]$DelayMilliseconds
    try {
        $process = Start-Process -FilePath $probeExecutable -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $probeRoot ("stderr-$($probeChildren.Count).log"))
    } finally { $env:THEARCHIVE_PROBE_DELAY_MS = $savedProbeDelay }
    $probeChildren.Add($process)
    return $process
}
function Wait-Ready([Diagnostics.Process]$Process) {
    Wait-Condition {
        $Process.Refresh()
        if ($Process.HasExited) { throw "Native probe exited before setup: $($Process.ExitCode); logs: $probeRoot" }
        Test-Path -LiteralPath (Join-Path $probeRoot 'ready')
    } 'First instance did not initialize'
}
function Clear-Signals {
    foreach ($signal in @('ready', 'locked', 'control', 'state.json')) {
        $path = Join-Path $probeRoot $signal
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
    }
}
function Wait-Condition([scriptblock]$Condition, [string]$Message) {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (-not (& $Condition)) {
        if ([DateTime]::UtcNow -gt $deadline) { throw $Message }
        Start-Sleep -Milliseconds 30
    }
}
function Control([string]$Action) {
    $script:sequence++
    [IO.File]::WriteAllText((Join-Path $probeRoot 'control'), "$sequence`:$Action")
    Wait-Condition { try { (Test-Path -LiteralPath (Join-Path $probeRoot 'state.json')) -and ((Get-Content -LiteralPath (Join-Path $probeRoot 'state.json') -Raw | ConvertFrom-Json).sequence -eq [string]$sequence) } catch { $false } } "Probe did not acknowledge $Action"
    return Get-Content -LiteralPath (Join-Path $probeRoot 'state.json') -Raw | ConvertFrom-Json
}
function Repeat-Launch {
    $duplicate = Start-Probe
    Wait-Condition { $duplicate.Refresh(); $duplicate.HasExited } 'Repeated launch did not exit'
    if ($duplicate.ExitCode -ne 0) { throw 'Repeated launch exited with an error' }
    Wait-Condition { $state = Control 'status'; $state.visible -and -not $state.minimized } 'Existing hidden/minimized window was not restored'
}
try {
    $primary = Start-Probe 1500
    Wait-Condition { $primary.Refresh(); if ($primary.HasExited) { throw "Native probe failed: $($primary.ExitCode); logs: $probeRoot" }; Test-Path -LiteralPath (Join-Path $probeRoot 'locked') } 'First launch did not acquire startup mutex'
    $duringStartup = Start-Probe
    Start-Sleep -Milliseconds 250
    $duringStartup.Refresh()
    if ($duringStartup.HasExited -or (Test-Path -LiteralPath (Join-Path $probeRoot 'starts.log'))) { throw 'Second launch did not wait for first plugin initialization' }
    Wait-Ready $primary
    Wait-Condition { $duringStartup.Refresh(); $duringStartup.HasExited } 'Concurrent startup did not resolve'
    if ($duringStartup.ExitCode -ne 0) { throw 'Concurrent launch failed' }
    $primaryId = $primary.Id
    Control 'hide' | Out-Null
    Repeat-Launch
    Control 'minimize' | Out-Null
    Repeat-Launch
    Control 'hide' | Out-Null
    $burst = @(1..4 | ForEach-Object { Start-Probe })
    foreach ($duplicate in $burst) {
        Wait-Condition { $duplicate.Refresh(); $duplicate.HasExited } 'Burst launch did not exit'
        if ($duplicate.ExitCode -ne 0) { throw 'Burst launch failed' }
    }
    $state = Control 'status'
    if (-not $state.visible -or $state.minimized -or $state.pid -ne $primaryId) { throw 'Burst changed the primary process/window' }
    if (@(Get-Content -LiteralPath (Join-Path $probeRoot 'starts.log')).Count -ne 1) { throw 'Duplicate instance reached application setup' }
    Control 'exit' | Out-Null
    Wait-Condition { $primary.Refresh(); $primary.HasExited } 'Graceful exit did not finish'
    Clear-Signals
    $restarted = Start-Probe
    Wait-Ready $restarted
    Stop-Process -Id $restarted.Id
    Wait-Condition { $restarted.Refresh(); $restarted.HasExited } 'Probe termination did not finish'
    Clear-Signals
    $afterCrash = Start-Probe
    Wait-Ready $afterCrash
    if (@(Get-Content -LiteralPath (Join-Path $probeRoot 'starts.log')).Count -ne 3) { throw 'Unexpected setup count' }
    Control 'exit' | Out-Null
    Wait-Condition { $afterCrash.Refresh(); $afterCrash.HasExited } 'Final probe exit failed'
    Clear-Signals
    $interruptedStartup = Start-Probe 5000
    Wait-Condition { Test-Path -LiteralPath (Join-Path $probeRoot 'locked') } 'Interrupted startup did not acquire mutex'
    $waitingStartup = Start-Probe
    Start-Sleep -Milliseconds 150
    $waitingStartup.Refresh()
    if ($waitingStartup.HasExited) { throw 'Waiting launch did not hold at startup mutex' }
    Stop-Process -Id $interruptedStartup.Id
    Wait-Ready $waitingStartup
    if (@(Get-Content -LiteralPath (Join-Path $probeRoot 'starts.log')).Count -ne 4) { throw 'Abandoned mutex failed to recover' }
    Control 'exit' | Out-Null
    Wait-Condition { $waitingStartup.Refresh(); $waitingStartup.HasExited } 'Recovered probe did not exit'
    Write-Output 'Native single-instance passed: simultaneous first startup, hidden/minimized restore, four-launch burst, duplicate exits before setup, graceful/crash restart and abandoned startup mutex recovery. No application data was opened.'
} finally {
    foreach ($process in $probeChildren) { $process.Refresh(); if (-not $process.HasExited) { Stop-Process -Id $process.Id } }
    $env:THEARCHIVE_PROBE_DIR = $savedProbeDir
    $env:THEARCHIVE_PROBE_ID = $savedProbeId
}
