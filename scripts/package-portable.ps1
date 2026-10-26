param()
$ErrorActionPreference = 'Stop'
$pdfRoot = Split-Path $PSScriptRoot
$pdfManifest = Get-Content -LiteralPath (Join-Path $pdfRoot 'Cargo.toml') -Raw
$pdfVersion = [regex]::Match($pdfManifest, '(?m)^version = "([^"]+)"').Groups[1].Value
$pdfExecutable = Join-Path $pdfRoot 'runtime/pdfinator.exe'
& python (Join-Path $PSScriptRoot 'generate_licenses.py') --check
if ($LASTEXITCODE) { throw 'Les mentions légales ne correspondent pas à cette version.' }
if ((Get-Item -LiteralPath $pdfExecutable).VersionInfo.ProductVersion -ne $pdfVersion) {
    throw 'Construisez la version actuelle avant de créer le ZIP portable.'
}
$pdfDist = Join-Path $pdfRoot 'dist'
New-Item -ItemType Directory -Force -Path $pdfDist | Out-Null
$pdfZipPath = Join-Path $pdfDist "PDFinator-$pdfVersion-portable-exe-windows-x64.zip"
$pdfTemporaryZip = Join-Path $pdfDist ('.portable-' + [guid]::NewGuid().ToString('N') + '.zip')
Add-Type -AssemblyName System.IO.Compression.FileSystem
$pdfFiles = @(
    [pscustomobject]@{ Source = $pdfExecutable; Entry = 'PDFinator/PDFinator.exe' }
    [pscustomobject]@{ Source = (Join-Path $pdfRoot 'licenses/LICENCES.txt'); Entry = 'PDFinator/LICENCES.txt' }
)
$pdfZip = [System.IO.Compression.ZipFile]::Open($pdfTemporaryZip, [System.IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($pdfFile in $pdfFiles) {
        [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($pdfZip, $pdfFile.Source,
            $pdfFile.Entry, [System.IO.Compression.CompressionLevel]::Optimal) | Out-Null
    }
}
finally { $pdfZip.Dispose() }
# Verify the archive before replacing a previous delivery. Running app backups,
# caches, test documents and source trees are never added to this file list.
$pdfReadZip = [System.IO.Compression.ZipFile]::OpenRead($pdfTemporaryZip)
try {
    if ($pdfReadZip.Entries.Count -ne $pdfFiles.Count) { throw 'Le ZIP est incomplet.' }
    $pdfEntry = $pdfReadZip.GetEntry('PDFinator/PDFinator.exe')
    if ($null -eq $pdfEntry) { throw 'Exécutable absent du ZIP.' }
    foreach ($pdfArchiveEntry in $pdfReadZip.Entries) {
        if ($pdfArchiveEntry.FullName -notin @('PDFinator/PDFinator.exe', 'PDFinator/LICENCES.txt')) {
            throw 'Le ZIP contient un lanceur ou un chemin non prévu.'
        }
    }
    $pdfStream = $pdfEntry.Open()
    $pdfHasher = [System.Security.Cryptography.SHA256]::Create()
    try { $pdfArchivedHash = [BitConverter]::ToString($pdfHasher.ComputeHash($pdfStream)).Replace('-', '') }
    finally { $pdfStream.Dispose(); $pdfHasher.Dispose() }
    if ($pdfArchivedHash -ne (Get-FileHash -LiteralPath $pdfExecutable -Algorithm SHA256).Hash) {
        throw "L’exécutable archivé ne correspond pas à la version construite."
    }
}
finally { $pdfReadZip.Dispose() }
Move-Item -LiteralPath $pdfTemporaryZip -Destination $pdfZipPath -Force
[pscustomobject]@{
    Archive = $pdfZipPath
    Version = $pdfVersion
    Files = $pdfFiles.Count
    Bytes = (Get-Item -LiteralPath $pdfZipPath).Length
    SHA256 = (Get-FileHash -LiteralPath $pdfZipPath -Algorithm SHA256).Hash
} | Format-List
