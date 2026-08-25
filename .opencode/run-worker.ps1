param(
    [Parameter(Mandatory=$true)][string]$TaskFile,
    [Parameter(Mandatory=$true)][string]$JobId
)

$env:PATH += ";C:\Program Files\GitHub CLI"
chcp 65001 | Out-Null
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$projectDir = "E:\github\nimble"
$workerPromptFile = "$projectDir\.opencode\worker-prompt.md"
$reviewScript = "$projectDir\.opencode\review-pr.ps1"
$branchName = "auto/" + (Get-Date -Format "yyyyMMdd-HHmmss") + "-$JobId"
$worktreePath = "E:\github\nimble-wt-$JobId"
$maxAttempts = 3

function Log($msg) {
    Write-Host "[$JobId][$(Get-Date -Format o)] $msg"
}

Set-Location $projectDir
git worktree remove --force $worktreePath 2>&1 | Out-Null
git branch -D $branchName 2>&1 | Out-Null

git worktree add -b $branchName $worktreePath main 2>&1 | Write-Host
if (-not (Test-Path $worktreePath)) {
    Log "failed to create worktree, aborting"
    Write-Output "FAILED"
    exit 1
}

Copy-Item $TaskFile "$worktreePath\.opencode-NEXT_TASK.md" -Force
New-Item -ItemType Directory -Path "$worktreePath\.opencode" -Force | Out-Null
Copy-Item "$worktreePath\.opencode-NEXT_TASK.md" "$worktreePath\.opencode\NEXT_TASK.md" -Force

$workerPrompt = Get-Content $workerPromptFile -Raw -Encoding UTF8

Set-Location $worktreePath

$attempt = 0
$done = $false
while (-not $done -and $attempt -lt $maxAttempts) {
    $attempt++
    Log "opencode iteration attempt $attempt/$maxAttempts"
    if ($attempt -eq 1) {
        opencode run --auto --model "opencode/big-pickle" $workerPrompt 2>&1 | ForEach-Object { Write-Host "[$JobId] $_" }
    } else {
        opencode run --auto --continue --model "opencode/big-pickle" $workerPrompt 2>&1 | ForEach-Object { Write-Host "[$JobId] $_" }
    }
    $ahead = [int](git rev-list --count main..HEAD 2>$null)
    Log "commits ahead of main: $ahead"
    if ($ahead -ge 1) { $done = $true }
}

Set-Location $projectDir

if (-not $done) {
    Log "worker made no progress after $maxAttempts attempts, abandoning branch $branchName"
    git worktree remove --force $worktreePath 2>&1 | Out-Null
    git branch -D $branchName 2>&1 | Out-Null
    Write-Output "FAILED"
    exit 1
}

git push -u origin $branchName 2>&1 | Write-Host
$existingPr = gh pr list --head $branchName --json number --jq ".[0].number" 2>&1
if (-not $existingPr -or $existingPr -eq "") {
    gh pr create --title "auto: $branchName" --body "Automated changes by parallel opencode agent (job $JobId). See commits for detail." --head $branchName 2>&1 | Write-Host
    $existingPr = gh pr list --head $branchName --json number --jq ".[0].number" 2>&1
}

if ($existingPr -and $existingPr -ne "") {
    Log "requesting review on PR #$existingPr"
    $reviewResult = & $reviewScript -PrNumber ([int]$existingPr)
    Log "review result: $reviewResult"
} else {
    Log "failed to create/find PR for $branchName"
}

git worktree remove --force $worktreePath 2>&1 | Out-Null
Write-Output "DONE:$branchName"
