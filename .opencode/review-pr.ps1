param(
    [Parameter(Mandatory=$true)][int]$PrNumber
)

$env:PATH += ";C:\Program Files\GitHub CLI"
$projectDir = "E:\github\nimble"
$notesFile = "$projectDir\.opencode\REVIEW_NOTES.md"

if (Test-Path $notesFile) { Remove-Item $notesFile -Force }

Set-Location $projectDir

$reviewPrompt = @"
Review PR #$PrNumber in this repository (run 'gh pr diff $PrNumber' and 'gh pr view $PrNumber' to see the content). Apply the same rigor as the /code-review skill: correctness bugs, security, and whether the code actually matches what CLAUDE.md documents as this project's conventions (tests under crate/tests/*_test.rs, Conventional Commits, no abstraction beyond what's needed).

After reviewing, pick exactly ONE of these two actions:

1. If the PR is OK to merge: run 'gh pr merge $PrNumber --squash --delete-branch' and reply with just the word MERGED.

2. If it needs changes: do NOT merge. Write a file at .opencode\REVIEW_NOTES.md with an objective list of what needs fixing (file:line + the problem + what to do), and reply with just the word NEEDS_FIXES.

Do nothing else beyond this.
"@

Write-Host "[$(Get-Date -Format o)] --- review-pr: reviewing PR #$PrNumber ---"
$lines = claude -p $reviewPrompt --verbose --allowedTools "Read,Grep,Glob,Bash,Write" 2>&1 | ForEach-Object { Write-Host $_; $_ }

if (($lines -join "`n") -match "MERGED") {
    Write-Output "MERGED"
} else {
    Write-Output "NEEDS_FIXES"
}
