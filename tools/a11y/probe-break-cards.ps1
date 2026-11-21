# A11Y-1: UI Automation probe for Ocy's break cards (Windows PowerShell 5.1 or pwsh on Windows).
#   powershell -ExecutionPolicy Bypass -File tools\a11y\probe-break-cards.ps1 -Exe .\Ocy.exe -Break short
# Starts an isolated demo break, then lists every named UIA element under each of Ocy's top-level
# windows, twice: right away and again after AccessKit has had time to answer the first query.
# AccessKit builds its tree lazily: the FIRST WM_GETOBJECT on a window that never had focus returns
# a placeholder root, the real tree follows a moment later. A single FindAll therefore shows nothing.
param(
  [string]$Exe = ".\Ocy.exe",
  [ValidateSet("short", "warning", "long")][string]$Break = "short",
  [string]$Language = "en"
)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$p = Start-Process -FilePath $Exe -ArgumentList "--demo-language $Language --demo-break $Break" -PassThru
Start-Sleep -Seconds 3
$cond = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $p.Id)
function Dump($label) {
  Write-Host "== $label"
  $tops = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
    [System.Windows.Automation.TreeScope]::Children, $cond)
  foreach ($w in $tops) {
    $r = $w.Current.BoundingRectangle
    Write-Host ("window '{0}' class={1} {2}x{3}" -f $w.Current.Name, $w.Current.ClassName, $r.Width, $r.Height)
    $all = $w.FindAll([System.Windows.Automation.TreeScope]::Descendants,
      [System.Windows.Automation.Condition]::TrueCondition)
    foreach ($e in $all) {
      if ($e.Current.Name) {
        Write-Host ("   {0,-14} '{1}'" -f $e.Current.ControlType.ProgrammaticName.Replace("ControlType.", ""), $e.Current.Name)
      }
    }
  }
}
Dump "first query (expect only the placeholder on a never-focused card)"
Start-Sleep -Milliseconds 1500
Dump "second query (expect the card's text and buttons)"
$hasFocus = [System.Windows.Automation.AutomationElement]::FocusedElement.Current.ProcessId -eq $p.Id
Write-Host "card took focus: $hasFocus (must be False)"
Stop-Process -Id $p.Id
