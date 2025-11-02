# Address Type Selection Feature

## Problem
When using Agones with `portPolicy: None`, the GameServer exposes the pod internally without node port allocation. The standard `status.address` field points to the node's InternalIP, which cannot be used to reach the pod. Instead, the `status.addresses` array contains multiple address entries including the **PodIP**, which is the correct address to use for direct pod communication.

## Solution
Added an optional `addressType` field to the `resourceQueryMapping` configuration that allows filtering addresses from a `status.addresses` array by their type.

## Configuration Changes

### New Field: `addressType`
- **Location**: `resourceQueryMapping.<resource>.addressType`
- **Type**: String (optional)
- **Purpose**: Filters addresses from an array by matching the `type` field
- **Common Values**: `"PodIP"`, `"InternalIP"`, `"Hostname"`

### Usage Examples

#### For portPolicy: None (Use PodIP)
```yaml
resourceQueryMapping:
  gameserver:
    group: "agones.dev"
    version: "v1"
    resource: "gameservers"
    addressPath: "status.addresses"
    addressType: "PodIP"
    portName: "default"
```

#### For portPolicy: Dynamic/Passthrough (Use InternalIP or simple address)
```yaml
# Option 1: Use simple address field
resourceQueryMapping:
  gameserver:
    addressPath: "status.address"
    # no addressType needed
    portName: "default"

# Option 2: Explicitly use InternalIP from addresses array
resourceQueryMapping:
  gameserver:
    addressPath: "status.addresses"
    addressType: "InternalIP"
    portName: "default"
```

## Implementation Details

### Code Changes

1. **config.rs**: Added `address_type: Option<String>` field to `ResourceMapping` struct
2. **k8s_client.rs**: Updated `extract_address()` method to accept `address_type` parameter
   - When `address_type` is `None`: Extracts simple string from `addressPath` (original behavior)
   - When `address_type` is specified: Searches array at `addressPath` for entry with matching `type` field
3. **Updated all call sites**: 
   - `main.rs`: `extract_and_log_target()`
   - `proxy.rs`: `extract_direct_endpoint()`
   - `query_server.rs`: `extract_direct_target()`
   - `resource_monitor.rs`: Default endpoint monitoring

### Address Array Structure
The implementation expects addresses to follow this structure:
```yaml
status:
  addresses:
    - address: "10.0.0.34"
      type: "InternalIP"
    - address: "talos-6l3-1qi"
      type: "Hostname"
    - address: "10.244.1.113"
      type: "PodIP"
```

## Benefits

1. **Flexible Address Selection**: Support for different Agones port policies
2. **Backward Compatible**: Existing configs without `addressType` continue to work
3. **Explicit Configuration**: Clear intent when using PodIP vs other address types
4. **Consistent Behavior**: Same configuration applies to:
   - Default endpoint allocation
   - Client query responses
   - Resource monitoring

## Testing

All existing tests pass without modification, confirming backward compatibility.

## Migration Guide

If you're currently using `portPolicy: Dynamic` or `Passthrough`, no changes are needed.

If you're using `portPolicy: None`, update your configmap:
```yaml
# Before (won't work with portPolicy: None)
addressPath: "status.address"

# After (works with portPolicy: None)
addressPath: "status.addresses"
addressType: "PodIP"
```
