#!/bin/bash
# Coverage improvement agent

# Run tests with coverage
echo "📊 Generating coverage report..."
DATABASE_URL="postgres://postgres:password@localhost:5432/rain_tracker_test" \
  cargo llvm-cov --lcov --output-path lcov.info --lib

# Trigger Claude with analysis prompt
cat <<'PROMPT'
🎯 **Test Coverage Improvement Task**

I've generated an lcov.info file. Please:

1. **Parse lcov.info** to identify:
   - Files with <80% line coverage
   - Uncovered functions/branches
   - Critical paths missing tests (error handling, edge cases)

2. **Prioritize** by:
   - Business logic criticality (services > repositories > models)
   - Complexity (functions with multiple branches)
   - Risk (error handling, database operations)

3. **For each priority item**:
   - Read the source file
   - Identify untested code paths
   - Write focused unit tests
   - Verify coverage improved

4. **Iterate** until overall coverage >80% or diminishing returns

Focus on meaningful tests, not just coverage numbers.
PROMPT
