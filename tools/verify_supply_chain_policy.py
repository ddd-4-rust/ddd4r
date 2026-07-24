#!/usr/bin/env python3
"""Fail when the time-bounded RustSec exception or CI policy drifts."""

from __future__ import annotations

import datetime as dt
import os
import sys
import tomllib
from pathlib import Path


ALLOWED_ADVISORIES = {
    "RUSTSEC-2024-0436": dt.date(2026, 9, 30),
    "RUSTSEC-2025-0134": dt.date(2026, 9, 30),
    "RUSTSEC-2026-0049": dt.date(2026, 9, 30),
    "RUSTSEC-2026-0098": dt.date(2026, 9, 30),
    "RUSTSEC-2026-0099": dt.date(2026, 9, 30),
    "RUSTSEC-2026-0104": dt.date(2026, 9, 30),
}
AUDIT_COMMAND = (
    "cargo audit --deny warnings"
    " --ignore RUSTSEC-2024-0436"
    " --ignore RUSTSEC-2025-0134"
    " --ignore RUSTSEC-2026-0049"
    " --ignore RUSTSEC-2026-0098"
    " --ignore RUSTSEC-2026-0099"
    " --ignore RUSTSEC-2026-0104"
)


def current_date() -> dt.date:
    epoch = os.environ.get("SOURCE_DATE_EPOCH")
    if epoch is not None:
        return dt.datetime.fromtimestamp(int(epoch), tz=dt.timezone.utc).date()
    return dt.datetime.now(tz=dt.timezone.utc).date()


def main() -> int:
    root = Path(__file__).resolve().parent.parent
    deny = tomllib.loads((root / "deny.toml").read_text(encoding="utf-8"))
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    ignored = {
        item["id"] for item in deny.get("advisories", {}).get("ignore", [])
    }
    expected = set(ALLOWED_ADVISORIES)
    if ignored != expected:
        print(
            f"deny.toml advisory exceptions differ: expected {sorted(expected)}, found {sorted(ignored)}",
            file=sys.stderr,
        )
        return 1

    today = current_date()
    expired = [
        advisory
        for advisory, deadline in ALLOWED_ADVISORIES.items()
        if today > deadline
    ]
    if expired:
        print(f"expired advisory exceptions: {expired}", file=sys.stderr)
        return 1

    version = manifest["workspace"]["package"]["version"]
    if not version.startswith("0."):
        print(
            f"advisory exceptions are forbidden for release line {version}",
            file=sys.stderr,
        )
        return 1

    for workflow in ("ci.yml", "release.yml"):
        content = (root / ".github" / "workflows" / workflow).read_text(
            encoding="utf-8"
        )
        if AUDIT_COMMAND not in content:
            print(f"{workflow} does not enforce the accepted audit command", file=sys.stderr)
            return 1

    print(
        f"supply-chain policy valid through {min(ALLOWED_ADVISORIES.values()).isoformat()}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
