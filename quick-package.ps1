# 快捷打包脚本：debug 档、免安装、跳过安装包与 updater 签名。
# 产物 src-tauri\target\debug\aurora-launcher.exe，并拷贝到仓库根 out-quick\ 便携目录，
# 首次运行会自行生成 tool/，双击即用，用于快速验证改动或发给他人自测。
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$out = Join-Path $root 'out-quick'
$exe = Join-Path $root 'src-tauri\target\debug\aurora-launcher.exe'

Write-Host '==> 构建前端并编译 debug 版 exe（--no-bundle，不生成安装包/签名）...'
Push-Location $root
try {
    pnpm exec tauri build --debug --no-bundle
    if ($LASTEXITCODE -ne 0) { throw "tauri build 失败（exit=$LASTEXITCODE）" }
}
finally {
    Pop-Location
}

if (-not (Test-Path -LiteralPath $exe)) { throw "未找到构建产物: $exe" }

Write-Host '==> 拷贝到便携目录 out-quick\ ...'
New-Item -ItemType Directory -Force -Path $out | Out-Null
Copy-Item -LiteralPath $exe -Destination (Join-Path $out 'aurora-launcher.exe') -Force

Write-Host ''
Write-Host "完成：$out\aurora-launcher.exe（tool/ 首次运行自动生成）" -ForegroundColor Green
