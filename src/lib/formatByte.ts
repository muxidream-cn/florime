/**
 * 将字节数转换为更大单位（十进制进位，与参考项目一致）。
 * @param byte 字节数
 * @param type 目标单位；省略时按 KB 显示
 * @param withUnit 是否携带单位
 */
export function formatByte(byte: number, type?: "KB" | "MB" | "GB", withUnit = true): string {
  let num = "0"
  switch (type) {
    case "GB":
      num = (byte / 1_000_000_000).toFixed(0)
      break
    case "MB":
      num = (byte / 1_000_000).toFixed(0)
      break
    default:
      num = (byte / 1000).toFixed(0)
  }
  const unit = withUnit ? (type ?? "KB") : ""
  return `${num}${unit}`
}
