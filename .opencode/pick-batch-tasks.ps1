$env:PATH += ";C:\Program Files\GitHub CLI"
$projectDir = "E:\github\atomic"
$tasksDir = "$projectDir\.opencode\tasks"

if (Test-Path $tasksDir) { Remove-Item $tasksDir -Recurse -Force }
New-Item -ItemType Directory -Path $tasksDir -Force | Out-Null

Set-Location $projectDir

$pickPrompt = @"
Read JS_ENGINE_CAPABILITY_MATRIX.md in this repository root. It lists roadmap items as markdown checkboxes: `- [ ] ...` means pending, `- [x] ...` or `~~item~~ — done` means already implemented.

Pick between 1 and 4 pending `- [ ]` items that can be safely implemented IN PARALLEL by separate agents, each on its own git branch, without conflicting: they must NOT touch the same file, NOT depend on each other's output, and ideally live in different crates. You decide how many — pick fewer (even just 1) if you can't find that many truly independent items; never force unrelated work together just to fill a number. Prioritize items in sections with high existing progress.

For EACH item you pick, create one file at .opencode\tasks\task-N.md (N = 1, 2, 3... one per item, use the Write tool to create each — these files do not exist yet) with this exact structure:

## Task
One sentence describing exactly what to implement.

## Files
List the specific file(s) that most likely need to change (based on the existing crate layout), one per line.

## Acceptance
One sentence: what test or behavior proves this is done.

Keep each task short and concrete — a junior engineer following only that one file, with no other context, should know exactly what to do.

You are running headless with no human on the other end. This is not a conversation — do not ask any question, do not write Rust code or diffs in your chat reply. Do NOT implement anything yourself — only write the task-N.md files via the Write tool. After creating all the files, your entire chat reply must be just a single line: DONE:N where N is how many task files you created (e.g. DONE:3).
"@

Write-Host "[$(Get-Date -Format o)] --- pick-batch-tasks: asking claude to pick independent parallel items ---"
claude -p $pickPrompt --verbose --allowedTools "Read,Grep,Glob,Write" --disallowedTools "Bash,NotebookEdit,Edit" 2>&1 | Write-Host

$taskFiles = Get-ChildItem -Path $tasksDir -Filter "task-*.md" -ErrorAction SilentlyContinue
if ($taskFiles.Count -gt 0) {
    Write-Output "OK:$($taskFiles.Count)"
} else {
    Write-Output "FAILED:0"
}
