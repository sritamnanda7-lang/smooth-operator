# SPDX-License-Identifier: FSL-1.1-MIT
<#
 Smooth Operator step-0 Windows tester kit. No admin needed. Run in a normal PowerShell:
   powershell -ExecutionPolicy Bypass -File .\windows-tester.ps1 -Installer .\Smooth-Operator-setup.exe -ExpectedSha256 <hash from founder>
 Needs next to it: idle-ram.ps1 and keytest.html. Asks Y/N per gate; writes results-<date>.txt here.
 It never records keys, audio or screen. The app's harness log holds event types + timestamps only.
#>
param(
  [Parameter(Mandatory = $true)][string]$Installer,
  [Parameter(Mandatory = $true)][string]$ExpectedSha256,
  [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Smooth Operator"),
  [string]$ExeName = "smooth-operator.exe",
  [string]$HarnessLog = (Join-Path $env:APPDATA "com.sritamnanda.smoothoperator\spike-harness.log")
)
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$date = Get-Date -Format yyyy-MM-dd
$resultsFile = Join-Path $here "results-$date.txt"
$results = New-Object System.Collections.Generic.List[object]

function Add-Result($gate, $result, $note) {
  $results.Add([pscustomobject]@{ Gate = $gate; Result = $result; Note = $note })
  Write-Host ("[{0}] {1} {2}" -f $result, $gate, $note) -ForegroundColor $(if ($result -eq "PASS") { "Green" } elseif ($result -eq "FAIL") { "Red" } else { "Yellow" })
}
function Ask($q) {
  while ($true) {
    $a = (Read-Host "$q [Y/N]").Trim().ToUpper()
    if ($a -eq "Y" -or $a -eq "N") { return ($a -eq "Y") }
  }
}
function Gate-YN($gate, $steps, $q) {
  Write-Host "`n=== $gate ===" -ForegroundColor Cyan
  $steps | ForEach-Object { Write-Host "  - $_" }
  if (Ask $q) { Add-Result $gate "PASS" "" } else {
    $why = Read-Host "One line: what went wrong? (no personal data)"
    Add-Result $gate "FAIL" $why
  }
}

# --- preflight ---
$os = Get-CimInstance Win32_OperatingSystem
$osText = "$($os.Caption) build $($os.BuildNumber)"
Write-Host "OS: $osText"
if ([int]$os.BuildNumber -lt 22000) { Write-Host "This is not Windows 11. Stop and tell the founder." -ForegroundColor Red; Add-Result "preflight OS" "FAIL" $osText; $results | Format-Table | Out-String | Set-Content $resultsFile; return }
Add-Result "preflight OS" "PASS" $osText
try {
  $d = Get-MpComputerStatus
  $note = "RealTimeProtection=$($d.RealTimeProtectionEnabled)"
  if ($d.RealTimeProtectionEnabled) { Add-Result "preflight Defender on" "PASS" $note } else { Add-Result "preflight Defender on" "FAIL" "$note (turn Windows Security real-time protection ON and re-run)" }
} catch { Add-Result "preflight Defender on" "FAIL" "cannot read Defender status: $($_.Exception.Message)" }

# --- SHA-256 ---
$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Installer).Hash
if ($hash -ieq $ExpectedSha256.Trim()) { Add-Result "installer SHA-256" "PASS" $hash } else {
  Add-Result "installer SHA-256" "FAIL" "got $hash"; Write-Host "Hash differs. Do not install. Tell the founder." -ForegroundColor Red
  $results | Format-Table -AutoSize | Out-String -Width 200 | Set-Content $resultsFile; return
}

# --- install (per-user, silent) ---
Write-Host "`nSmartScreen may say 'Windows protected your PC': click More info, then Run anyway. Write down exactly what you see."
$p = Start-Process -FilePath $Installer -ArgumentList "/S" -PassThru -Wait
$exe = Join-Path $InstallDir $ExeName
if ($p.ExitCode -eq 0 -and (Test-Path $exe)) { Add-Result "install /S per-user" "PASS" $InstallDir } else { Add-Result "install /S per-user" "FAIL" "exit=$($p.ExitCode) exe present=$(Test-Path $exe)" }
Gate-YN "SmartScreen/Defender did not block the run" @("Did the installer run after More info -> Run anyway (or no warning)?", "Did Defender quarantine or delete anything?") "Installer ran and nothing was quarantined?"

