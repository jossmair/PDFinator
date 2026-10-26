param([string]$Compiler = '')
$ErrorActionPreference = 'Stop'
$pdfRoot = Split-Path $PSScriptRoot
if (-not $Compiler) {
    $pdfCandidates = @(
        (Join-Path $pdfRoot '.tools/innosetup/ISCC.exe'),
        'C:/Program Files (x86)/Inno Setup 6/ISCC.exe',
        'C:/Program Files/Inno Setup 7/ISCC.exe'
    )
    $Compiler = $pdfCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}
if (-not $Compiler) { throw 'Installez Inno Setup 6 ou 7, ou indiquez -Compiler chemin/ISCC.exe.' }
$pdfManifest = Get-Content -LiteralPath (Join-Path $pdfRoot 'Cargo.toml') -Raw
$pdfVersion = [regex]::Match($pdfManifest, '(?m)^version = "([^"]+)"').Groups[1].Value
$pdfExe = Join-Path $pdfRoot 'runtime/pdfinator.exe'
if ((Get-Item -LiteralPath $pdfExe).VersionInfo.ProductVersion -ne $pdfVersion) {
    throw 'Construisez la version actuelle avant de créer le setup.'
}
& python (Join-Path $PSScriptRoot 'generate_licenses.py') --check
if ($LASTEXITCODE) { throw 'Les mentions légales ne correspondent pas à cette version.' }
& $Compiler '/Qp' "/DAppVersion=$pdfVersion" (Join-Path $PSScriptRoot 'pdfinator.iss')
if ($LASTEXITCODE) { throw 'La compilation du setup a échoué.' }
$pdfSetup = Join-Path $pdfRoot "dist/PDFinator-$pdfVersion-Setup.exe"
Get-Item -LiteralPath $pdfSetup | Select-Object FullName,Length
Get-FileHash -LiteralPath $pdfSetup -Algorithm SHA256
