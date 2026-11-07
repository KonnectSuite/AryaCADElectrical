#Build a portable AryaCADElectrical folder.
#
# Copies the release binary, the electrical library (symbols, parts, panel footprints,
# systems) and the quick-start document into one folder that can be zipped and moved to
# another machine. Nothing outside the workspace is written.
#
#   powershell -File packaging\portable.ps1
#   powershell -File packaging\portable.ps1 -Binary C:\build\OpenCADStudio.exe -Out dist\portable
param(
    [string]$Binary = '',
    [string]$Out = 'dist\AryaCADElectrical-portable'
)

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($Binary)) {
    $Binary = Join-Path $root 'target\release\OpenCADStudio.exe'
}
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
    throw "binary not found: $Binary (build with cargo build --release first)"
}

$outPath = [System.IO.Path]::GetFullPath((Join-Path $root $Out))
$rootPath = [System.IO.Path]::GetFullPath($root)
if (-not $outPath.StartsWith($rootPath, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "refusing to write outside the workspace: $outPath"
}
if ($outPath -eq $rootPath) {
    throw "refusing to use the workspace root as the output folder"
}

if (Test-Path -LiteralPath $outPath) {
    Remove-Item -LiteralPath $outPath -Recurse -Force
}
New-Item -ItemType Directory -Path $outPath -Force | Out-Null

Copy-Item -LiteralPath $Binary -Destination (Join-Path $outPath 'OpenCADStudio.exe')

# Workspace files inherit a low mandatory integrity label when they are produced by the
# sandboxed build, and a low-integrity process cannot write outside its own directory:
# the wizard would then fail with "Access is denied" in Documents and the MCP automation
# bridge could not publish its session descriptor. Raise the shipped copy to Medium so a
# portable folder behaves like a normally downloaded program.
$portableExe = Join-Path $outPath 'OpenCADStudio.exe'
try {
    & icacls $portableExe /setintegritylevel Medium 2>&1 | Out-Null
} catch {
    Write-Warning "could not raise the integrity level of $portableExe : $_"
}

$library = Join-Path $root 'library'
if (Test-Path -LiteralPath $library) {
    Copy-Item -LiteralPath $library -Destination (Join-Path $outPath 'library') -Recurse
}
$samples = Join-Path $root 'samples'
if (Test-Path -LiteralPath $samples) {
    Copy-Item -LiteralPath $samples -Destination (Join-Path $outPath 'samples') -Recurse
}
$templates = Join-Path $root 'templates'
if (Test-Path -LiteralPath $templates) {
    Copy-Item -LiteralPath $templates -Destination (Join-Path $outPath 'templates') -Recurse
}
$quickstart = Join-Path $root 'docs\QUICKSTART.md'
if (Test-Path -LiteralPath $quickstart) {
    Copy-Item -LiteralPath $quickstart -Destination (Join-Path $outPath 'QUICKSTART.md')
}
$license = Join-Path $root 'LICENSE'
if (Test-Path -LiteralPath $license) {
    Copy-Item -LiteralPath $license -Destination (Join-Path $outPath 'LICENSE')
}

$version = ''
try {
    $version = (& $Binary --version 2>&1 | Select-Object -First 1)
} catch {
    $version = 'version unavailable'
}
Set-Content -LiteralPath (Join-Path $outPath 'VERSION.txt') -Value "$version`r`n`r`nPortable AryaCADElectrical. Run OpenCADStudio.exe, open the Electrical tab, and start with ELECHELP.`r`nThe sample project is in samples\demo-line; QUICKSTART.md is the guided tour." -Encoding ASCII

$files = Get-ChildItem -LiteralPath $outPath -Recurse -File
$sizeMb = [math]::Round((($files | Measure-Object -Property Length -Sum).Sum / 1MB), 1)
Write-Host "portable folder: $outPath"
Write-Host ("  {0} file(s), {1} MB" -f $files.Count, $sizeMb)
Write-Host "  $version"
