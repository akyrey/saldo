import { useQuery } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { NativeSelect } from "@/components/ui/native-select";
import { membersQuery } from "@/features/auth/queries";
import { parseScope } from "@/features/shared/scope";

/** Mine / a member / the whole household. The choice lives in `?scope=`. */
export function ScopeSwitch({ currentUserId }: { currentUserId: string }) {
  const { scope } = useSearch({ from: "/app" });
  const navigate = useNavigate();
  const members = useQuery(membersQuery);
  const others = (members.data ?? []).filter((m) => m.user_id !== currentUserId);

  return (
    <NativeSelect
      aria-label="Scope"
      className="h-8 w-auto"
      value={scope ?? "household"}
      onChange={(event) => {
        const next = parseScope(event.target.value);
        void navigate({
          to: ".",
          search: (prev) => ({ ...prev, scope: next === "household" ? undefined : next }),
        });
      }}
    >
      <option value="household">Household</option>
      <option value="mine">Mine</option>
      {others.map((member) => (
        <option key={member.user_id} value={`user:${member.user_id}`}>
          {member.display_name}
        </option>
      ))}
    </NativeSelect>
  );
}
