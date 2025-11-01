# Testing Guide for UDP Director

This document describes how to test the UDP Director locally and in a Kubernetes environment.

## Unit Tests

Run all unit tests:

```bash
cargo test
```

Run tests with output:

```bash
cargo test -- --nocapture
```

Run specific test:

```bash
cargo test test_token_generation_and_lookup
```

## Local Development Testing

### Prerequisites

- Kubernetes cluster (kind, minikube, or k3s recommended for local testing)
- kubectl configured
- Cilium CNI installed

### Setup Local Cluster with kind

```bash
# Create a kind cluster
kind create cluster --name udp-director-test

# Install Cilium
cilium install

# Verify Cilium is running
cilium status
```

### Deploy Test Resources

Create a test namespace and mock game server:

```yaml
# test-resources.yaml
apiVersion: v1
kind: Namespace
metadata:
  name: game-servers
---
apiVersion: v1
kind: Pod
metadata:
  name: test-gameserver-1
  namespace: game-servers
  labels:
    agones.dev/gameserver: test-gameserver-1
    game.example.com/map: de_dust2
spec:
  containers:
  - name: game
    image: nginx:alpine
    ports:
    - containerPort: 7777
      protocol: UDP
---
apiVersion: v1
kind: Service
metadata:
  name: test-gameserver-1
  namespace: game-servers
  labels:
    agones.dev/gameserver: test-gameserver-1
spec:
  selector:
    agones.dev/gameserver: test-gameserver-1
  ports:
  - name: default
    port: 7777
    protocol: UDP
```

Apply:

```bash
kubectl apply -f test-resources.yaml
```

### Deploy UDP Director

```bash
# Build and load image into kind
docker build -t udp-director:latest .
kind load docker-image udp-director:latest --name udp-director-test

# Deploy
kubectl apply -f k8s/rbac.yaml
# Use the games configmap for testing with Agones
kubectl apply -f k8s/configmap-games.yaml
kubectl apply -f k8s/deployment.yaml

# Wait for pod to be ready
kubectl wait --for=condition=ready pod -l app=udp-director -n udp-director --timeout=60s
```

### Test Query Server

Port-forward the query server:

```bash
kubectl port-forward -n udp-director svc/udp-director 9000:9000
```

In another terminal, test the query:

```bash
# Using curl with JSON
echo '{
  "resourceType": "gameserver",
  "namespace": "game-servers",
  "labelSelector": {
    "game.example.com/map": "de_dust2"
  }
}' | nc localhost 9000
```

Expected response:

```json
{"token":"550e8400-e29b-41d4-a716-446655440000"}
```

### Test Data Proxy

Port-forward the data port:

```bash
kubectl port-forward -n udp-director svc/udp-director 7777:7777
```

Test with netcat:

```bash
# Send token (replace with actual token from query)
echo "550e8400-e29b-41d4-a716-446655440000" | nc -u localhost 7777

# Send game data
echo "PLAYER_MOVE x:100 y:200" | nc -u localhost 7777
```

### Test Session Reset

```bash
# Get a new token
TOKEN_B=$(echo '{"resourceType":"gameserver","namespace":"game-servers"}' | nc localhost 9000 | jq -r '.token')

# Create control packet (magic bytes + token)
# Magic bytes: FFFFFFFF5245534554
echo -n -e "\xFF\xFF\xFF\xFF\x52\x45\x53\x45\x54${TOKEN_B}" | nc -u localhost 7777
```

## Integration Tests

### Using the Example Client

Run the example client (requires UDP Director running):

```bash
cargo run --example client_example
```

This will:
1. Query for a game server
2. Establish a UDP session
3. Send game data
4. Reset to a new server
5. Continue sending data

## Load Testing

### Using vegeta for Query Server

```bash
# Install vegeta
go install github.com/tsenart/vegeta@latest

# Create target file
cat > targets.txt << EOF
POST http://localhost:9000
Content-Type: application/json
@query.json
EOF

# Create query file
cat > query.json << EOF
{
  "resourceType": "gameserver",
  "namespace": "game-servers"
}
EOF

# Run load test
echo "POST http://localhost:9000" | vegeta attack -rate=100 -duration=10s -body=query.json | vegeta report
```

### UDP Load Testing

Use `iperf3` for UDP throughput testing:

```bash
# Server side (in a test pod)
kubectl run iperf-server --image=networkstatic/iperf3 -- -s -p 7777

# Client side
iperf3 -c <udp-director-ip> -u -p 7777 -b 10M -t 30
```

## Debugging

### View Logs

```bash
# Follow logs
kubectl logs -n udp-director -l app=udp-director -f

# View logs with debug level
kubectl set env deployment/udp-director -n udp-director RUST_LOG=udp_director=debug
```

### Check Session State

Add debug endpoints (future enhancement) or inspect logs for session information.

### Network Debugging

```bash
# Check connectivity to query port
nc -zv <udp-director-ip> 9000

# Check UDP port
nc -zuv <udp-director-ip> 7777

# Capture packets
kubectl exec -n udp-director <pod-name> -- tcpdump -i any -n port 7777
```

## Performance Benchmarks

### Expected Performance

- **Query Latency**: < 10ms (depends on K8s API latency)
- **Proxy Latency**: < 1ms added latency
- **Throughput**: > 10,000 packets/second per instance
- **Concurrent Sessions**: > 1,000 sessions per instance

### Measuring Performance

```bash
# Query server response time
time echo '{"resourceType":"gameserver","namespace":"game-servers"}' | nc localhost 9000

# Packet round-trip time (requires echo server)
ping -c 100 <target-server-via-proxy>
```

## Troubleshooting Tests

### Query Returns "No matching resources found"

- Verify test resources are deployed: `kubectl get pods -n game-servers`
- Check labels match: `kubectl get pods -n game-servers --show-labels`
- Verify Services exist: `kubectl get svc -n game-servers`

### Token Validation Fails

- Check token TTL hasn't expired (default: 30 seconds)
- Verify token is sent as raw bytes, not JSON
- Check logs for token validation errors

### Session Not Established

- Verify UDP port is accessible
- Check firewall rules
- Ensure LoadBalancer has external IP assigned
- Review logs for connection errors

### Control Packet Not Working

- Verify magic bytes are correct: `FFFFFFFF5245534554`
- Ensure packet format is: `[magic_bytes][token]`
- Check token is valid and not expired
- Review logs for control packet detection

## Continuous Integration

The project includes GitHub Actions workflows that run:

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- `cargo build --release`
- Docker image build

See `.github/workflows/ci.yml` for details.

## Test Coverage

Generate test coverage report (requires `cargo-tarpaulin`):

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage
cargo tarpaulin --out Html --output-dir coverage

# View report
open coverage/index.html
```

## Cleanup

```bash
# Delete test resources
kubectl delete -f test-resources.yaml

# Delete UDP Director
kubectl delete -f k8s/deployment.yaml
kubectl delete -f k8s/configmap-games.yaml
kubectl delete -f k8s/rbac.yaml

# Delete kind cluster
kind delete cluster --name udp-director-test
```
