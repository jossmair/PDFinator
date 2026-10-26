param([switch]$DebugBuild, [switch]$SkipTests)
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    $cargo = 'cargo'
    if (Test-Path '.tools/cargo/bin/cargo.exe') {
        $env:RUSTUP_HOME = Join-Path $PSScriptRoot '.tools/rustup'
        $env:CARGO_HOME = Join-Path $PSScriptRoot '.tools/cargo'
        $cargo = Join-Path $env:CARGO_HOME 'bin/cargo.exe'
    }
    if (-not $SkipTests) {
        New-Item -ItemType Directory -Force test-output | Out-Null
        & $cargo test --locked -j 2
        if ($LASTEXITCODE) { throw 'Les tests ont échoué.' }
    }
    $profile = if ($DebugBuild) { 'debug' } else { 'release' }
    $arguments = @('build', '--locked', '-j', '2')
    $targetDirectory = 'target'
    if (-not $DebugBuild) {
        $targetDirectory = 'target-release'
        $env:RUSTFLAGS = '-C target-feature=+crt-static'
        $arguments += @('--release', '--target-dir', $targetDirectory)
    }
    & $cargo @arguments
    if ($LASTEXITCODE) { throw 'La compilation a échoué.' }
    New-Item -ItemType Directory -Force runtime | Out-Null
    $pdfStagedExe = Join-Path $PSScriptRoot 'runtime/pdfinator.update.exe'
    $pdfInstalledExe = Join-Path $PSScriptRoot 'runtime/pdfinator.exe'
    Copy-Item "$targetDirectory/$profile/pdfinator.exe" $pdfStagedExe -Force
    try {
        Copy-Item -LiteralPath $pdfStagedExe -Destination $pdfInstalledExe -Force
        Remove-Item -LiteralPath $pdfStagedExe
    }
    catch {
        # A running image can be renamed on Windows. Keep it alive with its open PDFs.
        if (-not (Test-Path -LiteralPath $pdfInstalledExe)) { throw }
        $pdfBackupDirectory = Join-Path $PSScriptRoot 'runtime/.previous'
        New-Item -ItemType Directory -Force $pdfBackupDirectory | Out-Null
        $pdfBackupExe = Join-Path $pdfBackupDirectory ('pdfinator-' + [guid]::NewGuid().ToString('N') + '.exe')
        Move-Item -LiteralPath $pdfInstalledExe -Destination $pdfBackupExe
        try { Move-Item -LiteralPath $pdfStagedExe -Destination $pdfInstalledExe }
        catch {
            Move-Item -LiteralPath $pdfBackupExe -Destination $pdfInstalledExe
            throw
        }
    }
    & python scripts/generate_licenses.py
    if ($LASTEXITCODE) { throw 'La génération des mentions légales a échoué.' }
    Copy-Item -LiteralPath licenses/LICENCES.txt -Destination runtime/LICENCES.txt -Force
    Get-FileHash runtime/pdfinator.exe -Algorithm SHA256 | Format-List
}
finally { Pop-Location }
