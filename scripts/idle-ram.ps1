# SPDX-License-Identifier: FSL-1.1-MIT
<#
 Gate 4 (Windows): idle RSS (working set) sum of the app + all msedgewebview2 descendants, and CPU %.
 Usage: .\idle-ram.ps1 [-ExePath <path to smooth-operator.exe>] [-ProcessName smooth-operator]
        [-WaitSeconds 60] [-CpuWindowSeconds 120] [-CI]
 -CI: launches ExePath itself, prints one summary line, exits 1 on FAIL (indicative on CI runners).
 Pass = < 150 MB and < 1% of one core. No admin. Output = numbers + process names only.
#>
param(
  [string]$ExePath = "",
  [string]$ProcessName = "smooth-operator",
  [int]$WaitSeconds = 60,
  [int]$CpuWindowSeconds = 120,
  [switch]$CI
)
$ErrorActionPreference = "Stop"

function Get-Descendants([int]$RootPid) {
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name
  $out = New-Object System.Collections.Generic.List[int]
  $queue = New-Object System.Collections.Generic.Queue[int]
  $queue.Enqueue($RootPid)
  while ($queue.Count -gt 0) {
    $cur = $queue.Dequeue()
    foreach ($c in ($all | Where-Object { $_.ParentProcessId -eq $cur })) { $out.Add([int]$c.ProcessId); $queue.Enqueue([int]$c.ProcessId) }
  }
  return $out
}

$started = $null
if ($ExePath -ne "") { $started = Start-Process -FilePath $ExePath -PassThru }
Write-Host "waiting $WaitSeconds s for idle..."
Start-Sleep -Seconds $WaitSeconds

$app = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue | Sort-Object StartTime | Select-Object -First 1
if (-not $app) { Write-Host "app process '$ProcessName' not found"; if ($CI) { exit 2 } else { return } }
$ids = @($app.Id) + (Get-Descendants $app.Id)

function Snapshot($ids) {
  $rows = @()
  foreach ($i in $ids) {
    $p = Get-Process -Id $i -ErrorAction SilentlyContinue
    if ($p) { $rows += [pscustomobject]@{ Pid = $p.Id; Name = $p.ProcessName; MB = [math]::Round($p.WorkingSet64 / 1MB, 1); Cpu = $p.TotalProcessorTime.TotalSeconds } }
  }
  return $rows
}
$s0 = Snapshot $ids
Start-Sleep -Seconds $CpuWindowSeconds
$ids = @($app.Id) + (Get-Descendants $app.Id)   # webview may have spawned more
$s1 = Snapshot $ids
$mb = [math]::Round(($s1 | Measure-Object MB -Sum).Sum, 1)
$cpuDelta = ($s1 | Measure-Object Cpu -Sum).Sum - ($s0 | Measure-Object Cpu -Sum).Sum
$cpuPct = [math]::Round($cpuDelta / $CpuWindowSeconds * 100, 2)
$os = (Get-CimInstance Win32_OperatingSystem).Caption + " " + (Get-CimInstance Win32_OperatingSystem).BuildNumber
$result = if ($mb -lt 150 -and $cpuPct -lt 1) { "PASS" } else { "FAIL" }
if (-not $CI) { $s1 | Format-Table -AutoSize | Out-String | Write-Host }
Write-Host "TOTAL_RSS_MB=$mb CPU_PCT_ONE_CORE=$cpuPct (window ${CpuWindowSeconds}s)"
Write-Host "GATE 4 (win): $result os=$os date=$(Get-Date -Format yyyy-MM-dd)"
if ($started) { Stop-Process -Id $started.Id -ErrorAction SilentlyContinue }
if ($CI -and $result -ne "PASS") { exit 1 }
