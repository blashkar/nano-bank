# nano-bank — Local Setup Guide

## Overview

nano-bank is an experimental Canadian challenger-bank API written in Rust (`axum`), backed by PostgreSQL 16 running inside a local Kubernetes cluster (Kind). It models customer onboarding, account opening, and a complete credit-card payment rail (authorize → capture → settle) using a double-entry ledger enforced by database triggers.

```
┌──────────────────────────────┐
│  Rust API  (axum)            │   http://localhost:8081
│  Decimal money · typed models│
└───────────────┬──────────────┘
                │  sqlx  (port-forward :5432)
┌───────────────▼──────────────┐
│  PostgreSQL 16               │   Kind cluster, namespace: nano-bank
│  double-entry bookkeeping    │   DDL loaded by init Job on first boot
└──────────────────────────────┘
```

| Service | URL |
|---------|-----|
| API | http://localhost:8081 |
| Health check | http://localhost:8081/health |
| HTML docs | http://localhost:8081/docs |
| PostgreSQL | localhost:5432 |

> **Note:** The `start-nano-bank.sh` script has a hardcoded path (`~/dev/nano-bank`) that won't work here. Follow the manual steps below instead.

---

## Part 1 — Install Prerequisites (one time)

### 1.1 Docker Desktop

```bash
brew install --cask docker
```

After installation, **open Docker from `/Applications/Docker.app`** and wait until the menu-bar whale icon shows "Docker Desktop is running." Every subsequent step requires Docker to be running first.

### 1.2 Kubernetes tools

```bash
brew install kind kubectl
```

- `kind` — runs a Kubernetes cluster inside Docker containers
- `kubectl` — CLI to interact with the cluster

### 1.3 Rust toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Follow the on-screen prompts (choose option 1 — default install). Then reload your shell:

```bash
source "$HOME/.cargo/env"
```

Verify:

```bash
cargo --version   # should print e.g. cargo 1.78.0
```

---

## Part 2 — One-Time Cluster & Database Setup

> Only needed the first time, or after running `stop-nano-bank.sh` (which deletes the cluster).

### 2.1 Create the Kind cluster

```bash
cd ~/SIG/nano-bank
kind create cluster --config k8s/kind-cluster-config.yaml
```

Expected output ends with: `Have a question, bug, or feature request? ...`

### 2.2 Deploy PostgreSQL and initialise the schema

```bash
cd ~/SIG/nano-bank
./k8s/deploy.sh
```

This script:
1. Creates the `nano-bank` Kubernetes namespace
2. Applies secrets, config maps, PVC, and the PostgreSQL deployment
3. Waits for the Postgres pod to become ready
4. Runs an init Job that executes all seven DDL scripts in order:
   `00_init.sql` → `01_enums.sql` → `02_customers.sql` → `03_accounts.sql` → `04_transactions.sql` → `05_security.sql` → `06_triggers.sql`

The script prints `Nano Bank PostgreSQL deployment complete!` when done (~2–3 min on first run).

Verify the pods are healthy:

```bash
kubectl get pods -n nano-bank
# NAME                        READY   STATUS      RESTARTS
# postgres-xxx                1/1     Running     0
# init-db-xxx                 0/1     Completed   0
```

---

## Part 3 — Starting the App (every session)

You need **two terminals** running simultaneously.

### Terminal 1 — Port-forward PostgreSQL

The API connects to Postgres at `localhost:5432`. Keep this running the whole time:

```bash
kubectl port-forward -n nano-bank svc/postgres-service 5432:5432
```

You'll see: `Forwarding from 127.0.0.1:5432 -> 5432`. Leave this terminal open.

### Terminal 2 — Start the API

```bash
cd ~/SIG/nano-bank/api
cargo run
```

First run compiles everything (1–2 min). Subsequent runs are faster. Ready when you see:

```
INFO nano_bank: Listening on 0.0.0.0:8081
```

### Verify everything is up

```bash
curl -s http://localhost:8081/health | jq
```

Expected response:

