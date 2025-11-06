#!/usr/bin/env python3
"""
Analyze lcov.info coverage report and display results by file.

Usage:
    python3 scripts/analyze-coverage.py [--filter PATTERN] [--uncovered]

Options:
    --filter PATTERN    Only show files matching this pattern (e.g., "excel_importer")
    --uncovered         Show uncovered line numbers for filtered files
"""

import re
import sys

def parse_lcov(lcov_path='lcov.info', include_lines=False):
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

        item = [relpath, coverage_pct, lh, lf]

        if include_lines:
            # Extract uncovered lines
            lines = re.findall(r'^DA:(\d+),(\d+)', record, re.MULTILINE)
            uncovered = [int(line_num) for line_num, hits in lines if int(hits) == 0]
            item.append(uncovered)

        coverage_data.append(tuple(item))

    return coverage_data

def format_line_ranges(line_numbers):
    """Format line numbers into ranges (e.g., '1-5, 7, 9-12')."""
    if not line_numbers:
        return "none"

    line_numbers = sorted(line_numbers)
    ranges = []
    start = line_numbers[0]
    end = line_numbers[0]

    for line in line_numbers[1:]:
        if line == end + 1:
            end = line
        else:
            if start == end:
                ranges.append(str(start))
            else:
                ranges.append(f"{start}-{end}")
            start = line
            end = line

    if start == end:
        ranges.append(str(start))
    else:
        ranges.append(f"{start}-{end}")

    return ", ".join(ranges)

def main():
    filter_pattern = None
    show_uncovered = False

    # Parse arguments
    i = 1
    while i < len(sys.argv):
        if sys.argv[i] == '--filter' and i + 1 < len(sys.argv):
            filter_pattern = sys.argv[i + 1]
            i += 2
        elif sys.argv[i] == '--uncovered':
            show_uncovered = True
            i += 1
        else:
            i += 1

    coverage_data = parse_lcov(include_lines=show_uncovered)
    coverage_data.sort(key=lambda x: x[1])

    if filter_pattern:
        print(f"="*80)
        print(f"FILES MATCHING '{filter_pattern}':")
        print("="*80)
        filtered = [item for item in coverage_data if filter_pattern in item[0]]

        for item in filtered:
            filepath, pct, lh, lf = item[:4]
            print(f"{filepath:60} {pct:6.1f}% ({lh:4}/{lf:4})")

            if show_uncovered and len(item) > 4:
                uncovered_lines = item[4]
                print(f"  Uncovered lines: {format_line_ranges(uncovered_lines)}")
                print()

        if not filtered:
            print(f"No files found matching '{filter_pattern}'")
    else:
        print("Files with lowest coverage:\n")
        for item in coverage_data[:20]:
            filepath, pct, lh, lf = item[:4]
            print(f"{filepath:60} {pct:6.1f}% ({lh:4}/{lf:4})")

if __name__ == '__main__':
    main()
