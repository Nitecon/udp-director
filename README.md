# UDP Director

[![Rust](https://img.shields.io/badge/Rust-2024%20Edition-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-TBD-blue)]()
[![Status](https://img.shields.io/badge/Status-Production%20Ready-green)]()

A Kubernetes-native, high-performance stateful UDP/TCP proxy for dynamic routing with token-based sessions and live migration support. Perfect for game server matchmaking, load balancing, and zero-downtime server switching.

## Quick Links

- **[Technical Reference](Docs/TECHNICAL_REFERENCE.md)** - Complete deployment and technical guide
- **[Coding Guidelines](Docs/CodingGuidelines.md)** - Standards for contributors
- **[Testing Guide](Docs/TESTING.md)** - Unit, integration, and load testing
- **[Quick Reference](Docs/QUICK_REFERENCE.md)** - Commands and API reference
- **[Docker Registry Setup](Docs/DOCKER_REGISTRY.md)** - Registry configuration
- **[Project Summary](Docs/PROJECT_SUMMARY.md)** - Implementation status
- **[Changelog](Docs/CHANGELOG.md)** - Version history

## What Problem Does This Solve?

Traditional UDP load balancers are stateless and can't intelligently route clients based on Kubernetes resource state (like Agones GameServers). UDP Director solves this by:

- **Querying Kubernetes resources** to find available game servers based on labels, status, and capacity
- **Establishing stateful sessions** so clients maintain connections to specific backends
- **Enabling live migration** where clients can seamlessly switch servers without reconnecting
- **Integrating with K8s** to automatically discover services and route traffic

**Use Cases:**
- Game server matchmaking (Agones, custom CRDs)
- Dynamic UDP load balancing based on resource state
- Zero-downtime server migration for players
- Multi-tenant UDP routing

## How It Works

UDP Director uses a three-phase flow:

```
1. QUERY (TCP :9000)
   Client → Director: "Find me a game server with map=de_dust2"
   Director → K8s API: Query resources, find matching service
   Director → Client: Return token (valid for 30s)

2. CONNECT (UDP :7777)
   Client → Director: Send token as first packet
   Director: Create session mapping (Client IP:Port → Target IP:Port)
   Client ↔ Target: All UDP traffic proxied

3. RESET (UDP :7777) - Optional
   Client → Director: Send control packet with new token
   Director: Update session to point to new target
   Client ↔ New Target: Traffic seamlessly redirected
```

## Quick Start

### Prerequisites

- Rust 2024 Edition (stable via `rustup`)
- Kubernetes cluster with Cilium CNI
- Docker for building images
- `kubectl` configured

### Deploy to Kubernetes

```bash
# Clone repository
git clone <repository-url>
cd udp-director

# Build and push to registry
./dockerpush.sh

# Deploy
kubectl apply -f k8s/rbac.yaml
# Choose the appropriate configmap for your use case:
kubectl apply -f k8s/configmap-games.yaml  # For game servers (Agones)
# OR kubectl apply -f k8s/configmap-dns.yaml    # For DNS routing
# OR kubectl apply -f k8s/configmap-ntp.yaml    # For NTP routing
kubectl apply -f k8s/deployment.yaml

# Verify
kubectl get pods -n udp-director
kubectl logs -n udp-director -l app=udp-director -f
```

### Client Integration Example

```bash
# Phase 1: Query for backend (with label and status filtering)
echo '{"resourceType":"gameserver","namespace":"starx","labelSelector":{"agones.dev/fleet":"m-tutorial"},"statusQuery":{"jsonPath":"status.state","expectedValue":"Ready"}}' | nc <LoadBalancer-IP> 9000
# Response: {"token":"550e8400-e29b-41d4-a716-446655440000"}

# Phase 2: Connect with token
echo "550e8400-e29b-41d4-a716-446655440000" | nc -u <LoadBalancer-IP> 7777

# Phase 3: Reset to new server (optional)
# Send control packet: [MagicBytes][NewToken]
echo -n -e "\xFF\xFF\xFF\xFF\x52\x45\x53\x45\x54${NEW_TOKEN}" | nc -u <LoadBalancer-IP> 7777
```

See [Technical Reference](Docs/TECHNICAL_REFERENCE.md) and [Testing Guide](Docs/TESTING.md) for complete examples.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Kubernetes Cluster                        │
│                                                               │
│  ┌────────────────────────────────────────────────────┐     │
│  │              UDP Director Pod                       │     │
│  │                                                      │     │
│  │  ┌─────────────┐         ┌──────────────────┐     │     │
│  │  │Query Server │         │   Data Proxy     │     │     │
│  │  │  (TCP:9000) │         │   (UDP:7777)     │     │     │
│  │  └──────┬──────┘         └────────┬─────────┘     │     │
│  │         │                          │                │     │
│  │  ┌──────▼────────┐      ┌─────────▼─────────┐     │     │
│  │  │ Token Cache   │◄─────┤ Session Manager   │     │     │
│  │  │  (TTL: 30s)   │      │ (Timeout: 300s)   │     │     │
│  │  └───────────────┘      └───────────────────┘     │     │
│  │         │                                           │     │
│  │  ┌──────▼─────────────────────────────────┐       │     │
│  │  │    Kubernetes API Client                │       │     │
│  │  └─────────────────────────────────────────┘       │     │
│  └────────────────────────────────────────────────────┘     │
│                                                               │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│  │GameSvr 1 │  │GameSvr 2 │  │GameSvr N │                  │
│  └──────────┘  └──────────┘  └──────────┘                  │
└─────────────────────────────────────────────────────────────┘
         ▲
         │ UDP/TCP Traffic
         │
   External Client
```

## Configuration

Choose the appropriate ConfigMap for your use case:

- **`k8s/configmap-games.yaml`** - For Agones game servers (GameServer and Fleet routing)
- **`k8s/configmap-dns.yaml`** - For DNS service routing (port 53)
- **`k8s/configmap-ntp.yaml`** - For NTP service routing (port 123)

Each ConfigMap includes:
```yaml
queryPort: 9000                    # TCP query endpoint
dataPort: 7777                     # UDP data proxy (varies by use case)
tokenTTLSeconds: 30                # Token validity
sessionTimeoutSeconds: 300         # Session timeout
controlPacketMagicBytes: "FFFFFFFF5245534554"  # Control packet ID

resourceQueryMapping:
  # Resource-specific mappings (see individual files)
```

Edit the chosen ConfigMap to customize for your environment.

## Performance

- **Query Latency**: < 10ms (+ K8s API latency)
- **Proxy Overhead**: < 1ms per packet
- **Throughput**: > 10,000 packets/second
- **Concurrent Sessions**: > 1,000 per instance
- **Memory**: ~128MB + ~1KB per session

## Development

```bash
# Format and lint
cargo fmt
cargo clippy -- -D warnings

# Test
cargo test

# Build
cargo build --release

# Or use make
make help
```

## Technology Stack

- **Language**: Rust 2024 Edition
- **Runtime**: Tokio (async I/O)
- **K8s Client**: kube-rs
- **Caching**: moka (TTL cache)
- **Concurrency**: DashMap
- **Target**: Cilium Service Mesh on Kubernetes

## Contributing

1. Follow [Coding Guidelines](Docs/CodingGuidelines.md)
2. Ensure `cargo fmt` and `cargo clippy` pass
3. Add tests for new functionality
4. Update documentation in `Docs/`
5. Add changelog entry

## License

[Add license information]

---

**Version**: 0.1.0  
**Status**: Production Ready  
**Target**: Cilium Service Mesh on Kubernetes