```json
{
  "status": "healthy",
  "services": {
    "database": "healthy",
    "api": "healthy"
  }
}
```

### Logs (optional third terminal)

```bash
tail -f /tmp/nano-bank-api.log          # API structured JSON logs
tail -f /tmp/nano-bank-port-forward.log  # port-forward events
```

---

## Part 4 — Playing with the API (Bruno Collection)

A Bruno collection lives in the `bruno/` folder. It has all requests pre-built and **automatically passes IDs between steps** — no manual copy/paste needed.

### Install Bruno

Download from https://www.usebruno.com or:

```bash
brew install --cask bruno
```

### Open the collection

1. Open Bruno
2. Click **Open Collection**
3. Navigate to `~/SIG/nano-bank/bruno` and select that folder

### Select the environment

In the top-right environment dropdown, select **local** (maps to `http://localhost:8081`).

### Run the full flow

Execute the requests in folder order — Bruno auto-chains the IDs:

| Step | Request | Auto-saves |
|------|---------|-----------|
| 1 | `1_Health / Health Check` | — |
| 2 | `2_Customers / Create Customer` | `customerId` |
| 3 | `3_Accounts / Open Account` | `accountId` |
| 4 | `4_Cards / Authorize Purchase` | `authId` |
| 5 | `4_Cards / Capture Authorization` | — |
| 6 | `4_Cards / Settle All Captured Purchases` | — |

Each request has a `docs` tab inside Bruno explaining what it does and what to expect.

---

## Part 4b — VS Code REST Client (alternative to Bruno)

A ready-made `nano-bank.http` file lives at the repo root. It runs all six requests in sequence and automatically chains IDs between them — no manual copying needed.

### Install the extension (one time)

```bash
code --install-extension humao.rest-client
```

Or search for **REST Client** by Huachao Mao in the VS Code Extensions panel.

### Open and run

```bash
code ~/SIG/nano-bank/nano-bank.http
```

Each request has a **Send Request** link that appears above it when you hover. Click it and the response opens in a split pane on the right.

Run them top to bottom in order:

| Step | Request | Chains into next via |
|------|---------|----------------------|
| 1 | Health Check | — |
| 2 | Create Customer | `createCustomer.response.body.customer_id` |
| 3 | Open Account | `openAccount.response.body.account_id` |
| 4 | Authorize Purchase | `authorize.response.body.auth_id` |
| 5 | Capture Authorization | — |
| 6 | Settle | — |

> Each request is named with `# @name` — the next request references the previous response directly using `{{requestName.response.body.field}}`, so as long as you run them in order within the same VS Code session, no manual ID copying is needed.

---

### Manual curl alternative (if you prefer the terminal)

The examples below chain together: create a customer → open an account → run a card transaction. Copy the IDs from each response into the next command.

### Health check

```bash
curl -s http://localhost:8081/health | jq
```


### HTML documentation

Open in a browser: http://localhost:8081/docs

---

### Create a customer

```bash
curl -s -X POST http://localhost:8081/api/v1/customers \
  -H "Content-Type: application/json" \
  -d '{
    "email": "jane.doe@example.com",
    "phone_number": "4161234567",
    "first_name": "Jane",
    "last_name": "Doe",
    "date_of_birth": "1990-05-15",
    "sin": "123456789",
    "password": "securepass123"
  }' | jq
```

Save the `customer_id` from the response:

```bash
CUSTOMER_ID="<paste customer_id here>"
```

**Validation rules enforced by the database:**
- `date_of_birth` — must be at least 18 years ago
- `sin` — exactly 9 digits (`^[0-9]{9}$`)
- `email` — must be unique
- `password` — minimum 8 characters

**Error codes:**
- `409 Conflict` — email already exists
- `400 Bad Request` — underage DOB or malformed SIN

---

### Open a credit card account

```bash
curl -s -X POST http://localhost:8081/api/v1/accounts \
  -H "Content-Type: application/json" \
  -d "{
    \"customer_id\": \"$CUSTOMER_ID\",
    \"account_type\": \"credit_card\"
  }" | jq
```

Opening terms applied automatically by account type:

