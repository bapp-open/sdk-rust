# BAPP Auto API Client — Rust

Official Rust client for the [BAPP Auto API](https://www.bapp.ro). Provides a
simple, consistent interface for authentication, entity CRUD, and task execution.

## Getting Started

### 1. Install

Add to `Cargo.toml`:

```toml
[dependencies]
bapp-api-client = "0.3.0"
```

### 2. Create a client

```rust
use bapp_api_client::BappApiClient;

let client = BappApiClient::new().with_token("your-api-key");
```

### 3. Make your first request

```rust
// List with filters
let countries = client.list("core.country", Some(&[("page", "1")])).await?;

// Get by ID
let country = client.get("core.country", "42").await?;

// Create
let data = serde_json::json!({"name": "Romania", "code": "RO"});
let created = client.create("core.country", Some(&data)).await?;

// Patch (partial update)
let patch = serde_json::json!({"code": "RO"});
client.patch("core.country", "42", Some(&patch)).await?;

// Delete
client.delete("core.country", "42").await?;
```

## Authentication

The client supports **Token** (API key) and **Bearer** (JWT / OAuth) authentication.
Token auth already includes a tenant binding, so you don't need to specify `tenant` separately.

```rust
// Static API token (tenant is included in the token)
let client = BappApiClient::new().with_token("your-api-key");

// Bearer (JWT / OAuth)
let client = BappApiClient::new().with_bearer("eyJhbG...").with_tenant("1");
```

## Configuration

`tenant` and `app` can be changed at any time after construction:

```rust
client.tenant = Some("2".to_string());
client.app = "wms".to_string();
```

## API Reference

### Client options

| Option | Description | Default |
|--------|-------------|---------|
| `token` | Static API token (`Token <value>`) — includes tenant | — |
| `bearer` | Bearer / JWT token | — |
| `host` | API base URL | `https://panel.bapp.ro/api` |
| `tenant` | Tenant ID (`x-tenant-id` header) | `None` |
| `app` | App slug (`x-app-slug` header) | `"account"` |

### Methods

| Method | Description |
|--------|-------------|
| `me()` | Get current user profile |
| `get_app(app_slug)` | Get app configuration by slug |
| `list(content_type, **filters)` | List entities (paginated) |
| `get(content_type, id)` | Get a single entity |
| `create(content_type, data)` | Create an entity |
| `update(content_type, id, data)` | Full update (PUT) |
| `patch(content_type, id, data)` | Partial update (PATCH) |
| `delete(content_type, id)` | Delete an entity |
| `list_introspect(content_type)` | Get list view metadata |
| `detail_introspect(content_type)` | Get detail view metadata |
| `list_tasks()` | List available task codes |
| `detail_task(code)` | Get task configuration |
| `run_task(code, payload?)` | Execute a task |
| `run_task_async(code, payload?)` | Run a long-running task and poll until done |

### Paginated responses

`list()` returns the results directly as a list/array. Pagination metadata is
available as extra attributes:

- `count` — total number of items across all pages
- `next` — URL of the next page (or `null`)
- `previous` — URL of the previous page (or `null`)

## File Uploads

When data contains file objects, the client automatically switches from JSON to
`multipart/form-data`. Mix regular fields and files in the same call:

```rust
// Use request_multipart for file uploads
client.request_multipart(
    Method::POST,
    "/content-type/myapp.document/",
    &[("name", "Report")],        // text fields
    &[("file", "report.pdf")],    // file fields (field_name, file_path)
).await?;
```

## Tasks

Tasks are server-side actions identified by a dotted code (e.g. `myapp.export_report`).

```rust
let tasks = client.list_tasks().await?;

let cfg = client.detail_task("myapp.export_report").await?;

// Run without payload (GET)
let result = client.run_task("myapp.export_report", None).await?;

// Run with payload (POST)
let payload = serde_json::json!({"format": "csv"});
let result = client.run_task("myapp.export_report", Some(&payload)).await?;
```

### Long-running tasks

Some tasks run asynchronously on the server. When triggered, they return an `id`
that can be polled via `bapp_framework.taskdata`. Use `run_task_async()` to
handle this automatically — it polls until `finished` is `true` and returns the
final task data (which includes a `file` URL when the task produces a download).

## License

MIT
