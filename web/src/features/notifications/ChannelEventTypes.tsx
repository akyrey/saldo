import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, call } from "@/api/client";
import { errorMessage } from "@/api/errors";
import { Alert } from "@/components/ui/alert";
import { type Channel, notificationTypesQuery, toggleType } from "./queries";

/** One checkbox per notification type; each change saves at once. */
export function ChannelEventTypes({ channel }: { channel: Channel }) {
  const queryClient = useQueryClient();
  const types = useQuery(notificationTypesQuery);
  const save = useMutation({
    mutationFn: (event_types: string[]) =>
      call(
        api.PATCH("/api/v1/notification-channels/{id}", {
          params: { path: { id: channel.id } },
          body: { event_types },
        }),
      ),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["notification-channels"] }),
  });
  const order = types.data ?? [];
  return (
    <fieldset className="grid gap-1.5 sm:grid-cols-2">
      <legend className="mb-1 text-sm font-medium">Send me</legend>
      {order.map((type) => (
        <label key={type.type} className="flex items-start gap-2 text-sm" title={type.description}>
          <input
            type="checkbox"
            className="mt-0.5"
            checked={channel.event_types.includes(type.type)}
            disabled={save.isPending}
            onChange={(e) =>
              save.mutate(toggleType(channel.event_types, type.type, e.target.checked, order))
            }
          />
          <span>{type.label}</span>
        </label>
      ))}
      {save.error ? <Alert className="sm:col-span-2">{errorMessage(save.error)}</Alert> : null}
    </fieldset>
  );
}
