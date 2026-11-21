<#
.SYNOPSIS
  Remove the per-user Steelgum install while keeping its settings and logs.
.EXAMPLE
  irm https://brumley.info/steelgum/uninstall.ps1 | iex
#>
$ErrorActionPreference = 'Stop'
$debugDir = Join-Path $env:USERPROFILE '.config\steelgum\debug'
$logFile = Join-Path $debugDir 'update.log'
$installDir = Join-Path $env:LOCALAPPDATA 'Programs\Steelgum'
$installedExe = Join-Path $installDir 'steelgum.exe'
$registryKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{CA30D8B3-ACB0-4951-B503-2F91D6DD3BD4}_is1'
$lock = $null
$failed = $false

function Write-InstallLog([string]$Message) {
  Write-Host "[steelgum] $Message"
  Add-Content -LiteralPath $logFile -Value "[$(Get-Date -Format o)] [steelgum] $Message"
}

try {
  New-Item -ItemType Directory -Path $debugDir -Force | Out-Null
  $lock = [IO.File]::Open((Join-Path $debugDir 'install.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::Write, [IO.FileShare]::None)
  Write-InstallLog 'Uninstaller started.'
  if (-not (Test-Path -LiteralPath $registryKey)) {
    Write-InstallLog 'No registered per-user Steelgum installation was found.'
  } else {
    $entry = Get-ItemProperty -LiteralPath $registryKey
    if ($entry.DisplayName -cne 'Steelgum' -or $entry.Publisher -cne 'Jack Brumley') {
      throw 'The registered product identity does not match Steelgum.'
    }
    if ([string]$entry.UninstallString -cnotmatch '^"([^"]+)"$') {
      throw 'The registered Steelgum uninstaller path is invalid.'
    }
    $uninstaller = [IO.Path]::GetFullPath($Matches[1])
    if (
      [IO.Path]::GetDirectoryName($uninstaller) -ine [IO.Path]::GetFullPath($installDir) -or
      [IO.Path]::GetFileName($uninstaller) -cnotmatch '^unins[0-9]+\.exe$'
    ) {
      throw 'The registered uninstaller is outside the Steelgum install location.'
    }
    foreach ($process in @(Get-Process -Name 'steelgum' -ErrorAction SilentlyContinue)) {
      try {
        if (-not $process.HasExited -and $process.Path -ieq $installedExe) {
          if (-not $process.CloseMainWindow() -or -not $process.WaitForExit(10000)) {
            throw 'Close the installed Steelgum window and try again.'
          }
        }
      } finally { $process.Dispose() }
    }
    $process = Start-Process -FilePath $uninstaller -ArgumentList @(
      '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART'
    ) -Wait -PassThru
    $exitCode = $process.ExitCode
    $process.Dispose()
    if ($exitCode -ne 0) { throw "The Steelgum uninstaller exited with code $exitCode." }
    Write-InstallLog 'Removed Steelgum. Settings and logs were kept.'
  }
} catch {
  $failed = $true
  if (Test-Path -LiteralPath $debugDir -PathType Container) {
    Write-InstallLog "ERROR: $($_.Exception.Message)"
  } else {
    [Console]::Error.WriteLine("[steelgum] ERROR: $($_.Exception.Message)")
  }
} finally {
  if ($lock) { $lock.Dispose() }
}
if ($failed) { exit 1 }
