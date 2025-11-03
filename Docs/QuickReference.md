[← Back to README](../README.md)

# UDP Director - Quick Reference Card

## 🚀 Quick Commands

### Development
```bash
cargo fmt                          # Format code
cargo clippy -- -D warnings        # Lint code
cargo test                         # Run tests
cargo build --release              # Build release binary
cargo run --example client_example # Run example client
```

### Docker
```bash
docker build -t udp-director:latest .
docker run -v $(pwd)/config.yaml:/etc/udp-director/config.yaml udp-director:latest
```

### Kubernetes
```bash
# Deploy
make k8s-deploy
# or
kubectl apply -f k8s/rbac.yaml
# Choose the appropriate configmap:
kubectl apply -f k8s/configmap-games.yaml  # For game servers
# OR kubectl apply -f k8s/configmap-dns.yaml    # For DNS
# OR kubectl apply -f k8s/configmap-ntp.yaml    # For NTP
kubectl apply -f k8s/deployment.yaml

# Check status
kubectl get pods -n udp-director
kubectl get svc -n udp-director
kubectl logs -n udp-director -l app=udp-director -f

# Delete
make k8s-delete
```

---

## 📡 API Reference

### Query Server (TCP :9000)

**Request**:
```json
{
  "type": "query",
  "resourceType": "gameserver",
  "namespace": "game-servers",
  "labelSelector": {
    "agones.dev/fleet": "my-fleet",
    "map": "de_dust2"
  },
  "annotationSelector": {
    "currentPlayers": "32",
    "status": "available"
  },
  "statusQuery": {
    "jsonPath": "status.state",
    "expectedValues": ["Ready", "Allocated"]
  }
}
```

**Success Response**:
```json
{"token": "550e8400-e29b-41d4-a716-446655440000"}
```

**Error Response**:
```json
{"error": "No matching resources found"}
```

### Data Proxy (UDP :7777)

**First Packet** (Session Establishment):
```
[token-string]
```

**Control Packet** (Session Reset):
```
[0xFF 0xFF 0xFF 0xFF 0x52 0x45 0x53 0x45 0x54][new-token-string]
```

**Data Packets**:
```
[your-application-data]
```

---

## 🔧 Configuration Quick Reference

```yaml
queryPort: 9000                    # TCP query port
dataPort: 7777                     # UDP data port
tokenTTLSeconds: 30                # Token validity
sessionTimeoutSeconds: 300         # Session timeout
controlPacketMagicBytes: "FFFFFFFF5245534554" # Magic bytes (hex)

defaultEndpoint:
  resourceType: "gameserver"
  namespace: "default"
  # Labels: Static config (server-side filtering)
  labelSelector:
    agones.dev/fleet: "my-fleet"
    map: "de_dust2"
  # Annotations: Dynamic data (client-side filtering)
  annotationSelector:
    currentPlayers: "32"
    status: "available"
  statusQuery:
    jsonPath: "status.state"
    expectedValues: ["Ready"]

resourceQueryMapping:
  gameserver:
    group: "agones.dev"
    version: "v1"
    resource: "gameservers"
    addressPath: "status.address"
    portName: "default"
```

---

## 🧪 Testing Quick Reference

### Unit Tests
```bash
cargo test                              # All tests
cargo test test_token_generation        # Specific test
cargo test -- --nocapture               # With output
```

### Integration Test
```bash
# Terminal 1: Port-forward
kubectl port-forward -n udp-director svc/udp-director 9000:9000 7777:7777

# Terminal 2: Test query
echo '{"resourceType":"gameserver","namespace":"game-servers"}' | nc localhost 9000

# Terminal 3: Test data
echo "test-token-here" | nc -u localhost 7777
```

### Load Test
```bash
# Query server
echo "POST http://localhost:9000" | vegeta attack -rate=100 -duration=10s -body=query.json | vegeta report

# UDP throughput
iperf3 -c localhost -u -p 7777 -b 10M -t 30
```

---

## 🐛 Debug Commands

