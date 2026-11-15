-- An optional colour per category, a name from the web app's palette
-- (saldo_app::categories::CATEGORY_COLORS), checked by the service.
ALTER TABLE categories ADD COLUMN color TEXT;
