#!/bin/bash
set -e

echo "Formatting nanors project..."

# 格式化 Rust 代码
echo "Running cargo fmt..."
cargo fmt

# 格式化所有 toml 文件
echo "Running taplo fmt..."
taplo fmt Cargo.toml */*.toml */*/*.toml

echo ""
echo "Building and checking nanors project with clippy..."

# 设置编译选项
export RUSTFLAGS="-Z function-sections=yes -C link-arg=-fuse-ld=/usr/bin/mold -C link-args=-Wl,--gc-sections,--as-needed"

# 运行 clippy
cargo clippy --all-targets --all-features 2>&1 | tee /tmp/nanors_clippy.log

if [ $? -eq 0 ]; then
    echo ""
    echo "✅ All checks passed!"
else
    echo ""
    echo "❌ Clippy check failed!"
    exit 1
fi
