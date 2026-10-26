# Compila MVCreader en modo release y arma la carpeta dist\MVCreader lista para copiar.
# Requisitos: Rust (toolchain x86_64-pc-windows-gnu) y MinGW-w64 (WinLibs vía winget).
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

# MinGW-w64 completo: el que trae rustup no incluye el ensamblador que necesita dlltool.
$mingw = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Directory -Filter "BrechtSanders.WinLibs*" -ErrorAction SilentlyContinue |
    ForEach-Object { Join-Path $_.FullName "mingw64\bin" } | Where-Object { Test-Path "$_\as.exe" } | Select-Object -First 1
if (-not $mingw) {
    throw "No se encontró MinGW-w64. Instálalo con: winget install BrechtSanders.WinLibs.POSIX.UCRT"
}
$env:PATH = "$env:USERPROFILE\.cargo\bin;$mingw;$env:PATH"

# PDFium (versión que coincide con la feature pdfium_7881 de pdfium-render).
$pdfium = "vendor\pdfium\bin\pdfium.dll"
if (-not (Test-Path $pdfium)) {
    New-Item -ItemType Directory -Force "vendor\pdfium" | Out-Null
    $tgz = "vendor\pdfium-win-x64.tgz"
    Invoke-WebRequest "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F7881/pdfium-win-x64.tgz" -OutFile $tgz
    tar -xzf $tgz -C "vendor\pdfium"
    Remove-Item $tgz
}

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "Falló la compilación" }

New-Item -ItemType Directory -Force "dist\MVCreader" | Out-Null
Copy-Item "target\release\MVCreader.exe" "dist\MVCreader\MVCreader.exe" -Force
Copy-Item $pdfium "dist\MVCreader\pdfium.dll" -Force
Copy-Item "vendor\pdfium\LICENSE" "dist\MVCreader\LICENCIA-PDFIUM.txt" -Force
# Avisos de los componentes de terceros que lleva pdfium.dll (FreeType, ICU, libjpeg, etc.).
Copy-Item "vendor\pdfium\licenses" "dist\MVCreader\licencias-terceros" -Recurse -Force
Write-Host "Listo: dist\MVCreader"
