<#
.SYNOPSIS
  Install the x64 Steelgum desktop application for the current Windows user.
.EXAMPLE
  irm https://brumley.info/steelgum/install.ps1 | iex
#>
param(
  [string]$Version = '',
  [switch]$NoRelaunch,
  [switch]$Relaunch,
  [switch]$InsecureSkipVerify
)

$ErrorActionPreference = 'Stop'
$repo = 'jackbrumley/steelgum'
$versionPattern = '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
# Windows PowerShell 5.1 also runs on machines whose default is pre-TLS 1.2.
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

if ($PSVersionTable.PSEdition -ne 'Core') {
  # PowerShell 7 can pass its module folders to the 5.1 updater process.
  $env:PSModulePath = ($env:PSModulePath -split ';' | Where-Object { $_ -and $_ -notmatch '\\PowerShell\\' }) -join ';'
}

$debugDir = Join-Path $env:USERPROFILE '.config\steelgum\debug'
$logFile = Join-Path $debugDir 'update.log'
$installDir = Join-Path $env:LOCALAPPDATA 'Programs\Steelgum'
$installedExe = Join-Path $installDir 'steelgum.exe'
$lock = $null
$tempDir = $null
$failed = $false

function Write-InstallLog([string]$Message) {
  Write-Host "[steelgum] $Message"
  if ($env:STEELGUM_UPDATE_LOG -ine $logFile) {
    Add-Content -LiteralPath $logFile -Value "[$(Get-Date -Format o)] [steelgum] $Message"
  }
}

function Get-GitHubResponse([uri]$Uri, [bool]$Metadata) {
  # Manually follow HTTPS asset redirects. The bearer token stays on the API.
  for ($index = 0; $index -lt 6; $index++) {
    $allowedHost = if ($Metadata) {
      $Uri.Host -eq 'api.github.com'
    } else {
      $Uri.Host -eq 'github.com' -or $Uri.Host.EndsWith('.githubusercontent.com', [StringComparison]::OrdinalIgnoreCase)
    }
    if ($Uri.Scheme -cne 'https' -or -not $Uri.IsDefaultPort -or $Uri.UserInfo -or -not $allowedHost) {
      throw "Unexpected GitHub URL: $Uri"
    }
    $request = [Net.HttpWebRequest][Net.WebRequest]::Create($Uri)
    $request.AllowAutoRedirect = $false
    $request.UserAgent = 'steelgum-installer'
    $request.Timeout = 30000
    $request.ReadWriteTimeout = 30000
    if ($Metadata) {
      $request.Accept = 'application/vnd.github+json'
      if ($env:STEELGUM_GITHUB_TOKEN) {
        $request.Headers['Authorization'] = "Bearer $env:STEELGUM_GITHUB_TOKEN"
      }
    }
    $response = [Net.HttpWebResponse]$request.GetResponse()
    if ([int]$response.StatusCode -eq 200) {
      return $response
    }
    $status = [int]$response.StatusCode
    $location = $response.Headers['Location']
    $response.Dispose()
    if ($Metadata -or $status -notin @(301, 302, 303, 307, 308) -or -not $location) {
      throw "GitHub returned unexpected HTTP status $status."
    }
    $Uri = [uri]::new($Uri, $location)
  }
  throw 'GitHub sent too many redirects.'
}

function Get-Release([string]$Endpoint) {
  $response = Get-GitHubResponse "https://api.github.com/repos/$repo/releases/$Endpoint" $true
  try {
    $reader = [IO.StreamReader]::new($response.GetResponseStream())
    try { return ($reader.ReadToEnd() | ConvertFrom-Json) }
    finally { $reader.Dispose() }
  } finally { $response.Dispose() }
}

function Save-ReleaseAsset([uri]$Url, [string]$Destination) {
  $response = Get-GitHubResponse $Url $false
  try {
    $inputStream = $response.GetResponseStream()
    $outputStream = [IO.File]::Open($Destination, [IO.FileMode]::CreateNew)
    try { $inputStream.CopyTo($outputStream) }
    finally { $outputStream.Dispose() }
  } finally { $response.Dispose() }
  if ((Get-Item -LiteralPath $Destination).Length -eq 0) {
    throw 'GitHub returned an empty installer.'
  }
}

function Close-InstalledSteelgum {
  foreach ($process in @(Get-Process -Name 'steelgum' -ErrorAction SilentlyContinue)) {
    try {
      if (-not $process.HasExited -and $process.Path -ieq $installedExe) {
        Write-InstallLog 'Closing the installed Steelgum window.'
        if (-not $process.CloseMainWindow() -or -not $process.WaitForExit(10000)) {
          throw 'Close all installed Steelgum windows and run the installer again.'
        }
      }
    } finally { $process.Dispose() }
  }
}

