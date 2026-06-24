# nano-bank test harness

Three small containers for exercising the bank and watching it work:

- **`generator/`** — *input.* Fabricates Canadian-flavoured customers with
  [Faker] and registers them against the live API (`POST /api/v1/customers`),
  then opens accounts for each (`POST /api/v1/accounts`): always a chequing
  account, plus a savings account ~60 % of the time and a credit card ~40 % of
  the time. Chequing carries a 3 % return rate, savings 0 %, and credit cards a
  19.99 % APR with a $5,000 limit.
- **`visa/`** — *input (card rails).* A mock Visa network. Every couple of
  seconds it picks a random active credit-card account straight from Postgres,
  fabricates a Faker merchant, and drives the issuer endpoints
  (`POST /api/v1/cards/{authorize,capture,settle}`): **authorize** places a hold,
  **capture** posts the purchase as a balanced double-entry transaction, and a
  periodic **settle** runs the clearing/settlement batch between the internal GL
  accounts. See [Card rails model](#card-rails-model) below.
- **`viewer/`** — *output.* A Streamlit dashboard (port **8504**) that taps
  Postgres directly and shows activity live. The **Customers** tab has counts, a
  per-minute creation-rate chart, and a registration feed; the **Accounts** tab
  shows totals, a by-type breakdown, the chequing rate, an opening-rate chart,
  and a recent-accounts feed; the **Card transactions** tab shows captured
  purchases, hourly volume, unsettled count, open authorizations, a per-minute
  volume chart, and a merchant-level feed. The internal system/GL accounts are
  filtered out of every view.

```
 generator ──HTTP POST /customers,/accounts──▶
                                               ╲
 visa ──HTTP POST /cards/{authorize,capture,    ╲──▶ nano-bank API (:8081) ──▶ Postgres
              settle}──────────────────────────╱                                  ▲
 viewer (:8504) ──────────────read SQL─────────────────────────────────────────────┘
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

This builds all three images and runs them with podman **host networking**, so
they reach the host's API (`:8081`) and Postgres (`:5432`) directly. Then open
<http://localhost:8504>.

```bash
podman logs -f nano-bank-generator    # watch customers being created
podman logs -f nano-bank-visa         # watch card auth/capture/settle
podman logs -f nano-bank-viewer       # streamlit logs
./stop-testing.sh                      # tear the harness down
```

## Configuration

`run-testing.sh` honours these env vars (with defaults):

| var | default | meaning |
|-----|---------|---------|
| `INTERVAL_SECONDS` | `3` | delay between customer registrations |
| `COUNT` | `0` | how many to create (`0` = forever) |
| `SAVINGS_PROB` | `0.6` | chance a customer also opens a savings account |
| `CREDIT_CARD_PROB` | `0.4` | chance a customer also opens a credit card |
| `VISA_INTERVAL_SECONDS` | `2` | delay between card purchases the Visa sim drives |
| `SETTLE_INTERVAL_SECONDS` | `30` | how often the Visa sim runs a settlement batch |
| `CAPTURE_PROB` | `0.9` | chance an authorized purchase is captured (else left as an open auth) |
| `API_BASE_URL` | `http://localhost:8081` | API the generator + Visa sim target |
| `DB_HOST` / `DB_PORT` | `::1` / `5432` | Postgres the viewer + Visa sim read (IPv6 — see note above) |
| `VIEWER_PORT` | `8504` | Streamlit port |

Example — a quick burst of 50 customers, two per second:

```bash
COUNT=50 INTERVAL_SECONDS=0.5 ./run-testing.sh
```

## Cleanup

Wipe all test data for a clean slate:

```bash
./cleanup.sh              # stop input containers, then TRUNCATE customers (cascades
                          # to addresses, accounts, transactions, sessions, …)
./cleanup.sh --dry-run    # only print row counts, change nothing
./cleanup.sh --keep-generator   # don't stop the input containers first
```

`cleanup.sh` runs `psql` *inside* the Postgres pod via `kubectl exec`, so it
needs no host psql client — just a running cluster. It stops the input
containers (`nano-bank-generator` **and** `nano-bank-visa`) first so the wipe
sticks, then prints before/after counts for customers, accounts, and
transactions. Truncating `customers` also removes the internal system customer
and its GL accounts; the API re-creates them per-request, so the next card
operation self-heals. Restart the stopped containers afterwards with the
`podman start …` line the script prints (or re-run `./run-testing.sh`).

## Card rails model

The Visa sim talks to **issuer endpoints** on the API; the bank plays the card
issuer and keeps the books with proper double-entry:

- **authorize** → places a row in `account_holds` against the cardholder's
  credit-card account, shrinking its `available_balance`. Over-limit requests are
  declined (HTTP 200 with a decline reason), not errored.
- **capture** → posts one balanced transaction: the cardholder account is
  **credited** (debt up) and an internal **`VISA_CLEARING`** GL account is
  **debited**; the hold is released.
- **settle** → a batch that nets the clearing account against an internal
  **`BANK_SETTLEMENT`** GL account in one balanced transaction and tags the
  captured purchases as settled.

Both GL accounts belong to a synthetic system customer (`system@nano.bank`) and
are given a huge overdraft so their balances may go negative. They're identified
by `(system customer, account_type)` — `VISA_CLEARING` = `chequing`,
`BANK_SETTLEMENT` = `savings` — because the schema's
`trigger_generate_account_number` overwrites any account number on insert, so a
sentinel number can't be used. All postings insert **both legs in a single
multi-row `INSERT`** so the `trigger_validate_transaction_balance` AFTER trigger
sees a balanced (debits == credits) set; balance bookkeeping is left to
`trigger_update_account_balance`. The viewer filters this system customer out of
every tab.

## Run locally without containers

Both pieces are plain Python and work against a running API / port-forward:

```bash
# generator
pip install -r generator/requirements.txt
API_BASE_URL=http://localhost:8081 COUNT=10 python generator/generate_customers.py

# visa rails (needs some credit-card accounts to exist first)
pip install -r visa/requirements.txt
API_BASE_URL=http://localhost:8081 DB_HOST=::1 python visa/visa_simulator.py

# viewer
pip install -r viewer/requirements.txt
streamlit run viewer/app.py        # http://localhost:8501
```

[Faker]: https://faker.readthedocs.io/
