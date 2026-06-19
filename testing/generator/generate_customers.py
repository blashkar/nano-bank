"""nano-bank mock customer generator.

Continuously fabricates Canadian-flavoured customers with Faker and registers
them against the live nano-bank API (`POST /api/v1/customers`). This is the
*input* side of the test harness — run the Streamlit viewer to watch them land.

Config via env vars:
  API_BASE_URL     base URL of the API           (default http://localhost:8081)
  INTERVAL_SECONDS seconds between registrations  (default 3.0)
  COUNT            how many to create, 0 = forever (default 0)
  FAKER_LOCALE     Faker locale                   (default en_CA)
  REQUEST_TIMEOUT  per-request timeout, seconds    (default 10)
"""
from __future__ import annotations

import os
import random
import sys
import time
from datetime import date

import requests
from faker import Faker

API_BASE_URL = os.getenv("API_BASE_URL", "http://localhost:8081").rstrip("/")
INTERVAL_SECONDS = float(os.getenv("INTERVAL_SECONDS", "3.0"))
COUNT = int(os.getenv("COUNT", "0"))
FAKER_LOCALE = os.getenv("FAKER_LOCALE", "en_CA")
REQUEST_TIMEOUT = float(os.getenv("REQUEST_TIMEOUT", "10"))

CUSTOMERS_URL = f"{API_BASE_URL}/api/v1/customers"
HEALTH_URL = f"{API_BASE_URL}/health"

fake = Faker(FAKER_LOCALE)


def log(msg: str) -> None:
    """Timestamped, line-buffered stdout (so container logs stream live)."""
    print(f"{time.strftime('%H:%M:%S')}  {msg}", flush=True)


def random_sin() -> str:
    """A 9-digit string. The DB only checks the `^[0-9]{9}$` shape, not Luhn."""
    return "".join(str(random.randint(0, 9)) for _ in range(9))


def random_phone() -> str:
    """E.164-ish North-American number, 12 chars (API wants length 10–20)."""
    area = random.randint(200, 999)
    exch = random.randint(200, 999)
    line = random.randint(0, 9999)
    return f"+1{area}{exch}{line:04d}"


def make_customer() -> dict:
    first = fake.first_name()
    last = fake.last_name()
    # Unique-ish email: name + entropy, keeps the UNIQUE(email) constraint happy.
    email = f"{first}.{last}.{random.randint(1000, 9_999_999)}@example.com".lower()
    dob: date = fake.date_of_birth(minimum_age=18, maximum_age=90)
    return {
        "email": email,
        "phone_number": random_phone(),
        "first_name": first,
        "last_name": last,
        "date_of_birth": dob.isoformat(),
        "sin": random_sin(),
        "password": fake.password(length=12),
    }


def wait_for_api(retries: int = 30) -> None:
    for i in range(1, retries + 1):
        try:
            if requests.get(HEALTH_URL, timeout=REQUEST_TIMEOUT).ok:
                log(f"API healthy at {API_BASE_URL}")
                return
        except requests.RequestException:
            pass
        log(f"waiting for API ({i}/{retries}) …")
        time.sleep(2)
    log(f"⚠️  API never became healthy at {API_BASE_URL}; trying anyway")


def register_one(session: requests.Session) -> bool:
    """Create one customer; retry a few times on the rare duplicate (409)."""
    for _ in range(3):
        payload = make_customer()
        try:
            resp = session.post(CUSTOMERS_URL, json=payload, timeout=REQUEST_TIMEOUT)
        except requests.RequestException as e:
            log(f"✗ request failed: {e}")
            return False

        if resp.status_code == 201:
            data = resp.json()
            log(f"✓ created {data['first_name']} {data['last_name']} "
                f"<{data['email']}>  id={data['customer_id'][:8]}  kyc={data['kyc_status']}")
            return True
        if resp.status_code == 409:
            log("· duplicate, regenerating …")
            continue
        log(f"✗ {resp.status_code}: {resp.text[:200]}")
        return False
    log("✗ gave up after repeated duplicates")
    return False


def main() -> int:
    log(f"generator starting → {CUSTOMERS_URL}  "
        f"interval={INTERVAL_SECONDS}s  count={'∞' if COUNT == 0 else COUNT}  locale={FAKER_LOCALE}")
    wait_for_api()

    session = requests.Session()
    made = 0
    try:
        while COUNT == 0 or made < COUNT:
            if register_one(session):
                made += 1
            time.sleep(INTERVAL_SECONDS)
    except KeyboardInterrupt:
        log("interrupted")
    log(f"done — {made} customer(s) created")
    return 0


if __name__ == "__main__":
    sys.exit(main())
