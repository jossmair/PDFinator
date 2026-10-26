param([string]$Setup = '')
$ErrorActionPreference = 'Stop'
$pdfRoot = Split-Path $PSScriptRoot
$pdfVersion = [regex]::Match((Get-Content -LiteralPath (Join-Path $pdfRoot 'Cargo.toml') -Raw), '(?m)^version = "([^"]+)"').Groups[1].Value
if (-not $Setup) { $Setup = Join-Path $pdfRoot "dist/PDFinator-$pdfVersion-Setup.exe" }
$pdfTestParent = [IO.Path]::GetFullPath((Join-Path $pdfRoot 'test-output'))
$pdfTestDirectory = Join-Path $pdfTestParent ('installation espaces-é-' + [guid]::NewGuid().ToString('N'))
if (-not $pdfTestDirectory.StartsWith($pdfTestParent + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Dossier de test hors du projet.'
}
$pdfInstallLog = Join-Path $pdfTestParent 'setup-install.log'
$pdfUninstallLog = Join-Path $pdfTestParent 'setup-uninstall.log'
# The production installer has a smoke mode: same payload and uninstaller, with
# no Start Menu/desktop shortcuts or uninstall registry entry on the user's PC.
$pdfProcess = Start-Process -FilePath $Setup -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/SP-',
    '/NORESTART','/NOCLOSEAPPLICATIONS','/TESTINSTALL=1','/TASKS=',
    ('/DIR="' + $pdfTestDirectory + '"'), ('/LOG="' + $pdfInstallLog + '"')) -WindowStyle Hidden -PassThru
$pdfProcess.WaitForExit()
if ($pdfProcess.ExitCode -ne 0) { throw "Installation échouée : $($pdfProcess.ExitCode). Voir $pdfInstallLog" }
$pdfInstalledExe = Join-Path $pdfTestDirectory 'PDFinator.exe'
$pdfInstalledLegal = Join-Path $pdfTestDirectory 'LICENCES.txt'
foreach ($pdfPair in @(
    @($pdfInstalledExe, (Join-Path $pdfRoot 'runtime/pdfinator.exe')),
    @($pdfInstalledLegal, (Join-Path $pdfRoot 'licenses/LICENCES.txt'))
)) {
    if ((Get-FileHash -LiteralPath $pdfPair[0] -Algorithm SHA256).Hash -ne
        (Get-FileHash -LiteralPath $pdfPair[1] -Algorithm SHA256).Hash) { throw 'Fichier installé différent du fichier construit.' }
}
if ((Get-Item -LiteralPath $pdfInstalledExe).VersionInfo.ProductVersion -ne $pdfVersion) { throw 'Mauvaise version installée.' }
$pdfInstalledFiles = @(Get-ChildItem -LiteralPath $pdfTestDirectory -File | Select-Object -ExpandProperty Name)
if (@($pdfInstalledFiles | Where-Object { $_ -notin @('PDFinator.exe','LICENCES.txt','unins000.exe','unins000.dat') }).Count) {
    throw 'Fichier inattendu installé.'
}
$pdfUninstaller = Join-Path $pdfTestDirectory 'unins000.exe'
$pdfProcess = Start-Process -FilePath $pdfUninstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/LOG="' + $pdfUninstallLog + '"')) -WindowStyle Hidden -PassThru
$pdfProcess.WaitForExit()
if ($pdfProcess.ExitCode -ne 0) { throw "Désinstallation échouée : $($pdfProcess.ExitCode)." }
# The uninstaller finishes removing its own running image asynchronously.
for ($pdfAttempt = 0; $pdfAttempt -lt 50 -and (Test-Path -LiteralPath $pdfTestDirectory); $pdfAttempt++) {
    Start-Sleep -Milliseconds 100
}
if (Test-Path -LiteralPath $pdfInstalledExe) { throw 'Exécutable conservé après désinstallation.' }
if (Test-Path -LiteralPath $pdfInstalledLegal) { throw 'Fichier légal conservé après désinstallation.' }
$pdfResult = [ordered]@{ Version = $pdfVersion; SetupSHA256 = (Get-FileHash -LiteralPath $Setup -Algorithm SHA256).Hash;
    InstalledFiles = $pdfInstalledFiles; PayloadHashesMatch = $true; Uninstalled = $true;
    TestDirectoryRemoved = -not (Test-Path -LiteralPath $pdfTestDirectory); InstallLog = $pdfInstallLog; UninstallLog = $pdfUninstallLog }
$pdfResult | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $pdfRoot "setup-verification-$($pdfVersion.Replace('.','')).json") -Encoding utf8
[pscustomobject]$pdfResult | Format-List
