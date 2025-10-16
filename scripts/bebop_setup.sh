#!/bin/bash
# 启动Bebop模拟器的便捷脚本

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BEBOP_DIR="$(dirname "$SCRIPT_DIR")"
BEBOP_BIN="$BEBOP_DIR/bebop/target/release/bebop"

# 检查是否已编译
if [ ! -f "$BEBOP_BIN" ]; then
    echo "Bebop binary not found. Building..."
    cd "$BEBOP_DIR/bebop"
    cargo build --release --bin bebop
    
    if [ $? -ne 0 ]; then
        echo "Error: Failed to build bebop"
        exit 1
    fi
    echo ""
fi

# 检查端口是否已被占用
if netstat -tuln 2>/dev/null | grep -q ":9999 "; then
    echo "Warning: Port 9999 is already in use"
    echo "Please stop the existing process or change SOCKET_PORT"
    exit 1
fi

echo "Starting Bebop simulator..."
echo "Listening on 127.0.0.1:9999"
echo "Press Ctrl+C to stop"
echo ""

# 运行模拟器
exec "$BEBOP_BIN"