try {
  New-Item -ItemType Directory -Path $debugDir -Force | Out-Null
  $lock = [IO.File]::Open((Join-Path $debugDir 'install.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::Write, [IO.FileShare]::None)
  Write-InstallLog 'Installer started.'
  if ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne [Runtime.InteropServices.Architecture]::X64) {
    throw 'Only x64 Windows is supported.'
  }
  if ($NoRelaunch -and $Relaunch) { throw 'Choose either -Relaunch or -NoRelaunch.' }
  if ($Version) {
    if ($Version -cnotmatch $versionPattern) { throw "Invalid release tag: $Version" }
  } else {
    $latest = Get-Release 'latest'
    if ($latest.draft -ne $false -or $latest.prerelease -ne $false -or $latest.tag_name -cnotmatch $versionPattern) {
      throw 'GitHub did not return a valid stable Steelgum release.'
    }
    $Version = $latest.tag_name
  }

  $release = Get-Release "tags/$Version"
  if ($release.tag_name -cne $Version -or $release.draft -ne $false) {
    throw 'GitHub returned a different release or a draft.'
  }
  $assetName = "steelgum-$($Version.Substring(1))-windows-x64-setup.exe"
  $assetUrl = "https://github.com/$repo/releases/download/$Version/$assetName"
  $assets = @($release.assets | Where-Object { $_.name -ceq $assetName })
  if ($assets.Count -ne 1 -or $assets[0].browser_download_url -cne $assetUrl) {
    throw "Expected one release asset at $assetUrl"
  }
  $expected = ''
  if ($InsecureSkipVerify) {
    Write-InstallLog 'WARNING: checksum verification disabled by request.'
  } else {
    if ([string]$assets[0].digest -cnotmatch '^sha256:([0-9a-fA-F]{64})$') {
      throw "GitHub did not supply a valid SHA-256 digest for $assetName."
    }
    $expected = $Matches[1]
  }

  $tempDir = Join-Path ([IO.Path]::GetTempPath()) ("steelgum-" + [guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Path $tempDir | Out-Null
  $installer = Join-Path $tempDir $assetName
  Write-InstallLog "Downloading $assetName."
  Save-ReleaseAsset $assetUrl $installer
  if (-not $InsecureSkipVerify) {
    $actual = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash
    if ($expected -ine $actual) { throw "SHA-256 mismatch for $assetName." }
    Write-InstallLog 'SHA-256 verified.'
  }

  Close-InstalledSteelgum
  $setupLog = Join-Path $tempDir 'setup.log'
  $process = Start-Process -FilePath $installer -ArgumentList @(
    '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', "/LOG=`"$setupLog`""
  ) -Wait -PassThru
  if (Test-Path -LiteralPath $setupLog) {
    if ($env:STEELGUM_UPDATE_LOG -ieq $logFile) {
      Get-Content -LiteralPath $setupLog | ForEach-Object { Write-Host $_ }
    } else {
      Get-Content -LiteralPath $setupLog | Add-Content -LiteralPath $logFile
    }
  }
  $exitCode = $process.ExitCode
  $process.Dispose()
  if ($exitCode -ne 0) { throw "Setup exited with code $exitCode. See $logFile." }
  if (-not (Test-Path -LiteralPath $installedExe -PathType Leaf)) {
    throw "Setup did not install the executable at $installedExe."
  }
  Write-InstallLog "Installed $Version to $installedExe."
  if ($Relaunch -or (-not $NoRelaunch -and $env:STEELGUM_NO_RELAUNCH -ne '1')) {
    Write-InstallLog 'Starting Steelgum.'
    Start-Process -FilePath $installedExe
  }
} catch {
  $failed = $true
  if (Test-Path -LiteralPath $debugDir -PathType Container) {
    Write-InstallLog "ERROR: $($_.Exception.Message)"
  } else {
    [Console]::Error.WriteLine("[steelgum] ERROR: $($_.Exception.Message)")
  }
} finally {
  if ($tempDir -and (Test-Path -LiteralPath $tempDir)) {
    try { Remove-Item -LiteralPath $tempDir -Recurse -Force }
    catch { Write-InstallLog "WARNING: could not remove temporary installer files: $($_.Exception.Message)" }
  }
  if ($lock) { $lock.Dispose() }
}
if ($failed) { exit 1 }
