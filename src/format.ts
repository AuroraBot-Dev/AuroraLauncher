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
