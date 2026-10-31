$ErrorActionPreference = 'Stop'
& python (Join-Path $PSScriptRoot 'build-mod.py')
if ($LASTEXITCODE -ne 0) { throw "WASM build failed with exit code $LASTEXITCODE" }
