#!/usr/bin/bash -e 
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

cd $SCRIPT_DIR && cargo llvm-cov --lcov --output-path ${SCRIPT_DIR}/../lcov.info