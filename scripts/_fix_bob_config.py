from pathlib import Path

Path("/tmp/gpumesh-bob/.gpumesh").mkdir(parents=True, exist_ok=True)
Path("/tmp/gpumesh-bob/.gpumesh/config.toml").write_text(
    'node_name = "bob"\n'
    "listen_port = 47001\n"
    'default_image = "python:3.12-slim"\n'
    "max_concurrent_jobs = 1\n"
    "sharing_enabled = false\n"
)
print("wrote bob config")