```bash
# View logs with debug level
kubectl set env deployment/udp-director -n udp-director RUST_LOG=udp_director=debug
kubectl logs -n udp-director -l app=udp-director -f

# Check connectivity
nc -zv <ip> 9000  # TCP query port
nc -zuv <ip> 7777 # UDP data port

# Verify RBAC
kubectl auth can-i list gameservers.agones.dev \
  --as=system:serviceaccount:udp-director:udp-director

# Check resources
kubectl get gameservers -n game-servers --show-labels
kubectl get svc -n game-servers

# Packet capture
kubectl exec -n udp-director <pod> -- tcpdump -i any -n port 7777
```

---

## 📊 Module Overview

| Module | Purpose | Key Types |
|--------|---------|-----------|
| `config.rs` | Configuration management | `Config`, `ResourceMapping` |
| `token_cache.rs` | Token storage with TTL | `TokenCache`, `TokenTarget` |
| `session.rs` | Session state management | `SessionManager`, `Session` |
| `k8s_client.rs` | Kubernetes API client | `K8sClient`, `StatusQuery` |
| `query_server.rs` | TCP query endpoint | `QueryServer`, `QueryRequest` |
| `proxy.rs` | UDP data proxy | `DataProxy` |
| `main.rs` | Application entry point | - |

---

## 🔐 RBAC Permissions Required

```yaml
- apiGroups: [""]
  resources: ["services", "configmaps"]
  verbs: ["get", "list", "watch"]

- apiGroups: ["agones.dev"]
  resources: ["gameservers"]
  verbs: ["get", "list", "watch"]
```

---

## 📦 Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| tokio | 1.42 | Async runtime |
| kube | 0.97 | Kubernetes client |
| moka | 0.12 | TTL cache |
| dashmap | 6.1 | Concurrent HashMap |
| serde_json | 1.0 | JSON serialization |
| uuid | 1.11 | Token generation |
| tracing | 0.1 | Logging |

---

## 🎯 Common Patterns

### Query for Server
```rust
let query = json!({
    "resourceType": "gameserver",
    "namespace": "game-servers"
});
let token = tcp_query(director_ip, 9000, query)?;
```

### Establish Session
```rust
let socket = UdpSocket::bind("0.0.0.0:0")?;
socket.connect((director_ip, 7777))?;
socket.send(token.as_bytes())?;
```

### Reset Session
```rust
let magic = hex::decode("FFFFFFFF5245534554")?;
let mut packet = magic;
packet.extend_from_slice(new_token.as_bytes());
socket.send(&packet)?;
```

---

## 📈 Performance Targets

- **Query Latency**: < 10ms
- **Proxy Latency**: < 1ms
- **Throughput**: > 10k packets/sec
- **Sessions**: > 1k concurrent
- **Memory**: ~128MB + 1KB/session

---

## 🚨 Troubleshooting Checklist

- [ ] Rust toolchain installed (2024 edition)
- [ ] Kubernetes cluster accessible
- [ ] Cilium CNI installed
- [ ] RBAC resources applied
- [ ] ConfigMap deployed
- [ ] Backend resources exist with correct labels
- [ ] Services have matching selector labels
- [ ] LoadBalancer has external IP
- [ ] Firewall allows UDP 7777 and TCP 9000
- [ ] Token used within TTL window (30s)
- [ ] Magic bytes match configuration

---

## 📚 Documentation Links

- **Technical Reference**: `Docs/TechnicalReference.md`
- **Coding Guidelines**: `Docs/CodingGuidelines.md`
- **Testing Guide**: `Docs/Testing.md`
- **Project Summary**: `Docs/ProjectSummary.md`
- **Quick Reference**: `Docs/QuickReference.md`
- **Example Client**: `examples/client_example.rs`

[← Back to README](../README.md)

---

## 🎓 Learning Resources

### Rust Concepts Used
- Async/await with Tokio
- Error handling (Result, anyhow, thiserror)
- Ownership and borrowing
- Trait implementations
- Pattern matching
- Concurrent data structures

### Kubernetes Concepts
- Custom Resource Definitions (CRDs)
- Service discovery
- RBAC (ServiceAccount, Role, RoleBinding)
- ConfigMaps
- LoadBalancer Services

### Networking Concepts
- UDP stateful proxying
- Session management
- Token-based authentication
- Control vs data plane separation
