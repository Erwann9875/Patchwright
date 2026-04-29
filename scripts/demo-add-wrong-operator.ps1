$ErrorActionPreference = "Stop"

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Program,
        [string[]] $Arguments
    )

    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "cargo was not found on PATH. Install Rust or add C:\Users\$env:USERNAME\.cargo\bin to PATH."
}

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Error "git was not found on PATH."
}

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$Src = Join-Path $Root "fixtures\rust\add_wrong_operator"
$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("patchwright-add-wrong-operator." + [System.IO.Path]::GetRandomFileName())

New-Item -ItemType Directory -Path $Tmp | Out-Null
Copy-Item -Path (Join-Path $Src "*") -Destination $Tmp -Recurse -Force

Invoke-Checked "git" @("-C", $Tmp, "init", "-q")
Invoke-Checked "git" @("-C", $Tmp, "config", "user.email", "patchwright@example.invalid")
Invoke-Checked "git" @("-C", $Tmp, "config", "user.name", "Patchwright Demo")
Invoke-Checked "git" @("-C", $Tmp, "add", ".")
Invoke-Checked "git" @("-C", $Tmp, "commit", "-qm", "seed broken add fixture")

Write-Host "Fixture repo: $Tmp"
Write-Host ""
Write-Host "Before Patchwright:"
& cargo test --manifest-path (Join-Path $Tmp "Cargo.toml")
if ($LASTEXITCODE -eq 0) {
    Write-Error "fixture unexpectedly passed before Patchwright"
}

Write-Host ""
Write-Host "Running Patchwright:"
$Task = Get-Content -Raw (Join-Path $Tmp "TASK.md")
Invoke-Checked "cargo" @("run", "-p", "patchwright-cli", "--", "solve", "--repo", $Tmp, "--task", $Task, "--model-provider", "codex-cli", "--max-steps", "12")

Write-Host ""
Write-Host "After Patchwright:"
Invoke-Checked "cargo" @("test", "--manifest-path", (Join-Path $Tmp "Cargo.toml"))
