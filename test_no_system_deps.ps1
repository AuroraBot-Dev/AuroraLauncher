# Launch the launcher in a "clean host" environment so it cannot detect the
# git/uv/python/pnpm already installed on this machine (PATH stripped to system
# dirs only, and APPDATA/USERPROFILE pointed at empty folders to defeat the
# extra well-known-dir fallback in tools.rs::which_in_path).
# The launcher must then download + manage its own toolchain -> real download test.
$ErrorActionPreference = 'Stop'

$exe = Join-Path $PSScriptRoot 'src-tauri\target\release\aurora-launcher.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw "release exe not found: $exe" }

$isoRoot = Join-Path $env:TEMP ('aurora-no-deps-profile-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path (Join-Path $isoRoot 'AppData\Local') | Out-Null

# Close any running instance (file lock would block nothing here, but avoids confusion)
Get-Process -Name 'aurora-launcher' -ErrorAction SilentlyContinue | Stop-Process -Force

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $exe
$psi.WorkingDirectory = Split-Path $exe
$psi.UseShellExecute = $false
$psi.EnvironmentVariables.Clear()
$psi.EnvironmentVariables['PATH']           = "$env:SystemRoot\System32;$env:SystemRoot"
$psi.EnvironmentVariables['SystemRoot']     = $env:SystemRoot
$psi.EnvironmentVariables['windir']         = $env:windir
$psi.EnvironmentVariables['SystemDrive']    = $env:SystemDrive
$psi.EnvironmentVariables['COMSPEC']        = "$env:SystemRoot\System32\cmd.exe"
$psi.EnvironmentVariables['PATHEXT']        = '.COM;.EXE;.BAT;.CMD'
$psi.EnvironmentVariables['USERPROFILE']    = $isoRoot
$psi.EnvironmentVariables['HOME']           = $isoRoot
$psi.EnvironmentVariables['APPDATA']        = (Join-Path $isoRoot 'AppData')
$psi.EnvironmentVariables['LOCALAPPDATA']   = (Join-Path $isoRoot 'AppData\Local')

[System.Diagnostics.Process]::Start($psi) | Out-Null
Write-Host "Started in clean environment: $exe"
Write-Host "System tools are hidden. In the launcher UI, click install dependencies; it should download the managed toolchain into <exe dir>\tool\tools\"
