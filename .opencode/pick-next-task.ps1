$env:PATH += ";C:\Program Files\GitHub CLI"
$projectDir = "E:\github\nimble"
$taskFile = "$projectDir\.opencode\NEXT_TASK.md"

"## Task
(placeholder, to be filled in)

## Files

## Acceptance
" | Out-File -FilePath $taskFile -Encoding UTF8 -Force

Set-Location $projectDir

$pickPrompt = @"
Read JS_ENGINE_CAPABILITY_MATRIX.md in this repository root. It lists roadmap items as markdown checkboxes: `- [ ] ...` means pending, `- [x] ...` or `~~item~~ — done` means already implemented. Pick exactly ONE `- [ ]` item, preferring one in a section with high existing progress (build on what's already there) over starting a brand new area from scratch. Cross-check CLAUDE.md's "Implementation status" section too if useful for context on repo conventions, but the matrix file is the authoritative pending-items list.

Do NOT edit, create, or modify any source code file, any .rs file, or any file other than .opencode\NEXT_TASK.md. You have no code to write in this step. Your ONLY allowed write action is editing .opencode\NEXT_TASK.md via the Edit tool — it already exists with placeholder content, replace it.

You MUST actually call the Edit tool on .opencode\NEXT_TASK.md right now, in this same turn — do not just describe the task in your chat reply, that is a failure. The file must end up with this exact structure:

## Task
One sentence describing exactly what to implement.

## Files
List the specific file(s) that most likely need to change (based on the existing crate layout), one per line.

## Acceptance
One sentence: what test or behavior proves this is done.

Keep it short and concrete — a junior engineer following only this file, with no other context, should know exactly what to do. After the Edit tool call succeeds, your entire chat reply must be just the single word TASK_WRITTEN, nothing else.

You are running headless with no human on the other end. This is not a conversation — there is no next turn, no one will ever read or answer anything you say in your chat reply. Any question you ask ("want me to...?", "should I...?", "confirm?") gets no answer, ever, and counts as total failure of this run. Do NOT write any Rust code, diffs, or implementation detail in your chat reply. Do NOT ask any question. The ONLY correct behavior is: call the Edit tool on .opencode\NEXT_TASK.md immediately, then reply with the single word TASK_WRITTEN. Any other behavior — explaining your pick in prose, drafting code, asking what to do next — is a failure, even if the content itself is correct.
"@

Write-Host "[$(Get-Date -Format o)] --- pick-next-task: asking claude to pick a pending item ---"
claude -p $pickPrompt --verbose --allowedTools "Read,Grep,Glob,Edit,Write" --disallowedTools "Bash,NotebookEdit" 2>&1 | Write-Host

$content = Get-Content $taskFile -Raw -Encoding UTF8
if ($content -notmatch "\(placeholder") {
    Write-Output "OK"
    exit 0
}

$changedFiles = git status --short 2>$null | Where-Object { $_ -notmatch "\.opencode/" }
if ($changedFiles) {
    Write-Host "[$(Get-Date -Format o)] pick-next-task didn't write NEXT_TASK.md, but real source changes exist - treating as already-implemented"
    $fileList = ($changedFiles | ForEach-Object { ($_ -split "\s+", 3)[-1] }) -join "`n"
    "## Task
Verify and commit the already-written changes below. The code changes already exist in the working tree (uncommitted) - do NOT re-implement anything, just build, test, and commit what is already there.

## Files
$fileList

## Acceptance
``cargo build --workspace`` and ``cargo test --workspace`` both pass with these changes; then commit the code+tests as one commit and any doc update as a second commit.
" | Out-File -FilePath $taskFile -Encoding UTF8 -Force
    Write-Output "OK"
} else {
    Write-Output "FAILED"
}