| Type | Interest rate | Credit limit |
|------|---------------|--------------|
| `chequing` | 3.00% | $0 |
| `savings` | 0.00% | $0 |
| `credit_card` | 19.99% APR | $5,000.00 |

Save the `account_id`:

```bash
ACCOUNT_ID="<paste account_id here>"
```

---

### Authorize a purchase (place a hold)

```bash
curl -s -X POST http://localhost:8081/api/v1/cards/authorize \
  -H "Content-Type: application/json" \
  -d "{
    \"account_id\": \"$ACCOUNT_ID\",
    \"amount\": 99.99,
    \"merchant\": \"Tim Hortons\"
  }" | jq
```

**Approved response (201):**

```json
{
  "status": "approved",
  "auth_id": "...",
  "account_id": "...",
  "amount": 99.99,
  "merchant": "Tim Hortons",
  "available_balance": 4900.01,
  "reason": null
}
```

**Declined response (200)** — if balance insufficient:

```json
{
  "status": "declined",
  "auth_id": null,
  "reason": "insufficient_credit"
}
```

Save the `auth_id` from an approved response:

```bash
AUTH_ID="<paste auth_id here>"
```

---

### Capture the authorization (post to ledger)

This converts the hold into a real double-entry transaction:

```bash
curl -s -X POST http://localhost:8081/api/v1/cards/capture \
  -H "Content-Type: application/json" \
  -d "{\"auth_id\": \"$AUTH_ID\"}" | jq
```

Behind the scenes: inserts a `transaction` header and two `transaction_entries` (card account **credit** + internal VISA_CLEARING GL account **debit**). Database triggers validate that debits = credits before committing.

---

### Settle all captured purchases

Nets the VISA_CLEARING balance against the BANK_SETTLEMENT GL account in one batch:

```bash
curl -s -X POST http://localhost:8081/api/v1/cards/settle \
  -H "Content-Type: application/json" \
  -d '{}' | jq
```

**Response when there are transactions to settle (201):**

```json
{
  "status": "settled",
  "settled_transactions": 1,
  "net_amount": "99.99",
  "transaction_id": "..."
}
```

**Response when nothing to settle (200):**

```json
{
  "status": "nothing_to_settle",
  "settled_transactions": 0
}
```

---

### Stub endpoints (return "TODO")

The following routes are wired up but not yet implemented — they return a plain text string, not JSON:

| Endpoint | Status |
|----------|--------|
| `POST /api/v1/auth/login` | TODO |
| `POST /api/v1/auth/refresh` | TODO |
| `POST /api/v1/auth/logout` | TODO |
| `GET /api/v1/customers/profile` | TODO |
| `PUT /api/v1/customers/profile` | TODO |
| `POST /api/v1/customers/kyc/documents` | TODO |
| `GET /api/v1/accounts` | TODO |
| `GET /api/v1/accounts/:id` | TODO |
| `GET /api/v1/accounts/:id/balance` | TODO |
| `POST /api/v1/transactions/transfer` | TODO |
| `POST /api/v1/transactions/deposit` | TODO |
| `POST /api/v1/transactions/withdrawal` | TODO |
| `GET /api/v1/transactions` | TODO |
| `GET /api/v1/security/sessions` | TODO |
| `GET /api/v1/security/devices` | TODO |

---

## Part 5 — Direct Database Access

### Option A — via kubectl (no local psql needed, always works)

```bash
kubectl exec -it -n nano-bank deployment/postgres -- \
  psql -U nanobank_user -d nano_bank_db
```

### Option B — via local psql (requires port-forward running in Terminal 1)

If `psql` is not installed: `brew install libpq && brew link --force libpq`

```bash
psql -h localhost -p 5432 -U nanobank_user -d nano_bank_db
# password: secure_nano_password_2024!
```

---

### Inspect tables and columns

