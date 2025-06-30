#!/bin/bash
set -e
NEW=$(lcov --summary lcov.info | grep -o 'lines.*: [0-9.]*%.*' | awk '{print $2}')
echo "current coverage: $NEW%"
if [ -f baseline/lcov.info ]; then
  BASE=$(lcov --summary baseline/lcov.info | grep -o 'lines.*: [0-9.]*%.*' | awk '{print $2}')
  echo "base coverage: $BASE%"
  DELTA=$(python3 - <<PY
import sys
base=float(sys.argv[1].strip('%'))
new=float(sys.argv[2].strip('%'))
print(f"coverage change: {new-base:+.2f}%")
PY
"$BASE" "$NEW")
  echo "$DELTA"
else
  echo "no base coverage to compare"
fi
