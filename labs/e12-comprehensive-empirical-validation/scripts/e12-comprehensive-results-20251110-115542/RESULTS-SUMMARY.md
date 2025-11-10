# E12 Comprehensive Results Summary

**Total Tests:** 24


## Scale: 12node

### Bandwidth: 1gbps

| Architecture | Network Bytes | App Bytes | Latency p50 | Latency p90 | Doc Receptions |
|-------------|---------------|-----------|-------------|-------------|----------------|
| traditional | 720,000 | 75,045 | 0.4ms | 39.7ms | 22 |
| cap-full | 5,304,000 | 876 | 16.1ms | 1446.4ms | 132 |

### Bandwidth: 100mbps

| Architecture | Network Bytes | App Bytes | Latency p50 | Latency p90 | Doc Receptions |
|-------------|---------------|-----------|-------------|-------------|----------------|
| traditional | 717,200 | 75,681 | 0.4ms | 35.7ms | 22 |
| cap-full | 5,345,000 | 876 | 13.3ms | 1502.6ms | 132 |

### Bandwidth: 1mbps

| Architecture | Network Bytes | App Bytes | Latency p50 | Latency p90 | Doc Receptions |
|-------------|---------------|-----------|-------------|-------------|----------------|
| traditional | 883,800 | 96,923 | 21.1ms | 39.1ms | 22 |
| cap-full | 5,612,000 | 1,314 | 16.8ms | 3416.4ms | 176 |

### Bandwidth: 256kbps

| Architecture | Network Bytes | App Bytes | Latency p50 | Latency p90 | Doc Receptions |
|-------------|---------------|-----------|-------------|-------------|----------------|
| cap-full | 5,835,000 | 1,314 | 16.6ms | 3187.6ms | 176 |
| traditional | 870,000 | 98,827 | 84.2ms | 156.2ms | 22 |

## Scale: 24node

### Bandwidth: 1gbps

| Architecture | Network Bytes | App Bytes | Latency p50 | Latency p90 | Doc Receptions |
|-------------|---------------|-----------|-------------|-------------|----------------|
| traditional | 7,473,300 | 0 | 0.0ms | 0.0ms | 0 |
| cap-full | 8,160,000 | 3,912 | 138.4ms | 4960.2ms | 198 |
| cap-hierarchical | 8,041,300 | 3,912 | 482.2ms | 4802.8ms | 195 |

### Bandwidth: 100mbps

| Architecture | Network Bytes | App Bytes | Latency p50 | Latency p90 | Doc Receptions |
|-------------|---------------|-----------|-------------|-------------|----------------|
| traditional | 7,456,100 | 0 | 0.0ms | 0.0ms | 0 |
| cap-hierarchical | 8,046,000 | 3,912 | 319.0ms | 4845.6ms | 204 |
| cap-full | 7,843,600 | 3,912 | 414.5ms | 4980.6ms | 207 |

### Bandwidth: 1mbps

| Architecture | Network Bytes | App Bytes | Latency p50 | Latency p90 | Doc Receptions |
|-------------|---------------|-----------|-------------|-------------|----------------|
| traditional | 7,392,100 | 0 | 0.0ms | 0.0ms | 0 |
| cap-hierarchical | 9,148,000 | 5,868 | 474.1ms | 4951.0ms | 258 |
| cap-full | 9,335,600 | 5,868 | 220.3ms | 4974.8ms | 246 |

### Bandwidth: 256kbps

| Architecture | Network Bytes | App Bytes | Latency p50 | Latency p90 | Doc Receptions |
|-------------|---------------|-----------|-------------|-------------|----------------|
| traditional | 7,666,700 | 0 | 0.0ms | 0.0ms | 0 |
| cap-hierarchical | 9,334,000 | 5,868 | 175.6ms | 5120.8ms | 252 |
| cap-full | 9,235,000 | 5,868 | 377.3ms | 5017.6ms | 255 |

## Scale: 2node

### Bandwidth: 1gbps

| Architecture | Network Bytes | App Bytes | Latency p50 | Latency p90 | Doc Receptions |
|-------------|---------------|-----------|-------------|-------------|----------------|
| traditional | 55,600 | 12,147 | 0.1ms | 0.2ms | 2 |

### Bandwidth: 100mbps

| Architecture | Network Bytes | App Bytes | Latency p50 | Latency p90 | Doc Receptions |
|-------------|---------------|-----------|-------------|-------------|----------------|
| traditional | 48,200 | 12,449 | 34.8ms | 35.7ms | 2 |

### Bandwidth: 1mbps

| Architecture | Network Bytes | App Bytes | Latency p50 | Latency p90 | Doc Receptions |
|-------------|---------------|-----------|-------------|-------------|----------------|
| traditional | 69,200 | 15,771 | 4.2ms | 4.8ms | 2 |

### Bandwidth: 256kbps

| Architecture | Network Bytes | App Bytes | Latency p50 | Latency p90 | Doc Receptions |
|-------------|---------------|-----------|-------------|-------------|----------------|
| traditional | 69,800 | 15,771 | 15.9ms | 16.4ms | 2 |
