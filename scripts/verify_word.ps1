param([string]$Document = 'test-output/38-document-reel.docx', [string]$Output = 'test-output/office-word-059.pdf')
$ErrorActionPreference = 'Stop'
if (Get-Process WINWORD -ErrorAction SilentlyContinue) { throw 'Word est déjà ouvert ; aucune session utilisateur ne sera touchée.' }
$word = $null
$doc = $null
try {
    $word = New-Object -ComObject Word.Application
    $word.Visible = $false
    $word.DisplayAlerts = 0
    $word.AutomationSecurity = 3
    $inputPath = (Resolve-Path -LiteralPath $Document).Path
    $outputPath = [IO.Path]::GetFullPath((Join-Path $pwd $Output))
    $doc = $word.Documents.Open($inputPath, $false, $true, $false)
    $doc.ExportAsFixedFormat($outputPath, 17)
    [pscustomobject]@{Version = $word.Version; Pages = $doc.ComputeStatistics(2); Output = $outputPath}
} finally {
    if ($doc) { $doc.Close(0); [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($doc) }
    if ($word) { $word.Quit(0); [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($word) }
}
