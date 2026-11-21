$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$scriptPath = Join-Path ([IO.Path]::GetTempPath()) ('steelgum-update-' + [guid]::NewGuid().ToString('N') + '.ps1')
try {
    [IO.File]::WriteAllText($env:STEELGUM_UPDATE_READY, "")
    $running = Get-Process -Id ([int]$env:STEELGUM_UPDATE_PARENT) -ErrorAction SilentlyContinue
    if ($null -ne $running -and -not $running.WaitForExit(20000)) {
        throw 'Steelgum did not exit; installation was cancelled.'
    }
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -UseBasicParsing -MaximumRedirection 0 -TimeoutSec 45 -Uri ($env:STEELGUM_UPDATE_SITE + '/install.ps1') -OutFile $scriptPath
        $powershell = Join-Path $PSHOME 'powershell.exe'
        & $powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $scriptPath -Version $env:STEELGUM_UPDATE_TAG -Relaunch
        if ($LASTEXITCODE -ne 0) { throw "The installer returned $LASTEXITCODE." }
    } catch {
        [Console]::Error.WriteLine($_.Exception.Message)
        [Console]::Error.WriteLine('Installation failed. Relaunching the installed Steelgum.')
        Start-Process -FilePath $env:STEELGUM_UPDATE_EXECUTABLE -ArgumentList '--update-failed'
        exit 1
    }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
} finally {
    if (Test-Path -LiteralPath $scriptPath) { Remove-Item -LiteralPath $scriptPath -Force }
}
