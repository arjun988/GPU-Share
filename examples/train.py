#!/usr/bin/env python3
"""Minimal workload for local / remote gpumesh run demos."""

import time

print("GPUMesh example workload starting")
for i in range(5):
    print(f"step {i}")
    time.sleep(0.2)
print("done")
