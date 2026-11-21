$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

Push-Location (Join-Path $PSScriptRoot '../..')
try {
    $version = node scripts/release-assets.mjs version
    $asset = node scripts/release-assets.mjs asset windows
    $outputBase = [IO.Path]::GetFileNameWithoutExtension($asset)
    $lockArgs = @()
    if (Test-Path 'apps/door-game-bevy/Cargo.lock') {
        $lockArgs = @('--locked')
    }
    cargo build --release --manifest-path apps/door-game-bevy/Cargo.toml --bin steelgum @lockArgs

    $actual = & './apps/door-game-bevy/target/release/steelgum.exe' --version
    if ($actual -cne "steelgum $version") {
        throw "Expected steelgum $version, got $actual."
    }

    $compiler = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe'
    if (-not (Test-Path $compiler -PathType Leaf)) {
        throw "Inno Setup compiler not found at $compiler."
    }
    $workDirectory = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Path $workDirectory | Out-Null
    try {
        $icon = Join-Path $workDirectory 'steelgum.ico'
        magick -background none docs/assets/steelgum.svg -define icon:auto-resize=128,64,48,32,16 $icon
        & $compiler "/DSteelgumVersion=$version" "/DOutputBase=$outputBase" "/DSteelgumIcon=$icon" packaging/windows/steelgum.iss
        if (-not (Test-Path "release-artifacts/$asset" -PathType Leaf)) {
            throw "Setup did not create $asset."
        }
    } finally {
        Remove-Item -LiteralPath $workDirectory -Recurse -Force
    }
} finally {
    Pop-Location
}
