/**
 * Font sizes as a person names them: the Chinese size names (初号 to 八号, as WPS and Word list
 * them) and points. A typed size is a name or a number of points (1 to 1638, halves kept).
 */
export const CHINESE_SIZES: Array<[string, number]> = [
  ['初号', 42],
  ['小初', 36],
  ['一号', 26],
  ['小一', 24],
  ['二号', 22],
  ['小二', 18],
  ['三号', 16],
  ['小三', 15],
  ['四号', 14],
  ['小四', 12],
  ['五号', 10.5],
  ['小五', 9],
  ['六号', 7.5],
  ['小六', 6.5],
  ['七号', 5.5],
  ['八号', 5],
]

/** The points `typed` names (a Chinese size name or a number), or null. */
export function sizeOf(typed: string): number | null {
  const text = typed.trim()
  const named = CHINESE_SIZES.find(([name]) => name === text)
  if (named) return named[1]
  const value = Number(text.replace(/\s*(pt|磅)$/i, ''))
  if (!Number.isFinite(value) || value < 1 || value > 1638) return null
  return Math.round(value * 2) / 2
}
