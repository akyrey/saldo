/** Progress as a CSS width: basis points (0 to 10 000) as a percentage. */
export function progressWidth(bp: number): string {
  return `${Math.min(100, Math.max(0, bp / 100))}%`;
}
