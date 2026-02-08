#!/bin/bash
set -e

echo "Auto-fixing clippy warnings..."

# 设置编译选项
export RUSTFLAGS="-Z function-sections=yes -C link-arg=-fuse-ld=/usr/bin/mold -C link-args=-Wl,--gc-sections,--as-needed"

# 运行 clippy --fix 自动修复警告
cargo clippy --fix --allow-dirty

echo ""
echo "✅ Auto-fix completed!"
echo ""
echo "Please review the changes with 'git diff'"
echo "Run './debug.sh' to verify fixes"
