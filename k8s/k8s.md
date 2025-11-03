# Kubernetes Manifests

This directory contains Kubernetes manifests for deploying UDP Director.

## ConfigMap Files

UDP Director provides **five pre-configured ConfigMaps** for different use cases. Choose the one that matches your needs:

### 🎮 configmap-agones-gameserver.yaml (Recommended)
**Use for**: Agones game server routing with direct resource inspection

- Extracts address/port directly from GameServer status
- No service discovery needed - simpler and faster
- Supports label and status filtering
- Default data port: 7777

**Example query**:
```json
{"resourceType": "gameserver", "namespace": "starx", "labelSelector": {"agones.dev/fleet": "m-tutorial"}}
```

**Deploy**:
```bash
kubectl apply -f k8s/configmap-agones-gameserver.yaml
```

---

### 🎮 configmap-agones-service.yaml
**Use for**: Agones game server routing via Kubernetes Services

- Routes through Kubernetes Services (requires Services for each GameServer)
- Demonstrates service-based discovery pattern
- Default data port: 7777

**Example query**:
```json
{"resourceType": "gameserver", "namespace": "starx", "labelSelector": {"agones.dev/fleet": "m-tutorial"}}
```

**Deploy**:
```bash
kubectl apply -f k8s/configmap-agones-service.yaml
```

---

### 📦 configmap-pods.yaml
**Use for**: Direct pod routing for standard Kubernetes deployments

**Features**:
- Routes directly to pod IPs (bypasses services)
- Supports label-based pod selection
- Filters by pod phase (Running, Pending, etc.)
- Extracts ports by name or array index
- Perfect for game servers, stateful apps, or any pod-based routing
- Default data port: 7777

**Example query**:
```json
{"resourceType": "starx-pod", "namespace": "starx", "labelSelector": {"app": "starx-test", "map": "m-tutorial"}}
```

**Deploy**:
```bash
kubectl apply -f k8s/configmap-pods.yaml
```

**Key Features**:
- **Port Name Lookup**: Automatically finds ports by name (e.g., "game-udp", "game-tcp")
- **Array Indexing**: Supports JSONPath with array syntax like `spec.containers[0].ports[0].containerPort`
- **Multi-Container Support**: Can target specific containers in multi-container pods
- **Status Filtering**: Only routes to pods in "Running" state

---

### 🌐 configmap-dns.yaml
**Use for**: DNS service routing

**Features**:
- Routes DNS packets (UDP port 53)
- Targets CoreDNS or other DNS services
- Uses service-based lookup
- Default data port: 53

**Example query**:
```json
{"resourceType": "dns", "namespace": "kube-system", "labelSelector": {"k8s-app": "coredns"}}
```

**Deploy**:
```bash
kubectl apply -f k8s/configmap-dns.yaml
```

---

### ⏰ configmap-ntp.yaml
**Use for**: NTP service routing

**Features**:
- Routes NTP packets (UDP port 123)
- Targets Chrony or other NTP services
- Uses service-based lookup
- Default data port: 123

**Example query**:
```json
{"resourceType": "ntp", "namespace": "default", "labelSelector": {"app": "chrony"}}
```

**Deploy**:
```bash
kubectl apply -f k8s/configmap-ntp.yaml
```

---

## Deployment Order

All manifests are namespace-agnostic and will deploy to your current context or specified namespace.

### Quick Start

```bash
# Create and use your namespace
kubectl create namespace starx
kubectl config set-context --current --namespace=starx

# Deploy everything
kubectl apply -f k8s/rbac.yaml -n starx
kubectl apply -f k8s/configmap-agones-gameserver.yaml -n starx
kubectl apply -f k8s/deployment.yaml -n starx

# Optional: Apply Pod Security Standards (K8s 1.23+)
kubectl label namespace starx \
  pod-security.kubernetes.io/enforce=restricted \
  pod-security.kubernetes.io/audit=restricted \
  pod-security.kubernetes.io/warn=restricted
```

### Step-by-Step

1. **Create namespace** (if needed):
   ```bash
   kubectl create namespace <your-namespace>
   ```

2. **Update RBAC namespace reference**:
   ```bash
   # Edit k8s/rbac.yaml and change the ClusterRoleBinding subject namespace
   # Or use sed:
   sed -i 's/namespace: udp-director/namespace: <your-namespace>/' k8s/rbac.yaml
   
   # Then apply
   kubectl apply -f k8s/rbac.yaml
   ```

3. **Pod Security** (recommended for K8s 1.23+):
   ```bash
   # Apply Pod Security Standards labels to your namespace
   kubectl label namespace <your-namespace> \
     pod-security.kubernetes.io/enforce=restricted \
     pod-security.kubernetes.io/audit=restricted \
     pod-security.kubernetes.io/warn=restricted
   ```

4. **ConfigMap** (choose one):
   ```bash
   # For Agones with direct GameServer inspection (recommended)
   kubectl apply -f k8s/configmap-agones-gameserver.yaml -n <your-namespace>
   
   # OR for Agones with service-based lookup
   kubectl apply -f k8s/configmap-agones-service.yaml -n <your-namespace>
   
   # OR for direct pod routing (standard Kubernetes deployments)
   kubectl apply -f k8s/configmap-pods.yaml -n <your-namespace>
   
   # OR for DNS routing
   kubectl apply -f k8s/configmap-dns.yaml -n <your-namespace>
   
   # OR for NTP routing
   kubectl apply -f k8s/configmap-ntp.yaml -n <your-namespace>
   ```

5. **Deployment**:
   ```bash
   kubectl apply -f k8s/deployment.yaml -n <your-namespace>
   ```

## Customization

Each ConfigMap is fully documented with inline comments. You can:
- Modify port numbers
- Adjust TTL values
- Change resource mappings
- Add custom resource types

See the [Technical Reference](../Docs/TECHNICAL_REFERENCE.md) for advanced configuration options.

## Switching ConfigMaps

To switch between ConfigMaps:

```bash
# Delete the current configmap
kubectl delete configmap udp-director-config -n udp-director

# Apply the new one
kubectl apply -f k8s/configmap-ntp.yaml

# Restart the deployment to pick up changes
kubectl rollout restart deployment/udp-director -n udp-director
```
