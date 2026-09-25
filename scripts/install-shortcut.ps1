# Adds BeatFrame to the Start Menu and the Desktop for the current user.
# Nothing is copied or installed: the shortcut opens the exe that
# `cargo build --release` left in target\release, so this folder has to stay
# where it is.
#
#   powershell -ExecutionPolicy Bypass -File scripts\install-shortcut.ps1
#
# -Destination writes the shortcut into that folder only, and -Remove deletes
# the shortcuts this script writes.
[CmdletBinding()]
param(
  [string]$Destination = '',
  [switch]$Remove
)

$ErrorActionPreference = 'Stop'

$name = 'BeatFrame'
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root 'target\release\beatframe.exe'

# A shortcut to an exe that is not there opens nothing and says nothing.
if (-not $Remove -and -not (Test-Path -LiteralPath $exe)) {
  throw "$exe is missing. Run: cargo build --release"
}

if ($Destination -ne '') {
  New-Item -ItemType Directory -Path $Destination -Force | Out-Null
  $places = @((Resolve-Path -LiteralPath $Destination).Path)
} else {
  $places = @(
    [Environment]::GetFolderPath('Programs'),
    [Environment]::GetFolderPath('Desktop')
  )
}

if ($Remove) {
  foreach ($place in $places) {
    $lnk = Join-Path $place "$name.lnk"
    if (Test-Path -LiteralPath $lnk) {
      Remove-Item -LiteralPath $lnk
      Write-Output "removed $lnk"
    }
  }
  exit 0
}

$shell = New-Object -ComObject WScript.Shell
foreach ($place in $places) {
  $lnk = Join-Path $place "$name.lnk"
  $link = $shell.CreateShortcut($lnk)
  $link.TargetPath = $exe
  $link.WorkingDirectory = Split-Path -Parent $exe
  # The icon is the one build.rs puts inside the exe.
  $link.IconLocation = "$exe,0"
  $link.Description = 'Lights the screen edges to the drums'
  $link.Save()
  Write-Output "wrote $lnk"
}
