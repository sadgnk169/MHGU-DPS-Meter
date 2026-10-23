# Windows 入口；依赖和输出统一由 build.sh 管理。
param([switch]$Offline)
$ErrorActionPreference = "Stop"
$taskDevkit = if ($env:DEVKITPRO) { $env:DEVKITPRO } else { "C:/devkitPro" }
$taskBash = Join-Path $taskDevkit "msys2/usr/bin/bash.exe"
$taskBuildArgs = @(($PSScriptRoot + "/build.sh").Replace("\", "/"))
if ($Offline) { $taskBuildArgs += "--offline" }
& $taskBash @taskBuildArgs
if ($LASTEXITCODE -ne 0) { throw "NRO 构建失败，退出码：$LASTEXITCODE" }
