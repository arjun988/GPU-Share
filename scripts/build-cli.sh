#!/bin/bash
export PATH="$HOME/.cargo/bin:$PATH"
cd /mnt/c/Users/Arjun/Desktop/GPU-Share
cargo build -p gpumesh-cli
echo EXIT:$?
