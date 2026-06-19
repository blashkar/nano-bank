# nano-bank test harness

Two small containers for exercising the bank and watching it work:

- **`generator/`** — *input.* Fabricates Canadian-flavoured customers with
  [Faker] and registers them against the live API (`POST /api/v1/customers`).
- **`viewer/`** — *output.* A Streamlit dashboard (port **8504**) that taps
  Postgres directly and shows customer activity live — counts, a per-minute
  creation-rate chart, and a newest-first registration feed. Accounts and
  transactions get their own tabs as those handlers land.

```
 generator ──HTTP POST──▶  nano-bank API (:8081) ──▶  Postgres
                                                        ▲
 viewer (:8504) ──────────────read SQL──────────────────┘
```

The viewer reads the database rather than the API on purpose: it's an
observability tool that watches the source of truth, independent of which API
endpoints exist yet.

## Prerequisites

nano-bank itself must be running first:

```bash
cd ~/dev/nano-bank
./start-nano-bank.sh     # API on :8081, Postgres port-forward on :5432
```

> [!IMPORTANT]
> **Use the IPv6 loopback `::1` for the DB, not `127.0.0.1`.** The kind
> control-plane container publishes a stale `0.0.0.0:5432` (docker-proxy) that
> nothing serves, so IPv4 connections get reset
> (`server closed the connection unexpectedly`). The live data path is the
> `kubectl port-forward`, which binds `[::1]:5432`. That's why
> `api/config/default.toml` uses `host = "::1"` and the viewer defaults to
> `DB_HOST=::1`. To clear the dead IPv4 mapping you'd have to recreate the kind
> cluster.

## Run

```bash
cd ~/dev/nano-bank/testing
./run-testing.sh
```

This builds both images and runs them with podman **host networking**, so they
reach the host's API (`:8081`) and Postgres (`:5432`) directly. Then open
<http://localhost:8504>.

```bash
podman logs -f nano-bank-generator    # watch customers being created
podman logs -f nano-bank-viewer       # streamlit logs
./stop-testing.sh                      # tear the harness down
```

## Configuration

`run-testing.sh` honours these env vars (with defaults):

| var | default | meaning |
|-----|---------|---------|
| `INTERVAL_SECONDS` | `3` | delay between customer registrations |
| `COUNT` | `0` | how many to create (`0` = forever) |
| `API_BASE_URL` | `http://localhost:8081` | API the generator targets |
| `DB_HOST` / `DB_PORT` | `::1` / `5432` | Postgres the viewer reads (IPv6 — see note above) |
| `VIEWER_PORT` | `8504` | Streamlit port |

Example — a quick burst of 50 customers, two per second:

```bash
COUNT=50 INTERVAL_SECONDS=0.5 ./run-testing.sh
```

## Cleanup

Wipe all test data for a clean slate:

```bash
./cleanup.sh              # stop generator, then TRUNCATE customers (cascades to
                          # addresses, accounts, transactions, sessions, …)
./cleanup.sh --dry-run    # only print row counts, change nothing
./cleanup.sh --keep-generator   # don't stop the generator container first
```

`cleanup.sh` runs `psql` *inside* the Postgres pod via `kubectl exec`, so it
needs no host psql client — just a running cluster. It stops the generator first
so the wipe sticks, then prints before/after counts for customers, accounts, and
transactions. Restart the generator afterwards with
`podman start nano-bank-generator` (or re-run `./run-testing.sh`).

## Run locally without containers

Both pieces are plain Python and work against a running API / port-forward:

```bash
# generator
pip install -r generator/requirements.txt
API_BASE_URL=http://localhost:8081 COUNT=10 python generator/generate_customers.py

# viewer
pip install -r viewer/requirements.txt
streamlit run viewer/app.py        # http://localhost:8501
```

[Faker]: https://faker.readthedocs.io/
