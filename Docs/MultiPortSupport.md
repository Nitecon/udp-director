[← Back to README](../README.md)

# Multi-Port Support

UDP Director supports routing to multiple ports using a single token. This is essential for game servers that expose multiple ports for different purposes (e.g., game traffic, RCON, query, voice chat).

## Overview

**Single Token, Multiple Ports** - One token provides access to all configured ports on your game server.

### Benefits

- ✅ **Simplified Client Experience** - Get one token, connect to any port
- ✅ **Efficient Resource Usage** - Single proxy instance handles all ports
- ✅ **Protocol Flexibility** - Supports both UDP and TCP
- ✅ **Independent Sessions** - Each port maintains its own session state
- ✅ **Backwards Compatible** - Single-port configurations still work

## Use Cases

### Game Server with Multiple Ports
```
UDP 7777  - Game traffic
TCP 7777  - RCON (admin console)
UDP 27015 - Query (server browser)
UDP 27016 - Voice chat
```

### Single Token Access
Clients get one token and can connect to all ports:
```bash
# Get token once
TOKEN=$(query_for_token)

# Use same token for all ports
connect_game_udp $TOKEN 7777
connect_rcon_tcp $TOKEN 7777
connect_query_udp $TOKEN 27015
```

## Configuration

### Basic Multi-Port Setup

```yaml
queryPort: 9000
tokenTtlSeconds: 30
sessionTimeoutSeconds: 300
controlPacketMagicBytes: "FFFFFFFF5245534554"

# Define multiple data ports that the proxy will listen on
dataPorts:
  - port: 7777
    protocol: "udp"
    name: "game-udp"
  - port: 7777
    protocol: "tcp"
    name: "game-tcp"
  - port: 27015
    protocol: "udp"
    name: "query"

# Default endpoint configuration
defaultEndpoint:
  resourceType: "game-pod"
  namespace: "game-servers"
  labelSelector:
    app: "game-server"

# Map proxy ports to container ports
resourceQueryMapping:
  game-pod:
    group: ""
    version: "v1"
    resource: "pods"
    addressPath: "status.podIP"
    # Multi-port mapping - each proxy port maps to a container port
    ports:
      - name: "game-udp"
        portName: "game-udp"  # Matches container port name
      - name: "game-tcp"
        portName: "game-tcp"  # Matches container port name
      - name: "query"
        portName: "query"     # Matches container port name
```

### Pod Specification

Your game server pods must expose the corresponding ports:

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: game-server
  labels:
    app: game-server
spec:
  containers:
    - name: game
      image: game-server:latest
      ports:
        - name: game-udp      # Must match config
          containerPort: 7777
          protocol: UDP
        - name: game-tcp      # Must match config
          containerPort: 7777
          protocol: TCP
        - name: query         # Must match config
          containerPort: 27015
          protocol: UDP
```

## Client Usage

### 1. Query for Token

```bash
# 1. Query for token (returns all port mappings)
echo '{"resourceType":"game-pod","namespace":"game-servers"}' | nc <IP> 9000

# Response:
# {
#   "token": "550e8400-...",
#   "address": "10.244.1.44",
#   "ports": {
#     "game-udp": 7777,
#     "game-tcp": 7777,
#     "query": 27015
#   },
#   "ttl": 30
# }

# 2. Connect to each port using the same token
# Game UDP traffic
echo "550e8400-..." | nc -u <IP> 7777

# Game TCP traffic
echo "550e8400-..." | nc <IP> 7777

# Query port
echo "550e8400-..." | nc -u <IP> 27015
```

### 2. Connect to Multiple Ports

Use the same token for all ports:

```bash
# Game traffic (UDP)
echo "$TOKEN" | nc -u <PROXY_IP> 7777
# Then send game packets...

# RCON (TCP)
echo "$TOKEN" | nc <PROXY_IP> 7777
# Then send RCON commands...

# Query (UDP)
echo "$TOKEN" | nc -u <PROXY_IP> 27015
# Then send query packets...
```

## How It Works

1. **Token Generation** - Query server generates a single token containing all port mappings
2. **Session Establishment** - First packet on each port establishes a session for that port/protocol
3. **Independent Sessions** - Each port maintains its own session state and timeout
4. **Intelligent Routing** - Proxy routes packets based on destination port and protocol

## Backwards Compatibility

Single-port configurations still work:

```yaml
# Old config (still supported)
dataPort: 7777

# Automatically converted to:
dataPorts:
  - port: 7777
    protocol: "udp"
    name: "default"
```

## Complete Example

See `k8s/configmap-pods-multiport.yaml` for a complete working example.

## Troubleshooting

### Port Not Found in Token

**Error:** `No port mapping found for proxy port X`

**Solution:** Ensure the port is configured in both `dataPorts` and `resourceQueryMapping.ports`

### Session Not Established

**Issue:** Packets dropped after sending token

**Check:**
1. Token is valid (not expired)
2. Port mapping exists for the destination port
3. Resource has the required port exposed

### Port Name Mismatch

**Issue:** "Port with name 'X' not found in resource"

**Solution:** Verify port names in your pod spec match the configuration:
```bash
kubectl get pod <pod-name> -o jsonpath='{.spec.containers[*].ports[*].name}'
```

## Related Documentation

- [Pod Routing Guide](PodRoutingGuide.md) - Pod-based routing configuration
- [Kubernetes Deployment](../k8s/k8s.md) - Deployment examples

[← Back to README](../README.md)
