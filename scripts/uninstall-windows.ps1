# Remove Tamlil and everything it stored on this PC.
#   powershell -ExecutionPolicy Bypass -File scripts\uninstall-windows.ps1 [-KeepModels]
param([switch]$KeepModels)
$id = "com.enpitech.tamlil"
Get-Process -Name "Tamlil" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
$data = Join-Path $env:APPDATA $id
if ($KeepModels -and (Test-Path (Join-Path $data "models"))) {
  Get-ChildItem $data -Force | Where-Object { $_.Name -ne "models" } | Remove-Item -Recurse -Force
} else {
  Remove-Item -Recurse -Force $data -ErrorAction SilentlyContinue
}
Remove-Item -Recurse -Force (Join-Path $env:LOCALAPPDATA $id) -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force (Join-Path $env:LOCALAPPDATA "Tamlil") -ErrorAction SilentlyContinue
$uninst = Get-ItemProperty "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*","HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*" -ErrorAction SilentlyContinue |
  Where-Object { $_.DisplayName -eq "Tamlil" } | Select-Object -First 1
if ($uninst -and $uninst.UninstallString) {
  Write-Host "Running the Tamlil uninstaller..."
  Start-Process -FilePath "cmd.exe" -ArgumentList "/C", $uninst.UninstallString -Wait
} else {
  Write-Host "Remove Tamlil from Settings > Apps if it is still listed."
}
Write-Host "Done."
