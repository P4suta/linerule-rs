# SPDX-FileCopyrightText: linerule-rs contributors <https://github.com/P4suta/linerule-rs>
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Fail when an llvm-cov summary metric is below its release threshold."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=Path)
    parser.add_argument("metric")
    parser.add_argument("minimum", type=float)
    arguments = parser.parse_args()

    with arguments.report.open(encoding="utf-8") as stream:
        report = json.load(stream)
    percent = report["data"][0]["totals"][arguments.metric]["percent"]
    print(f"{arguments.metric} coverage: {percent:.2f}%")
    if percent < arguments.minimum:
        raise SystemExit(
            f"{arguments.metric} coverage is below {arguments.minimum:.2f}%"
        )


if __name__ == "__main__":
    main()