# --- launch ---
Start-Process -FilePath $exe
Write-Host "`nApp started. Allow the microphone if Windows asks (Settings > Privacy > Microphone must be On)."
[void](Read-Host "Press Enter when the app panel/tray icon is visible")

Gate-YN "1 Hotkey" @(
  "Default hotkey is Right Ctrl (right-hand Ctrl key).",
  "Do 10 short holds (about 0.5 s). The panel must show 'hold' each time.",
  "Do 10 double-taps. The panel must show 'double-tap' each time.",
  "Open Notepad, tap Right Alt alone: no menu bar highlight, no key tips.",
  "Press AltGr (Ctrl+Alt on your layout): the panel must NOT react to its Ctrl part.") "All 10/10 hold, 10/10 double-tap, Notepad clean, AltGr ignored?"

Write-Host "`nOpening keytest.html in your default browser (Chrome preferred)."
Start-Process (Join-Path $here "keytest.html")
Gate-YN "2 Swallow" @(
  "Click the box on the page. Hold the hotkey and press Enter, 20 times. Then hotkey+Esc, 20 times.",
  "The Enter/Escape counters at the top must stay 0 down / 0 up.",
  "Then press plain Enter and plain Esc once each: counters must go up (sanity check).") "Counters stayed 0 during all 40 combos, and plain keys showed up?"

Gate-YN "3 Panel" @(
  "Open Chrome full screen (F11), click in a text box (any page, do not type personal data).",
  "Hold the hotkey: the panel must appear on screen and typing in Chrome must keep working (cursor stays in the box).",
  "Panel devtools tests are done by the founder remotely; skip them.") "Panel visible, Chrome kept focus?"

Gate-YN "6 Windows hook under stall" @(
  "In the panel/settings window click the 'stall 2 s' button (spike build) and, while it is stalled, hold the hotkey.",
  "After the stall ends, the hold must still have been registered in the harness (panel shows it, or log has it)." ) "Hook still fired during the stall?"

Gate-YN "7 Extension" @(
  "Chrome > Extensions > Developer mode > Load unpacked > pick the spike extension folder the founder sent.",
  "Its popup/badge must say 'connected' to the app within a few seconds.",
  "No Windows Firewall prompt and no Chrome local-network permission prompt may have appeared.") "Connected with no prompt?"

# --- harness log (event types + timestamps only) ---
Write-Host "`n=== Harness log ==="
if (Test-Path $HarnessLog) {
  $lines = Get-Content -LiteralPath $HarnessLog -Tail 40
  Write-Host "Last lines (event type + ms only):"; $lines | ForEach-Object { Write-Host "  $_" }
  Add-Result "harness log readable" "PASS" "$HarnessLog ($((Get-Content -LiteralPath $HarnessLog).Count) lines)"
} else { Add-Result "harness log readable" "FAIL" "not found: $HarnessLog" }

# --- idle RAM (fresh launch) ---
Write-Host "`n=== Gate 4 idle RAM: closing the app, relaunching, waiting 60 s + 120 s. Do not touch the PC. ==="
Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($ExeName)) -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 3
$ramScript = Join-Path $here "idle-ram.ps1"
$ramOut = & powershell -NoProfile -ExecutionPolicy Bypass -File $ramScript -ExePath $exe -ProcessName ([IO.Path]::GetFileNameWithoutExtension($ExeName)) -CI 2>&1
$ramOut | ForEach-Object { Write-Host "  $_" }
$line = ($ramOut | Where-Object { $_ -match "^GATE 4" } | Select-Object -Last 1)
$tot = ($ramOut | Where-Object { $_ -match "^TOTAL_RSS_MB" } | Select-Object -Last 1)
if ($line -match "PASS") { Add-Result "4 Idle RAM/CPU" "PASS" $tot } else { Add-Result "4 Idle RAM/CPU" "FAIL" "$tot" }

# --- results ---
$hdr = "Smooth Operator step 0 Windows results`nDate: $date`nOS: $osText`nInstaller SHA-256: $hash`n"
$table = $results | Format-Table -AutoSize -Wrap | Out-String -Width 200
Set-Content -LiteralPath $resultsFile -Value ($hdr + $table)
Write-Host "`nResults written to $resultsFile. Send that file (it holds no personal data) to the founder." -ForegroundColor Cyan
