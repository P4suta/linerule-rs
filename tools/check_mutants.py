# SPDX-FileCopyrightText: linerule-rs contributors <https://github.com/P4suta/linerule-rs>
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Fail unless cargo-mutants caught every viable mutant without timing out."""

from __future__ import annotations

import argparse
import collections
import json
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("outcomes", type=Path)
    arguments = parser.parse_args()

    with arguments.outcomes.open(encoding="utf-8") as stream:
        report = json.load(stream)

    outcomes = report.get("outcomes")
    if not isinstance(outcomes, list) or not outcomes:
        raise SystemExit("cargo-mutants outcomes are missing or empty")

    summaries = collections.Counter(outcome.get("summary") for outcome in outcomes)
    baseline = [outcome for outcome in outcomes if outcome.get("scenario") == "Baseline"]
    if len(baseline) != 1 or baseline[0].get("summary") != "Success":
        raise SystemExit("cargo-mutants baseline did not succeed exactly once")

    mutants = [outcome for outcome in outcomes if outcome.get("scenario") != "Baseline"]
    if not mutants:
        raise SystemExit("cargo-mutants did not evaluate any mutants")

    accepted = {"CaughtMutant", "Unviable"}
    failures = collections.Counter(
        outcome.get("summary")
        for outcome in mutants
        if outcome.get("summary") not in accepted
    )
    print(
        "mutation outcomes: "
        f"caught={summaries['CaughtMutant']} "
        f"unviable={summaries['Unviable']} "
        f"missed={summaries['MissedMutant']} "
        f"timeout={summaries['Timeout']}"
    )
    if failures:
        details = ", ".join(f"{name}={count}" for name, count in sorted(failures.items()))
        raise SystemExit(f"mutation gate failed: {details}")


if __name__ == "__main__":
    main()