```sql
-- List all tables
\dt

-- Show all columns, types and constraints for a table
\d customers
\d accounts
\d transactions
\d transaction_entries
\d account_holds
\d account_limits

-- Full column listing across every table
SELECT table_name, column_name, data_type, is_nullable
FROM information_schema.columns
WHERE table_schema = 'public'
ORDER BY table_name, ordinal_position;

-- Row counts across all tables
SELECT relname AS table, n_live_tup AS rows
FROM pg_stat_user_tables
ORDER BY n_live_tup DESC;
```

### Query live data

```sql
-- See all customers
SELECT customer_id, email, first_name, last_name, kyc_status, created_at
FROM customers ORDER BY created_at DESC;

-- See all accounts with balances
SELECT account_id, account_number, account_type, balance, available_balance, status
FROM accounts ORDER BY created_at DESC;

-- See all transactions
SELECT transaction_id, reference_number, transaction_type, amount, status, created_at
FROM transactions ORDER BY created_at DESC;

-- See double-entry ledger entries
SELECT te.entry_type, te.amount, te.balance_before, te.balance_after, a.account_type
FROM transaction_entries te
JOIN accounts a ON te.account_id = a.account_id
ORDER BY te.created_at DESC;

-- See open card holds
SELECT hold_id, account_id, amount, reason, expires_at, released_at
FROM account_holds
WHERE released_at IS NULL;

-- See audit trail
SELECT entity_type, action, changed_at FROM audit_logs ORDER BY changed_at DESC LIMIT 20;
```

```sql
-- Exit psql
\q
```

---

## Part 6 — Reset Data (without destroying the cluster)

Use this when you hit a `409 Conflict` error (duplicate email or phone) and want a clean slate, or any time you want to start fresh without rebuilding the entire cluster.

Wipes all customers, accounts, and transactions via `TRUNCATE ... CASCADE`. The system GL accounts self-heal on the next card operation.

```bash
cd ~/SIG/nano-bank

# Preview what will be deleted — shows row counts, makes no changes
testing/cleanup.sh --dry-run

# Wipe all data
testing/cleanup.sh
```

To delete a single customer instead of wiping everything, connect to the DB and run:

```bash
kubectl exec -it -n nano-bank deployment/postgres -- \
  psql -U nanobank_user -d nano_bank_db \
  -c "DELETE FROM customers WHERE email = 'jane.doe@example.com';"
```

The `DELETE` cascades automatically to that customer's accounts, transactions, holds, and audit logs via the foreign key constraints.

---

## Part 7 — Stop Everything

```bash
cd ~/SIG/nano-bank
./stop-nano-bank.sh
```

This kills the API process, the port-forward, and **deletes the entire Kind cluster** (all data is lost). The next `kind create cluster` + `deploy.sh` run will start fresh.

To just stop the API without destroying the cluster, kill the `cargo run` process (Ctrl+C in Terminal 2) and the port-forward (Ctrl+C in Terminal 1).

---

## Part 8 — Troubleshooting

**`kind create cluster` fails immediately**
→ Docker Desktop is not running. Open Docker.app and wait for the menu-bar whale to show "running."

**`cargo run` exits with a database connection error**
→ The port-forward (Terminal 1) is not running or crashed. Restart it:
```bash
kubectl port-forward -n nano-bank svc/postgres-service 5432:5432
```

**`Address already in use` on port 5432**
→ Another process is using the port. Find and kill it:
```bash
lsof -ti :5432 | xargs kill -9
```

**`deploy.sh` init Job times out**
→ Check what the Job pod is doing:
```bash
kubectl logs -n nano-bank job/init-db
kubectl get pods -n nano-bank
```

**API returns `500 Internal Server Error` on `/health`**
→ Postgres pod is not ready yet. Check:
```bash
kubectl get pods -n nano-bank
kubectl logs -n nano-bank deployment/postgres
```

**`curl` returns connection refused on port 8081**
→ API is still compiling (`cargo run` takes 1–2 min on first run). Wait for the `Listening on 0.0.0.0:8081` log line.

**`kind` or `kubectl` not found after install**
→ Reload your shell or open a new terminal tab so Homebrew's PATH is active:
```bash
export PATH="/opt/homebrew/bin:$PATH"
```
