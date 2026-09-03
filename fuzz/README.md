# Fuzzing

The trace JSON parser and sccache log parser are fuzz targets. Use a bounded
run with a measurable termination condition:

    cargo fuzz run trace -- -max_total_time=60

Record the command, duration, corpus, execution count, and result under the
private `qa/evidence/` directory. A timeout or resource limit is a test
boundary, not a passing assertion about all possible inputs.
