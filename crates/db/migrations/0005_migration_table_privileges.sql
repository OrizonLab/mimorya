-- The migration journal belongs to the owner role. The blanket grant of the
-- security migration also covered it: the application role must not see or
-- alter it.
REVOKE ALL ON TABLE _sqlx_migrations FROM mimorya_app;
