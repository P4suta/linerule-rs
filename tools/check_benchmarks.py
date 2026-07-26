# SPDX-FileCopyrightText: linerule-rs contributors <https://github.com/P4suta/linerule-rs>
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Reject statistically significant Criterion timing regressions."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("criterion_directory", type=Path)
    parser.add_argument("--maximum-percent", type=float, default=10.0)
    parser.add_argument("--minimum-comparisons", type=int, default=1)
    arguments = parser.parse_args()

    if arguments.maximum_percent <= 0.0:
        raise SystemExit("--maximum-percent must be positive")
    if arguments.minimum_comparisons <= 0:
        raise SystemExit("--minimum-comparisons must be positive")

    reports = sorted(arguments.criterion_directory.glob("**/change/estimates.json"))
    if len(reports) < arguments.minimum_comparisons:
        raise SystemExit(
            "Criterion produced "
            f"{len(reports)} comparisons; expected at least "
            f"{arguments.minimum_comparisons}"
        )

    maximum_ratio = arguments.maximum_percent / 100.0
    regressions: list[str] = []
    for report_path in reports:
        with report_path.open(encoding="utf-8") as stream:
            report = json.load(stream)
        try:
            estimate = report["mean"]
            point = float(estimate["point_estimate"])
            interval = estimate["confidence_interval"]
            lower = float(interval["lower_bound"])
            upper = float(interval["upper_bound"])
        except (KeyError, TypeError, ValueError) as error:
            raise SystemExit(f"invalid Criterion report {report_path}: {error}") from error
        if not all(math.isfinite(value) for value in (lower, point, upper)):
            raise SystemExit(f"non-finite Criterion estimate in {report_path}")

        benchmark = report_path.parent.parent.relative_to(
            arguments.criterion_directory
        )
        print(
            f"{benchmark}: mean change "
            f"{point * 100.0:+.2f}% "
            f"(95% CI {lower * 100.0:+.2f}%..{upper * 100.0:+.2f}%)"
        )

        # Criterion stores relative timing changes. A positive interval that
        # excludes zero is statistically significant; the point estimate must
        # also meet the release contract's 10% regression threshold.
        if point >= maximum_ratio and lower > 0.0:
            regressions.append(f"{benchmark} ({point * 100.0:+.2f}%)")

    if regressions:
        raise SystemExit(
            "significant benchmark regression(s) at or above "
            f"{arguments.maximum_percent:.2f}%: "
            + ", ".join(regressions)
        )


if __name__ == "__main__":
    main()
