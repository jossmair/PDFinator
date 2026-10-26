param([string]$Executable = (Join-Path (Split-Path $PSScriptRoot) 'runtime/pdfinator.exe'))
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$pdfRoot = Split-Path $PSScriptRoot
$pdfIcon = [System.Drawing.Icon]::ExtractAssociatedIcon((Resolve-Path -LiteralPath $Executable).Path)
if ($null -eq $pdfIcon) { throw "Windows ne trouve pas l’icône de cet EXE." }
$pdfBitmap = $pdfIcon.ToBitmap()
$pdfReferenceIcon = [System.Drawing.Icon]::new((Join-Path $pdfRoot 'assets/pdfinator.ico'), $pdfBitmap.Width, $pdfBitmap.Height)
$pdfReference = $pdfReferenceIcon.ToBitmap()
try {
    $pdfDifferent = 0
    for ($pdfY = 0; $pdfY -lt $pdfBitmap.Height; $pdfY++) {
        for ($pdfX = 0; $pdfX -lt $pdfBitmap.Width; $pdfX++) {
            if ($pdfBitmap.GetPixel($pdfX, $pdfY).ToArgb() -ne $pdfReference.GetPixel($pdfX, $pdfY).ToArgb()) { $pdfDifferent++ }
        }
    }
    if ($pdfDifferent -gt 0) { throw "L’icône extraite par Windows diffère : $pdfDifferent pixels." }
    $pdfBitmap.Save((Join-Path $pdfRoot 'test-output/explorer-icon-verified.png'), [System.Drawing.Imaging.ImageFormat]::Png)
    "Icône Windows réelle vérifiée : $($pdfBitmap.Width) × $($pdfBitmap.Height), identique à l’identité PDFinator."
}
finally {
    $pdfBitmap.Dispose(); $pdfIcon.Dispose(); $pdfReference.Dispose(); $pdfReferenceIcon.Dispose()
}
