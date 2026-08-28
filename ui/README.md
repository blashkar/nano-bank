# Nano-Bank UI

A Next.js (App Router) frontend for the nano-bank API.

## Pages

- `/` — splash page.
- `/auth/signup`, `/auth/signin` — customer registration and login forms.
- `/dashboard` — protected; requires a valid session, otherwise redirects to `/auth/signin`. Shows a
  skeleton (`loading.tsx`) while the session check is in flight.
- `/dashboard/accounts` — list of the customer's accounts; `/accounts/create` opens the account
  creation form.
- `/dashboard/accounts/[id]` — single account's detail and balance, plus its most recent
  transactions (linking out to the full history page and to individual transactions).
- `/dashboard/accounts/[id]/transactions` — full, paginated transaction history for one account
  (10/25/50/100 per page) with a debounced description search box.
- `/dashboard/accounts/transfer` — move money between two of the customer's own accounts.
- `/dashboard/accounts/deposit` — deposit money into one of the customer's own accounts.
- `/dashboard/transactions/[id]` — entry-level detail for a single transaction; the back link
  returns to the originating account's transactions page when reached via `?account=`.
- `/dashboard/credit` — list of the customer's credit card accounts; `/credit/[id]` shows one
  card's detail.
- `/privacy`, `/terms` — static Privacy Policy / Terms of Service pages.
- `/health` — pings the API's `/health` endpoint.

`Header` and `Footer` (`src/components/`) are shared across pages. `Header` is
authentication-aware: it shows a "Sign In" link when signed out, or
"Dashboard" + "Log out" when a session is active.

## Auth

Sign-up, sign-in, logout, and token refresh are Next.js server actions
(`src/actions/auth.ts`) that call the API's `/api/v1/auth/*` and
`/api/v1/customers` endpoints directly — no auth logic lives in the browser.

- On sign-in, the API's `access_token` / `refresh_token` are stored as
  `httpOnly` cookies.
- `src/proxy.ts` is a cheap edge-level gate on `/dashboard/:path*`: it bounces
  requests with no `access_token` cookie at all before they reach the page.
  This is a presence check only, not authoritative.
- `/dashboard` verifies the `access_token` server-side against
  `GET /api/v1/customers/profile` on every load, redirecting to `/auth/signin`
  if it's missing or rejected.
- `/dashboard` decodes the access token's `exp` claim server-side
  (`src/lib/jwt.ts`) and passes it to `TokenCountdown`, a client component
  that ticks down the remaining lifetime every second. Once it hits zero, it
  calls `refreshSessionAction` to silently rotate in a new access/refresh
  pair. If the refresh token itself is invalid or expired, the user is sent
  back to `/auth/signin`.
- `logoutAction` calls `POST /api/v1/auth/logout` and clears both cookies.

## Config

Create a `.env` file in the root of the `ui` directory:

```bash
NEXT_PUBLIC_API_BASE_URL=http://localhost:8081
```

## Running
Note that Node ≥ 20.9.0 is required for running this app.

1. Ensure the API is running.
2. From within the `ui` directory:

```bash
npm install
npm run dev
```

3. Open browser at `http://localhost:3000`
4. Visit `/health` to confirm the API is reachable, or go to `/auth/signup` to create an account and sign in.

## Notes
1. Multi-tab is silently broken. If a user has the dashboard open in two tabs, whichever tab refreshes first invalidates the token the other tab is about to send, and that second tab gets bounced to sign-in even though the session is fine.

2. Tests: unit tests run with `npm test` (vitest); an end-to-end auth suite lives in `e2e/` and runs against a live stack via `../scripts/e2e-ui.sh` (needs Node 20).

3. Learn More on the main page is a dead link at present.

## Demo account

A ready-made customer with a realistic profile and **6 months of backdated
salary + expense history**, for showing the UI and the personal-manager agent.

```bash
./scripts/deploy-all.sh     # full stack up (Postgres + modern core + API + UI)
./scripts/demo-seed.sh      # seed the demo customer (idempotent, re-runnable)
```

- **Log in** at `http://localhost:3000` with **`demo@nano.bank` / `Demo-Pass-2026`**.
- **Talk to the agent:** `kubectl -n nano-bank port-forward svc/agent-console 8505:8505`,
  open `http://localhost:8505`, click **Seed demo**, pick **Jordan Demo**, and ask
  e.g. *"summarize my salary and spending over the last 6 months."*

The seeder posts every transaction through the bank API (so ledger invariants
hold) and then backdates the timestamps via `kubectl exec … psql` — direct SQL is
confined to this demo tool. See `docs/specs/2026-07-29-demo-account-seed-design.md`.
