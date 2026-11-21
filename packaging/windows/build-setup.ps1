$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

Push-Location (Join-Path $PSScriptRoot '../..')
try {
    $version = node scripts/release-assets.mjs version
    $asset = node scripts/release-assets.mjs asset windows
    $outputBase = [IO.Path]::GetFileNameWithoutExtension($asset)
    $manifest = node scripts/release-assets.mjs desktop-manifest
    $target = Join-Path (Split-Path $manifest) 'target'
    $lockArgs = @()
    if (Test-Path (Join-Path (Split-Path $manifest) 'Cargo.lock')) {
        $lockArgs = @('--locked')
    }
    cargo build --release --manifest-path $manifest --target-dir $target --bin steelgum @lockArgs

    $binary = (Resolve-Path (Join-Path $target 'release/steelgum.exe')).Path
    $actual = & $binary --version
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
        & $compiler "/DSteelgumVersion=$version" "/DOutputBase=$outputBase" "/DSteelgumIcon=$icon" "/DSteelgumBinary=$binary" packaging/windows/steelgum.iss
        if (-not (Test-Path "release-artifacts/$asset" -PathType Leaf)) {
            throw "Setup did not create $asset."
        }
    } finally {
        Remove-Item -LiteralPath $workDirectory -Recurse -Force
    }
} finally {
    Pop-Location
}
