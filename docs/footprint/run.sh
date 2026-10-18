#!/bin/bash
# Home Assistant vs HomeMaid at idle, side by side in Docker on the same machine.
# Build first, from core/:
#   cargo build --release -p homemaid-bin
#   cargo build --release -p mock-plugin --target wasm32-wasip2
# Then: docs/footprint/run.sh   (about 6 minutes; HA's image is ~3.5 GB)
set -e
CORE=$(cd "$(dirname "$0")/../../core" && pwd)
B=$(mktemp -d)
mkdir -p "$B/ha" "$B/hm/plugins"
cp "$CORE/target/release/homemaid" "$B/hm/"
cp "$CORE/target/wasm32-wasip2/release/mock_plugin.wasm" "$B/hm/plugins/"
cat > "$B/hm/homemaid.toml" <<'TOML'
[core]
bind = "0.0.0.0:8123"
data_dir = "data"
token = "bench-core"

[agent]
bind = "0.0.0.0:8124"
token = "bench-agent"

[[plugin]]
id = "mock"
path = "plugins/mock_plugin.wasm"
permissions = ["local-network"]
TOML

docker rm -f ha-bench hm-bench >/dev/null 2>&1 || true
ready() { # name url: seconds until the API answers
  local t0=$(date +%s.%N)
  until curl -sf -o /dev/null "$2"; do sleep 0.2; done
  echo "$1 ready after $(echo "$(date +%s.%N) - $t0" | bc) s"
}
docker run -d --name ha-bench -p 18123:8123 -v "$B/ha:/config" ghcr.io/home-assistant/home-assistant:stable >/dev/null
ready "Home Assistant" http://localhost:18123/ &
docker run -d --name hm-bench -p 28123:8123 -v "$B/hm:/hm" -w /hm ubuntu:26.04 /hm/homemaid --core --agent --config /hm/homemaid.toml >/dev/null
ready "HomeMaid" http://localhost:28123/health
wait
sleep 30
echo "--- 30 samples, 10 s apart: name, cpu, memory"
for i in $(seq 1 30); do
  docker stats --no-stream --format '{{.Name}} {{.CPUPerc}} {{.MemUsage}}' ha-bench hm-bench
  sleep 8
done
docker image ls --format '{{.Repository}}:{{.Tag}} {{.Size}}' ghcr.io/home-assistant/home-assistant:stable
du -sh "$B/hm/homemaid" "$B/hm/plugins"
docker rm -f ha-bench hm-bench >/dev/null
