#!/bin/bash
set -e

echo "Building and checking nanors project with clippy..."

# 设置编译选项
export RUSTFLAGS="-Z function-sections=yes -C link-arg=-fuse-ld=/usr/bin/mold -C link-args=-Wl,--gc-sections,--as-needed"

# 运行 clippy
cargo clippy --all-targets --all-features 2>&1 | tee /tmp/nanors_clippy.log

if [ $? -eq 0 ]; then
    echo ""
    echo "✅ Clippy check passed!"
else
    echo ""
    echo "❌ Clippy check failed!"
    exit 1
fi
