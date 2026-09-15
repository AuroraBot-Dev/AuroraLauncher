/// 把 `<工具> --version` 的原始首行规范成便于展示的版本号：
/// 去掉常见前缀（如 `Python `、`git version `、`uv `）与括号内的构建信息；
/// 拿不到数字（如 pnpm 的 .cmd 包装脚本不会被实际执行）时按安装状态兜底。
export function formatToolVersion(version: string, installed: boolean): string {
  const cleaned = version
    .trim()
    .replace(/^(git version|python|uv|pnpm)\s+/i, '')
    .replace(/\s*\([^)]*\)\s*$/, '')
    .trim()
  if (/\d/.test(cleaned)) return cleaned
  return installed ? '已安装（未探测到版本）' : '未安装'
}

/// 把字节/秒格式化成便于阅读的速度（如 `1.2 MB/s`）；0 或非法值返回空串。
export function formatSpeed(bytesPerSecond: number): string {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return ''
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytesPerSecond
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1
  return `${value.toFixed(digits)} ${units[unit]}/s`
}
