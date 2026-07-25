# 🎮 Cosy Game Service

> A small Rust microservice that wraps the [SteamGridDB](https://www.steamgriddb.com/) game database API for the **Cosy** platform, exposing a focused HTTP API for game search and artwork (assets).

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![CI](https://github.com/Magenta-Mause/Cosy-Game-Service/actions/workflows/ci.yml/badge.svg)](https://github.com/Magenta-Mause/Cosy-Game-Service/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Magenta-Mause/Cosy-Game-Service?label=release)](https://github.com/Magenta-Mause/Cosy-Game-Service/releases)
[![Rust](https://img.shields.io/badge/rust-2021_edition-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![actix-web](https://img.shields.io/badge/actix--web-4.x-000000.svg)](https://actix.rs/)
[![Status: maintenance](https://img.shields.io/badge/status-maintenance-yellow.svg)](#-project-status-legacy--maintenance-mode)

**Changelog:** see [Releases](https://github.com/Magenta-Mause/Cosy-Game-Service/releases).

---

## 🧊 Project status: legacy / maintenance mode

> [!IMPORTANT]
> **Game data for Cosy has moved to the [Cosy Template Service](https://github.com/Magenta-Mause/Cosy-Template-Service).**
> The Cosy backend now sources its game catalogue from that service's `GET /v3/games` endpoint (`cosy.templates-api.games-url`) and caches it locally. It no longer calls this service.

This repository is kept **for legacy support**: Cosy backends released before the `/v3/games` migration still resolve game search and artwork through the hosted instance of this service (`cosy.games-api.url`, default `https://cosy-game-api.jannekeipert.de`), and the internal deployment keeps that instance running for them.

What that means in practice:

| | |
|---|---|
| ✅ Still done | Security advisories, dependency updates, and fixes to keep the running instance healthy |
| ❌ Not done | New features, API additions, or behaviour changes |
| 🆕 New integrations | Use the [Cosy Template Service](https://github.com/Magenta-Mause/Cosy-Template-Service) instead — do not build against this API |
| 🗄️ End of life | This repository will be **archived** once no supported Cosy backend depends on it |

Everything below documents the service as it exists today, for the people still running or maintaining it.

---

## 📖 Overview

**Cosy** (Cost Optimised Server Yard) is a self-hostable platform for hosting and managing game servers. When Cosy displays a game — for example to pick a server template or render a nice header image — it needs consistent game metadata and artwork. Rather than talking to third-party game databases directly, Cosy delegated that to this service — a role now filled by the [Cosy Template Service](https://github.com/Magenta-Mause/Cosy-Template-Service) for current backends (see [Project status](#-project-status-legacy--maintenance-mode)).

**Cosy Game Service** is a thin, self-contained Rust microservice that:

- Wraps the [SteamGridDB](https://www.steamgriddb.com/) API behind a stable, minimal HTTP interface tailored to Cosy's needs.
- Normalises upstream responses into a small, predictable JSON envelope (`success`, `timestamp`, `data`).
- Optionally enriches game results with hero and logo artwork URLs, fetching them in parallel for speed.

Some functionality is adapted from the [`steamgriddb_api`](https://crates.io/crates/steamgriddb_api) crate by [@PhilipK](https://github.com/PhilipK).

### ✨ Key features

- 🔎 **Game search** by name or name fragment, with `limit`/`offset` pagination.
- 🖼️ **Asset (artwork) lookup** for a specific game by its ID.
- 🦸 **Optional hero/logo enrichment** on game results, fetched concurrently.
- 📦 **Container-first**: ships as a small multi-stage Docker image, with Docker Compose and Kubernetes (Argo) manifests included.
- ⚡ Built on [actix-web](https://actix.rs/) and [Tokio](https://tokio.rs/).

### 🔗 Related repositories

| Repository | Description |
|------------|-------------|
| [Cosy](https://github.com/Magenta-Mause/Cosy) | The main Cosy project / download repo |
| [Cosy-Backend](https://github.com/Magenta-Mause/Cosy-Backend) | Core orchestration engine (Spring Boot) |
| [Cosy-Template-Service](https://github.com/Magenta-Mause/Cosy-Template-Service) | **Successor for game data** — serves templates and the game catalogue (`/v3/games`) |
| [Cosy-Frontend](https://github.com/Magenta-Mause/Cosy-Frontend) | Web frontend |
| [Cosy-Docs](https://github.com/Magenta-Mause/Cosy-Docs) | Official Cosy documentation |

---

## 🚀 Getting Started

### 📋 Prerequisites

- **Rust** (stable toolchain) — the repo pins the `stable` channel via [`rust-toolchain.toml`](rust-toolchain.toml); [rustup](https://rustup.rs/) will install it automatically. The Docker build uses Rust `1.91.1`.
- **Cargo** — installed with Rust.
- **Docker** (with Compose) — optional, only needed for the container-based workflow.
- **A SteamGridDB API key** — create an account and generate a key at <https://www.steamgriddb.com/>.

### 📥 Installation

Clone the repository:

```bash
git clone https://github.com/Magenta-Mause/Cosy-Game-Service.git
cd Cosy-Game-Service
```

### ⚙️ Configuration

The service is configured entirely through environment variables.

| Variable | Required | Description |
|----------|----------|-------------|
| `COSY_GAMEAPI_SGDB_API_KEY` | ✅ Yes | Your SteamGridDB API key. Sent as a Bearer token to the upstream API. The service exits on startup if this is not set. |

Copy the provided example file and fill in your key:

```bash
cp .env.example docker/.env   # Compose reads .env from next to the compose file
```

> [!NOTE]
> The binary does **not** load `.env` itself — there is no `dotenv`/`dotenvy` dependency in [`Cargo.toml`](Cargo.toml), so `cargo run` will not pick the file up. It is read only by Docker Compose, and only from the `docker/` directory (Compose resolves `.env` relative to the compose file, which is its project directory). For a local `cargo run`, export the variable instead — see [Quick Start](#quick-start) below.

> The HTTP server binds to `0.0.0.0:8080` (this address and port are currently fixed in the source).

<a id="quick-start"></a>

### ▶️ Quick Start

**Option A — run locally with Cargo:**

```bash
export COSY_GAMEAPI_SGDB_API_KEY=your_key_here
cargo run
```

`cargo run` does not auto-load `.env`. Either `export` the variable as above, or source the file into your shell first:

```bash
set -a; source docker/.env; set +a
cargo run
```

**Option B — run with Docker Compose** (reads `COSY_GAMEAPI_SGDB_API_KEY` from your environment, or from `docker/.env`):

```bash
cd docker
COSY_GAMEAPI_SGDB_API_KEY=your_key_here docker compose up --build
```

Either way, the service listens on **<http://localhost:8080>**. A quick smoke test:

```bash
curl "http://localhost:8080/games?query=zelda"
```

You should receive a JSON object with `success: true` and a `data.games` array.

---

## 🗂️ Project structure

```
Cosy-Game-Service/
├── src/
│   ├── main.rs                    # Entrypoint: reads API key, binds 0.0.0.0:8080, registers routes
│   ├── lib.rs                     # Library root, re-exports
│   ├── global_state.rs            # Shared state: SteamGridDB + reqwest clients
│   ├── model/                     # Request/response models (Game, Asset, Response envelope, SteamGridDB DTOs)
│   ├── routes/                    # HTTP handlers (games, assets)
│   └── services/                  # SteamGridDB service layer
├── tests/                         # Integration tests
├── docker/                        # Dockerfile + docker-compose.yaml
├── argo/                          # Kubernetes manifests (deployment, service, ingress)
├── .github/workflows/             # CI, release, and issue-redirect workflows
├── Cargo.toml                     # Crate manifest (crate name: cosy-gameapi)
└── rust-toolchain.toml            # Pinned Rust toolchain
```

---

## 🔌 API documentation

All responses use a common JSON envelope. On success:

```jsonc
{ "success": true, "timestamp": 1753370000000, "data": { /* ... */ } }
```

On error:

```jsonc
{ "success": false, "timestamp": 1753370000000, "message": "..." }
```

`timestamp` is Unix epoch milliseconds, stamped on every response as it is serialised.

> [!NOTE]
> Requests rejected before a handler runs do **not** use this envelope. The service registers no custom error handlers, so actix's extractors answer with their own plain-text errors: `400 Bad Request` for missing or unparsable query parameters, and `404 Not Found` for a path segment that cannot be parsed (e.g. a non-numeric `game_id`).

### `GET /games` — search games

Search for games by name (or a fragment of a name).

| Query param | Type | Default | Description |
|-------------|------|---------|-------------|
| `query` | string | *(required)* | (Fragment of) the game name, e.g. `zel` for `zelda`. |
| `limit` | integer | `15` | Maximum number of results to return. |
| `offset` | integer | `0` | Number of results to skip. |
| `include_hero` | boolean | `false` | If `true`, attempt to attach a hero image URL to each result. |
| `include_logo` | boolean | `false` | If `true`, attempt to attach a logo image URL to each result. |

**`200 OK`**

```ts
{
  success: boolean,
  timestamp: number,
  data: {
    games: [
      { id: number, name: string, hero_url?: string, logo_url?: string },
      // ...
    ],
    is_final: boolean,
  },
}
```

**`400 Bad Request`** — returned if `query` is missing, or if `limit`/`offset`/`include_hero`/`include_logo` cannot be parsed into their types.
**`500 Internal Server Error`** — returned if the upstream search fails.

### `GET /game` — fetch a single game

Fetch one game by its SteamGridDB ID.

| Query param | Type | Default | Description |
|-------------|------|---------|-------------|
| `id` | integer | *(required)* | The SteamGridDB game ID. |
| `include_hero` | boolean | `false` | If `true`, attempt to attach a hero image URL. |
| `include_logo` | boolean | `false` | If `true`, attempt to attach a logo image URL. |

**`200 OK`** — a single game object (`{ id, name, hero_url?, logo_url? }`) inside the standard envelope.
**`400 Bad Request`** — returned if `id` is missing or is not a valid integer, or if `include_hero`/`include_logo` cannot be parsed.
**`404 Not Found`** — returned if the game cannot be fetched.

### `GET /assets/{game_id}` — fetch artwork for a game

Fetch assets (images) for a specific game by its ID.

| Parameter | In | Type | Default | Description |
|-----------|-----|------|---------|-------------|
| `game_id` | path | integer | *(required)* | The SteamGridDB game ID. |
| `limit` | query | integer | `15` | Maximum number of assets to return. |
| `offset` | query | integer | `0` | Number of assets to skip. |

**`200 OK`**

```ts
{
  success: boolean,
  timestamp: number,
  data: {
    assets: [
      { width: number, height: number, url: string },
      // ...
    ],
    is_final: boolean,
  },
}
```

**`400 Bad Request`** — returned if `limit`/`offset` cannot be parsed into their types.
**`404 Not Found`** — returned if `game_id` is not a valid integer.
**`500 Internal Server Error`** — returned if fetching assets fails.

---

<a id="development"></a>

## 🛠️ Development

### Available commands

| Command | Description |
|---------|-------------|
| `cargo run` | Build and run the service locally (requires `COSY_GAMEAPI_SGDB_API_KEY`). |
| `cargo build` | Compile in debug mode. |
| `cargo build --release` | Compile an optimised release binary. |
| `cargo test` | Run the integration test suite. |
| `cargo fmt --all -- --check` | Check formatting (as run in CI). |
| `cargo clippy --all-targets --all-features -- -D warnings` | Lint (as run in CI). |
| `docker compose -f docker/docker-compose.yaml up --build` | Build and run in a container. |

### Development workflow

1. Create a feature branch off `main`.
2. Make your changes and keep them formatted: `cargo fmt --all`.
3. Ensure lints pass: `cargo clippy --all-targets --all-features -- -D warnings`.
4. Run the tests: `cargo test`.
5. Open a pull request against `main`.

CI (see [`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs `fmt`, `clippy`, the test suite, and `cargo audit` (currently non-blocking — it runs as `cargo audit || true`, so advisories are reported but never fail the build) on every push and pull request, so running these locally first avoids surprises.

### Key dependencies

| Crate | Role |
|-------|------|
| [`actix-web`](https://crates.io/crates/actix-web) | HTTP server / routing framework |
| [`tokio`](https://crates.io/crates/tokio) | Async runtime |
| [`reqwest`](https://crates.io/crates/reqwest) | HTTP client for upstream calls |
| [`steamgriddb_api`](https://crates.io/crates/steamgriddb_api) | SteamGridDB API client |
| [`serde`](https://crates.io/crates/serde) / [`serde_json`](https://crates.io/crates/serde_json) | (De)serialisation |
| [`futures`](https://crates.io/crates/futures) | Concurrent enrichment of results |
| [`chrono`](https://crates.io/crates/chrono) | Timestamps |
| [`httpmock`](https://crates.io/crates/httpmock) | HTTP mocking in tests (dev only) |

---

## 🚢 Deployment

- **Docker** — a multi-stage [`Dockerfile`](docker/Dockerfile) produces a slim `debian:bookworm-slim` runtime image. A [`docker-compose.yaml`](docker/docker-compose.yaml) is provided for local/single-host use.
- **Kubernetes** — manifests under [`argo/`](argo/) define a `Deployment`, `Service`, and `Ingress` (namespace `cosy`).
- **Container image — note the naming drift.** The `argo/` manifests pin `ghcr.io/magenta-mause/cosy-gameapi:v0.0.6`; note the [release workflow](.github/workflows/release.yml) publishes to `ghcr.io/magenta-mause/cosy-game-service` instead (it sets `images: ghcr.io/${{ github.repository }}`) — the two names have drifted. A release build therefore pushes to a repository nothing deploys from, and the `cosy-gameapi` images that *are* deployed were not produced by that workflow.
- The API key is injected via the `COSY_GAMEAPI_SGDB_API_KEY` environment variable (sourced from a Kubernetes secret in the Argo manifests).
- **Legacy hosted instance** — `https://cosy-game-api.jannekeipert.de` is the deployment that pre-`/v3/games` Cosy backends still call. Its Argo `Application` lives in [Cosy-Internal-Deployment](https://github.com/Magenta-Mause/Cosy-Internal-Deployment) and is retained deliberately; see [Project status](#-project-status-legacy--maintenance-mode) before removing it.

---

## 📚 Documentation

Broader Cosy documentation lives in [**Cosy-Docs**](https://github.com/Magenta-Mause/Cosy-Docs).

---

## 🤝 Contributing

This repository is in [maintenance mode](#-project-status-legacy--maintenance-mode): bug fixes and dependency/security updates are welcome, new features are not — please take those to the [Cosy Template Service](https://github.com/Magenta-Mause/Cosy-Template-Service). Cosy's org-wide community health files and contribution guidelines live in the [**Magenta-Mause/.github**](https://github.com/Magenta-Mause/.github) repository.

- 🐛 **Report a bug**: open an [issue](https://github.com/Magenta-Mause/Cosy-Game-Service/issues).
- 🔧 **Development setup**: see [Getting Started](#-getting-started) and [Development](#development) above.

Before opening a PR, please run `cargo fmt`, `cargo clippy`, and `cargo test` locally.

---

## 📄 License

This project is licensed under the **MIT License** — see the [LICENSE](LICENSE) file for details.

---

## 💬 Contact / Support

- Open an [issue](https://github.com/Magenta-Mause/Cosy-Game-Service/issues) for questions, bugs, or feature requests.
- For anything relating to the wider platform, see the [Cosy](https://github.com/Magenta-Mause/Cosy) and [Cosy-Docs](https://github.com/Magenta-Mause/Cosy-Docs) repositories.

---

## 🙏 Acknowledgments

- [SteamGridDB](https://www.steamgriddb.com/) for the upstream game database and artwork.
- The [`steamgriddb_api`](https://crates.io/crates/steamgriddb_api) crate by [@PhilipK](https://github.com/PhilipK).
