$ErrorActionPreference = "Continue"
$env:PATH += ";C:\Program Files\GitHub CLI"
chcp 65001 | Out-Null
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
[Console]::InputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

$projectDir = "E:\github\atomic"
$tasksDir = "$projectDir\.opencode\tasks"
$stopFile = "$projectDir\.opencode\STOP"
$pickBatchScript = "$projectDir\.opencode\pick-batch-tasks.ps1"
$runWorkerScript = "$projectDir\.opencode\run-worker.ps1"
$maxParallel = 4

Set-Location $projectDir

function Log($msg) {
    Write-Host "[$(Get-Date -Format o)] $msg"
}

Log "autonomous loop started (parallel, agent-decided batch size, capped at $maxParallel)"

while ($true) {
    if (Test-Path $stopFile) {
        Log "STOP file found, exiting loop"
        break
    }

    Log "picking a batch of independent tasks"
    $pickResult = & $pickBatchScript
    Log "pick-batch-tasks result: $pickResult"

    if ($pickResult -notmatch "^OK:(\d+)$") {
        Log "failed to pick any task, retrying in 30s"
        Start-Sleep -Seconds 30
        continue
    }

    $taskFiles = Get-ChildItem -Path $tasksDir -Filter "task-*.md" -ErrorAction SilentlyContinue | Sort-Object Name
    if ($taskFiles.Count -eq 0) {
        Log "no task files found despite OK result, retrying in 30s"
        Start-Sleep -Seconds 30
        continue
    }
    if ($taskFiles.Count -gt $maxParallel) {
        Log "agent picked $($taskFiles.Count) tasks, capping at $maxParallel"
        $taskFiles = $taskFiles | Select-Object -First $maxParallel
    }

    Log "launching $($taskFiles.Count) parallel worker job(s)"
    $jobs = @()
    $idx = 0
    foreach ($tf in $taskFiles) {
        $idx++
        $jobId = "job$idx"
        $job = Start-Job -ScriptBlock {
            param($script, $taskFile, $jobId)
            & $script -TaskFile $taskFile -JobId $jobId
        } -ArgumentList $runWorkerScript, $tf.FullName, $jobId
        $jobs += [PSCustomObject]@{ Job = $job; JobId = $jobId; Seen = 0 }
    }

    while ($jobs | Where-Object { $_.Job.State -eq "Running" }) {
        foreach ($j in $jobs) {
            $output = Receive-Job -Job $j.Job -Keep
            if ($output.Count -gt $j.Seen) {
                $output[$j.Seen..($output.Count - 1)] | ForEach-Object { Write-Host $_ }
                $j.Seen = $output.Count
            }
        }
        Start-Sleep -Seconds 3
    }

    foreach ($j in $jobs) {
        $output = Receive-Job -Job $j.Job -Keep
        if ($output.Count -gt $j.Seen) {
            $output[$j.Seen..($output.Count - 1)] | ForEach-Object { Write-Host $_ }
        }
        Remove-Job -Job $j.Job -Force
    }

    Log "batch complete"
    Start-Sleep -Seconds 5
}
