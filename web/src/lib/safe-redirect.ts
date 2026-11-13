/** Accepts only same-site paths, so `?redirect=` cannot send people elsewhere. */
export function safeRedirect(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  return value.startsWith("/") && !value.startsWith("//") && !value.startsWith("/\\")
    ? value
    : undefined;
}
