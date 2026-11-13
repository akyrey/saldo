/** Splits the event-type field (commas, spaces or new lines) into a list. */
export function parseEventTypes(text: string): string[] {
  const items = text
    .split(/[\s,]+/)
    .map((item) => item.trim())
    .filter((item) => item !== "");
  return [...new Set(items)];
}

/** The list as the form shows it. */
export function formatEventTypes(types: readonly string[]): string {
  return types.join(", ");
}

/** Patterns people pick most; shown as hints under the field. */
export const EVENT_TYPE_HINTS = ["*", "transaction.*", "account.*", "bill.*", "webhook.test"];
