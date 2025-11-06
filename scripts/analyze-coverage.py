#!/usr/bin/env python3
"""
Analyze lcov.info coverage report and display results by file.

Usage:
    python3 scripts/analyze-coverage.py [--filter PATTERN]

Options:
    --filter PATTERN    Only show files matching this pattern (e.g., "excel_importer")
"""

import re
import sys

def parse_lcov(lcov_path='lcov.info'):
    """Parse lcov.info file and return coverage data."""
    with open(lcov_path, 'r') as f:
        content = f.read()

    records = content.split('end_of_record')
    coverage_data = []

    for record in records:
        if not record.strip():
            continue

        sf_match = re.search(r'^SF:(.+)$', record, re.MULTILINE)
        if not sf_match:
            continue

        filename = sf_match.group(1)

        # Only look at source files, not bins
        if '/bin/' in filename or not '/src/' in filename:
            continue

        lf_match = re.search(r'^LF:(\d+)$', record, re.MULTILINE)
        lh_match = re.search(r'^LH:(\d+)$', record, re.MULTILINE)

        if not lf_match or not lh_match:
            continue

        lf = int(lf_match.group(1))
        lh = int(lh_match.group(1))

        if lf == 0:
            continue

        coverage_pct = (lh / lf) * 100

        # Extract relative path
        if '/rain-tracker-service/' in filename:
            relpath = filename.split('/rain-tracker-service/', 1)[1]
        else:
            relpath = filename

        coverage_data.append((relpath, coverage_pct, lh, lf))

    return coverage_data

def main():
    filter_pattern = None
    if len(sys.argv) > 1:
        if sys.argv[1] == '--filter' and len(sys.argv) > 2:
            filter_pattern = sys.argv[2]

    coverage_data = parse_lcov()
    coverage_data.sort(key=lambda x: x[1])

    if filter_pattern:
        print(f"="*80)
        print(f"FILES MATCHING '{filter_pattern}':")
        print("="*80)
        filtered = [item for item in coverage_data if filter_pattern in item[0]]
        for filepath, pct, lh, lf in filtered:
            print(f"{filepath:60} {pct:6.1f}% ({lh:4}/{lf:4})")

        if not filtered:
            print(f"No files found matching '{filter_pattern}'")
    else:
        print("Files with lowest coverage:\n")
        for filepath, pct, lh, lf in coverage_data[:20]:
            print(f"{filepath:60} {pct:6.1f}% ({lh:4}/{lf:4})")

if __name__ == '__main__':
    main()
