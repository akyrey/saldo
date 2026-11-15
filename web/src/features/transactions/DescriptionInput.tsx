import { useQuery } from "@tanstack/react-query";
import type { ComponentProps } from "react";
import { SuggestInput } from "@/components/SuggestInput";
import { useDebounced } from "@/lib/use-debounced";
import { descriptionSuggestions } from "./description-suggestions";
import { descriptionSearchQuery } from "./queries";

/** Fewer characters than this match too much to be useful. */
const MIN_QUERY_LENGTH = 2;
const DEBOUNCE_MS = 200;
const SUGGESTION_LIMIT = 8;

type DescriptionInputProps = Omit<ComponentProps<typeof SuggestInput>, "suggestions">;

/** The description field, suggesting descriptions used before. */
export function DescriptionInput(props: DescriptionInputProps) {
  const query = useDebounced(props.value.trim(), DEBOUNCE_MS);
  const enabled = query.length >= MIN_QUERY_LENGTH;
  const matches = useQuery({ ...descriptionSearchQuery(query), enabled });
  const suggestions =
    enabled && matches.data
      ? descriptionSuggestions(matches.data.data, props.value, SUGGESTION_LIMIT)
      : [];
  return <SuggestInput {...props} suggestions={suggestions} />;
}
