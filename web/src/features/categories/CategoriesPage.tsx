import { api, call } from "@/api/client";
import { NamedItemsPage, type NamedItemsConfig } from "@/features/shared/NamedItemsPage";
import { categoriesQuery, type Category } from "./queries";

const config: NamedItemsConfig<Category> = {
  title: "Categories",
  singular: "category",
  query: categoriesQuery,
  create: (name) => call(api.POST("/api/v1/categories", { body: { name } })),
  rename: (id, name) =>
    call(api.PATCH("/api/v1/categories/{id}", { params: { path: { id } }, body: { name } })),
  remove: (id) => call(api.DELETE("/api/v1/categories/{id}", { params: { path: { id } } })),
};

export function CategoriesPage() {
  return <NamedItemsPage config={config} />;
}
