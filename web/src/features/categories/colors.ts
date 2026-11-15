/** Must match `saldo_app::categories::CATEGORY_COLORS` on the server. */
export const CATEGORY_COLORS = [
  "red",
  "orange",
  "amber",
  "lime",
  "green",
  "teal",
  "sky",
  "blue",
  "violet",
  "pink",
  "gray",
] as const;

export type CategoryColor = (typeof CATEGORY_COLORS)[number];

/** Spelled out in full so Tailwind finds every class (it cannot see built strings). */
const BACKGROUND: Record<CategoryColor, string> = {
  red: "bg-cat-red",
  orange: "bg-cat-orange",
  amber: "bg-cat-amber",
  lime: "bg-cat-lime",
  green: "bg-cat-green",
  teal: "bg-cat-teal",
  sky: "bg-cat-sky",
  blue: "bg-cat-blue",
  violet: "bg-cat-violet",
  pink: "bg-cat-pink",
  gray: "bg-cat-gray",
};

/** The background class for a stored colour, or null for none or an unknown name. */
export function colorClass(color: string | null | undefined): string | null {
  return color && color in BACKGROUND ? BACKGROUND[color as CategoryColor] : null;
}
