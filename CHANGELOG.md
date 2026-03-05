# Changelog

## 0.2.0

- Initial release.
- Authentication: Bearer (JWT/OAuth) and Token (API key).
- Entity CRUD: list, get, create, update, patch, delete.
- Paginated list responses with metadata (count, next, previous).
- Entity introspection: list_introspect, detail_introspect.
- Tasks: list, detail, run (sync and async with polling).
- Long-running task support via run_task_async with automatic polling.
- File uploads: automatic multipart/form-data detection.
