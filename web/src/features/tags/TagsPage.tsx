import { Link } from "@tanstack/react-router";
import { api, call } from "@/api/client";
import { NamedItemsPage, type NamedItemsConfig } from "@/features/shared/NamedItemsPage";
import { mergeTags, tagsQuery, type Tag } from "./queries";

const config: NamedItemsConfig<Tag> = {
  title: "Tags",
  singular: "tag",
  query: tagsQuery,
  create: (name) => call(api.POST("/api/v1/tags", { body: { name } })),
  rename: (id, name) =>
    call(api.PATCH("/api/v1/tags/{id}", { params: { path: { id } }, body: { name } })),
  remove: (id) => call(api.DELETE("/api/v1/tags/{id}", { params: { path: { id } } })),
  merge: (sourceId, intoId) => mergeTags([{ into: intoId, sources: [sourceId] }]),
  actions: (
    <Link to="/tags/duplicates" className="text-sm hover:underline">
      Find case duplicates
    </Link>
  ),
};

export function TagsPage() {
  return <NamedItemsPage config={config} />;
}
