# HIVE-BTLE Sync Test: M5Stack Core2 + Samsung Tablet

**Organization**: (r)evolve - Revolve Team LLC  
**Date**: 2025-12-13  
**Test Objective**: Validate HIVE-Lite sync between two M5Stack Core2 nodes and a Samsung Android tablet

---

## Test Configuration

### Hardware

| Device | Role | Platform | BLE Capabilities |
|--------|------|----------|------------------|
| M5Stack Core2 #1 | Sensor Node (Leaf) | ESP32/ESP-IDF | BLE 4.2, GATT Peripheral |
| M5Stack Core2 #2 | Sensor Node (Leaf) | ESP32/ESP-IDF | BLE 4.2, GATT Peripheral |
| Samsung Tablet | Aggregator (Parent) | Android | BLE 5.x, GATT Central + Peripheral |

### Network Topology

```
                    ┌─────────────────────┐
                    │   Samsung Tablet    │
                    │   (HIVE Parent)     │
                    │   GATT Central      │
                    └──────────┬──────────┘
                               │
              ┌────────────────┼────────────────┐
              │                │                │
              ▼                ▼                ▼
     ┌─────────────┐  ┌─────────────┐  ┌─────────────┐
     │ M5Stack #1  │  │ M5Stack #2  │  │ (Future     │
     │ Sensor Leaf │  │ Sensor Leaf │  │  devices)   │
     │ GATT Periph │  │ GATT Periph │  └─────────────┘
     └─────────────┘  └─────────────┘
```

### Data Flow

1. **M5Stack nodes** generate sensor data (accelerometer, temperature, button presses)
2. **M5Stack nodes** advertise HIVE beacons and expose GATT service
3. **Samsung tablet** discovers M5Stack nodes via BLE scanning
4. **Samsung tablet** connects as GATT Central to both M5Stacks
5. **Samsung tablet** syncs CRDT state from each M5Stack
6. **Samsung tablet** can push commands/state back to M5Stacks
7. **M5Stack nodes** can see each other's state via tablet relay

---

## Test Scenarios

### Scenario 1: Basic Discovery

**Objective**: Verify M5Stack nodes are discoverable by Samsung tablet

**Steps**:
1. Power on M5Stack #1 and #2
2. Launch HIVE test app on Samsung tablet
3. Start BLE scan
4. Verify both M5Stack nodes appear with HIVE service UUID

**Expected Results**:
- Both nodes visible within 5 seconds
- HIVE beacon data parseable (node ID, battery, capabilities)
- RSSI values reasonable for distance

### Scenario 2: GATT Connection

**Objective**: Establish GATT connections from tablet to both M5Stacks

**Steps**:
1. From tablet, connect to M5Stack #1
2. Discover HIVE GATT service
3. Read Node Info characteristic
4. Connect to M5Stack #2 (multi-connection)
5. Read Node Info from both

**Expected Results**:
- Connection established within 3 seconds per device
- MTU negotiated to 251 bytes (or max supported)
- Node Info readable with correct format

### Scenario 3: Sensor Data Sync

**Objective**: Sync sensor readings from M5Stack to tablet

**Steps**:
1. Connect tablet to both M5Stacks
2. Subscribe to Sync State notifications
3. Move M5Stack #1 (accelerometer change)
4. Press button on M5Stack #2
5. Observe notifications on tablet

**Expected Results**:
- Accelerometer delta received within 500ms
- Button press event received within 200ms
- CRDT state merges correctly on tablet

### Scenario 4: Bidirectional Sync

**Objective**: Push state from tablet to M5Stack nodes

**Steps**:
1. Connect tablet to both M5Stacks
2. Send "LED Color" command to M5Stack #1
3. Send "Vibrate" command to M5Stack #2
4. Verify commands executed

**Expected Results**:
- M5Stack #1 LED changes color
- M5Stack #2 vibrates
- Command acknowledgment received

### Scenario 5: Multi-Node State Visibility

**Objective**: M5Stack #1 can see M5Stack #2's state via tablet relay

**Steps**:
1. Connect tablet to both M5Stacks
2. M5Stack #1 requests "all peer states" from tablet
3. Tablet syncs M5Stack #2's state to M5Stack #1
4. Verify M5Stack #1 can display #2's data

**Expected Results**:
- M5Stack #1 shows #2's battery level
- M5Stack #1 shows #2's sensor readings
- State consistency maintained

### Scenario 6: Disconnection Recovery

**Objective**: Handle connection loss gracefully

**Steps**:
1. Establish all connections
2. Move M5Stack #1 out of range
3. Wait for disconnection detection
4. Move M5Stack #1 back in range
5. Observe reconnection and state sync

**Expected Results**:
- Disconnection detected within 10 seconds
- Automatic reconnection when in range
- State synchronized after reconnection
- No data loss

### Scenario 7: Power Efficiency

**Objective**: Validate low-power operation

**Steps**:
1. Configure M5Stack for LowPower profile
2. Run for 1 hour with periodic syncs
3. Monitor battery consumption
4. Compare to continuous connection baseline

**Expected Results**:
- LowPower: <5% battery per hour
- Baseline: ~15% battery per hour
- >50% power reduction achieved

---

## Implementation

### M5Stack Core2 Firmware (ESP-IDF)

The M5Stack firmware uses ESP-IDF with the NimBLE stack for BLE operations.

#### Project Structure

```
m5stack-hive-lite/
├── CMakeLists.txt
├── sdkconfig.defaults
├── main/
│   ├── CMakeLists.txt
│   ├── main.c
│   ├── hive_ble.h
│   ├── hive_ble.c
│   ├── hive_gatt.h
│   ├── hive_gatt.c
│   ├── hive_crdt.h
│   ├── hive_crdt.c
│   ├── sensors.h
│   └── sensors.c
└── components/
    └── m5core2/  # M5Stack Core2 drivers
```

#### CMakeLists.txt

```cmake
cmake_minimum_required(VERSION 3.16)

set(EXTRA_COMPONENT_DIRS 
    $ENV{IDF_PATH}/examples/common_components/led_strip
    components
)

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(m5stack-hive-lite)
```

#### sdkconfig.defaults

```
# BLE Configuration
CONFIG_BT_ENABLED=y
CONFIG_BT_NIMBLE_ENABLED=y
CONFIG_BT_NIMBLE_MAX_CONNECTIONS=3
CONFIG_BT_NIMBLE_ROLE_PERIPHERAL=y
CONFIG_BT_NIMBLE_ROLE_BROADCASTER=y
CONFIG_BT_NIMBLE_GAP_DEVICE_NAME_MAX_LEN=32
CONFIG_BT_NIMBLE_ATT_PREFERRED_MTU=256
CONFIG_BT_NIMBLE_SM_LEGACY=y
CONFIG_BT_NIMBLE_SM_SC=y

# Power Management
CONFIG_PM_ENABLE=y
CONFIG_FREERTOS_USE_TICKLESS_IDLE=y

# Logging
CONFIG_LOG_DEFAULT_LEVEL_INFO=y
```

#### main/hive_ble.h

```c
#ifndef HIVE_BLE_H
#define HIVE_BLE_H

#include <stdint.h>
#include <stdbool.h>
#include "esp_err.h"

// HIVE Service UUID: f47ac10b-58cc-4372-a567-0e02b2c3d479
#define HIVE_SERVICE_UUID_128 \
    0x79, 0xd4, 0xc3, 0xb2, 0x02, 0x0e, 0x67, 0xa5, \
    0x72, 0x43, 0xcc, 0x58, 0x0b, 0xc1, 0x7a, 0xf4

// Characteristic UUIDs (16-bit short form)
#define CHAR_NODE_INFO_UUID     0x0001
#define CHAR_SYNC_STATE_UUID    0x0002
#define CHAR_SYNC_DATA_UUID     0x0003
#define CHAR_COMMAND_UUID       0x0004
#define CHAR_STATUS_UUID        0x0005

// Node capabilities flags
typedef enum {
    CAP_LITE_NODE       = 0x0001,
    CAP_SENSOR_ACCEL    = 0x0002,
    CAP_SENSOR_TEMP     = 0x0004,
    CAP_SENSOR_BUTTON   = 0x0008,
    CAP_ACTUATOR_LED    = 0x0010,
    CAP_ACTUATOR_VIBRATE = 0x0020,
    CAP_DISPLAY         = 0x0040,
} hive_capability_t;

// Hierarchy levels
typedef enum {
    LEVEL_PLATFORM = 0,
    LEVEL_SQUAD = 1,
    LEVEL_PLATOON = 2,
    LEVEL_COMPANY = 3,
} hive_hierarchy_level_t;

// HIVE Beacon structure (16 bytes)
typedef struct __attribute__((packed)) {
    uint8_t version_caps_hi;    // version(4) | caps_hi(4)
    uint8_t caps_lo;            // caps_lo(8)
    uint32_t node_id_short;     // Truncated node ID
    uint8_t hierarchy_level;    // Hierarchy level
    uint8_t geohash[3];         // 6-char geohash (24 bits)
    uint8_t battery_percent;    // Battery level (0-100)
    uint16_t seq_num;           // Sequence number
} hive_beacon_t;

// BLE configuration
typedef struct {
    uint32_t node_id;
    uint16_t capabilities;
    uint8_t hierarchy_level;
    uint32_t geohash;
    
    // Advertising parameters
    uint16_t adv_interval_ms;
    int8_t tx_power_dbm;
    
    // Connection parameters
    uint16_t conn_interval_min_ms;
    uint16_t conn_interval_max_ms;
    uint16_t slave_latency;
    uint16_t supervision_timeout_ms;
} hive_ble_config_t;

// Callbacks
typedef void (*hive_ble_connect_cb_t)(uint16_t conn_handle);
typedef void (*hive_ble_disconnect_cb_t)(uint16_t conn_handle, int reason);
typedef void (*hive_ble_sync_data_cb_t)(uint16_t conn_handle, const uint8_t *data, uint16_t len);
typedef void (*hive_ble_command_cb_t)(uint16_t conn_handle, const uint8_t *data, uint16_t len);

typedef struct {
    hive_ble_connect_cb_t on_connect;
    hive_ble_disconnect_cb_t on_disconnect;
    hive_ble_sync_data_cb_t on_sync_data;
    hive_ble_command_cb_t on_command;
} hive_ble_callbacks_t;

// API
esp_err_t hive_ble_init(const hive_ble_config_t *config, const hive_ble_callbacks_t *callbacks);
esp_err_t hive_ble_start_advertising(void);
esp_err_t hive_ble_stop_advertising(void);
esp_err_t hive_ble_update_beacon(const hive_beacon_t *beacon);
esp_err_t hive_ble_notify_sync_state(uint16_t conn_handle, const uint8_t *data, uint16_t len);
esp_err_t hive_ble_notify_status(uint16_t conn_handle, const uint8_t *data, uint16_t len);
uint8_t hive_ble_get_battery_percent(void);
bool hive_ble_is_connected(void);
uint16_t hive_ble_get_mtu(uint16_t conn_handle);

#endif // HIVE_BLE_H
```

#### main/hive_ble.c

```c
#include "hive_ble.h"
#include "hive_gatt.h"

#include "esp_log.h"
#include "esp_nimble_hci.h"
#include "nimble/nimble_port.h"
#include "nimble/nimble_port_freertos.h"
#include "host/ble_hs.h"
#include "host/util/util.h"
#include "services/gap/ble_svc_gap.h"
#include "services/gatt/ble_svc_gatt.h"

static const char *TAG = "HIVE_BLE";

static hive_ble_config_t s_config;
static hive_ble_callbacks_t s_callbacks;
static hive_beacon_t s_beacon;
static uint16_t s_conn_handle = BLE_HS_CONN_HANDLE_NONE;
static uint8_t s_own_addr_type;

// Forward declarations
static void ble_app_on_sync(void);
static int ble_gap_event(struct ble_gap_event *event, void *arg);

// Advertising data
static uint8_t s_adv_data[31];
static uint8_t s_adv_data_len;

static void build_adv_data(void) {
    uint8_t *p = s_adv_data;
    
    // Flags
    *p++ = 2;  // Length
    *p++ = BLE_HS_ADV_TYPE_FLAGS;
    *p++ = BLE_HS_ADV_F_DISC_GEN | BLE_HS_ADV_F_BREDR_UNSUP;
    
    // Complete 128-bit Service UUID
    *p++ = 17;  // Length
    *p++ = BLE_HS_ADV_TYPE_COMP_UUIDS128;
    uint8_t uuid[] = {HIVE_SERVICE_UUID_128};
    memcpy(p, uuid, 16);
    p += 16;
    
    // Manufacturer Specific Data (HIVE Beacon)
    *p++ = 1 + 2 + sizeof(hive_beacon_t);  // Length
    *p++ = BLE_HS_ADV_TYPE_MFG_DATA;
    *p++ = 0xFF;  // Company ID low (placeholder)
    *p++ = 0xFF;  // Company ID high (placeholder)
    memcpy(p, &s_beacon, sizeof(hive_beacon_t));
    p += sizeof(hive_beacon_t);
    
    s_adv_data_len = p - s_adv_data;
}

esp_err_t hive_ble_init(const hive_ble_config_t *config, const hive_ble_callbacks_t *callbacks) {
    ESP_LOGI(TAG, "Initializing HIVE BLE");
    
    memcpy(&s_config, config, sizeof(hive_ble_config_t));
    if (callbacks) {
        memcpy(&s_callbacks, callbacks, sizeof(hive_ble_callbacks_t));
    }
    
    // Initialize beacon
    s_beacon.version_caps_hi = (1 << 4) | ((config->capabilities >> 8) & 0x0F);
    s_beacon.caps_lo = config->capabilities & 0xFF;
    s_beacon.node_id_short = config->node_id;
    s_beacon.hierarchy_level = config->hierarchy_level;
    s_beacon.geohash[0] = (config->geohash >> 16) & 0xFF;
    s_beacon.geohash[1] = (config->geohash >> 8) & 0xFF;
    s_beacon.geohash[2] = config->geohash & 0xFF;
    s_beacon.battery_percent = hive_ble_get_battery_percent();
    s_beacon.seq_num = 0;
    
    // Initialize NimBLE
    esp_err_t ret = nimble_port_init();
    if (ret != ESP_OK) {
        ESP_LOGE(TAG, "NimBLE port init failed: %d", ret);
        return ret;
    }
    
    // Initialize GATT server
    ble_svc_gap_init();
    ble_svc_gatt_init();
    hive_gatt_init();
    
    // Set device name
    char name[32];
    snprintf(name, sizeof(name), "HIVE-%08lX", config->node_id);
    ble_svc_gap_device_name_set(name);
    
    // Configure callbacks
    ble_hs_cfg.sync_cb = ble_app_on_sync;
    ble_hs_cfg.reset_cb = NULL;
    
    // Start NimBLE task
    nimble_port_freertos_init(ble_host_task);
    
    ESP_LOGI(TAG, "HIVE BLE initialized, node ID: 0x%08lX", config->node_id);
    return ESP_OK;
}

static void ble_app_on_sync(void) {
    int rc;
    
    // Determine address type
    rc = ble_hs_id_infer_auto(0, &s_own_addr_type);
    if (rc != 0) {
        ESP_LOGE(TAG, "Failed to determine address type: %d", rc);
        return;
    }
    
    // Start advertising
    hive_ble_start_advertising();
}

esp_err_t hive_ble_start_advertising(void) {
    struct ble_gap_adv_params adv_params;
    int rc;
    
    // Build advertising data
    build_adv_data();
    
    // Set advertising data
    rc = ble_gap_adv_set_data(s_adv_data, s_adv_data_len);
    if (rc != 0) {
        ESP_LOGE(TAG, "Failed to set adv data: %d", rc);
        return ESP_FAIL;
    }
    
    // Configure advertising parameters
    memset(&adv_params, 0, sizeof(adv_params));
    adv_params.conn_mode = BLE_GAP_CONN_MODE_UND;  // Connectable
    adv_params.disc_mode = BLE_GAP_DISC_MODE_GEN;  // General discoverable
    
    // Convert interval from ms to 0.625ms units
    adv_params.itvl_min = (s_config.adv_interval_ms * 1000) / 625;
    adv_params.itvl_max = adv_params.itvl_min + 16;  // Small range
    
    // Start advertising
    rc = ble_gap_adv_start(s_own_addr_type, NULL, BLE_HS_FOREVER,
                           &adv_params, ble_gap_event, NULL);
    if (rc != 0) {
        ESP_LOGE(TAG, "Failed to start advertising: %d", rc);
        return ESP_FAIL;
    }
    
    ESP_LOGI(TAG, "Advertising started");
    return ESP_OK;
}

esp_err_t hive_ble_stop_advertising(void) {
    int rc = ble_gap_adv_stop();
    if (rc != 0 && rc != BLE_HS_EALREADY) {
        ESP_LOGE(TAG, "Failed to stop advertising: %d", rc);
        return ESP_FAIL;
    }
    return ESP_OK;
}

esp_err_t hive_ble_update_beacon(const hive_beacon_t *beacon) {
    memcpy(&s_beacon, beacon, sizeof(hive_beacon_t));
    s_beacon.seq_num++;
    
    // Rebuild and update advertising data
    build_adv_data();
    
    int rc = ble_gap_adv_set_data(s_adv_data, s_adv_data_len);
    if (rc != 0) {
        ESP_LOGE(TAG, "Failed to update beacon: %d", rc);
        return ESP_FAIL;
    }
    
    return ESP_OK;
}

static int ble_gap_event(struct ble_gap_event *event, void *arg) {
    switch (event->type) {
        case BLE_GAP_EVENT_CONNECT:
            ESP_LOGI(TAG, "Connection %s, handle=%d",
                     event->connect.status == 0 ? "established" : "failed",
                     event->connect.conn_handle);
            
            if (event->connect.status == 0) {
                s_conn_handle = event->connect.conn_handle;
                
                // Request MTU exchange
                ble_gattc_exchange_mtu(event->connect.conn_handle, NULL, NULL);
                
                if (s_callbacks.on_connect) {
                    s_callbacks.on_connect(event->connect.conn_handle);
                }
            } else {
                // Connection failed, restart advertising
                hive_ble_start_advertising();
            }
            break;
            
        case BLE_GAP_EVENT_DISCONNECT:
            ESP_LOGI(TAG, "Disconnected, reason=%d", event->disconnect.reason);
            s_conn_handle = BLE_HS_CONN_HANDLE_NONE;
            
            if (s_callbacks.on_disconnect) {
                s_callbacks.on_disconnect(event->disconnect.conn.conn_handle,
                                         event->disconnect.reason);
            }
            
            // Restart advertising
            hive_ble_start_advertising();
            break;
            
        case BLE_GAP_EVENT_MTU:
            ESP_LOGI(TAG, "MTU updated: %d", event->mtu.value);
            break;
            
        case BLE_GAP_EVENT_CONN_UPDATE:
            ESP_LOGI(TAG, "Connection updated");
            break;
            
        default:
            break;
    }
    
    return 0;
}

esp_err_t hive_ble_notify_sync_state(uint16_t conn_handle, const uint8_t *data, uint16_t len) {
    return hive_gatt_notify_sync_state(conn_handle, data, len);
}

bool hive_ble_is_connected(void) {
    return s_conn_handle != BLE_HS_CONN_HANDLE_NONE;
}

uint16_t hive_ble_get_mtu(uint16_t conn_handle) {
    return ble_att_mtu(conn_handle);
}

uint8_t hive_ble_get_battery_percent(void) {
    // TODO: Read actual battery level from M5Stack Core2 AXP192
    return 100;
}
```

#### main/hive_gatt.h

```c
#ifndef HIVE_GATT_H
#define HIVE_GATT_H

#include <stdint.h>
#include "esp_err.h"

// Initialize HIVE GATT service
esp_err_t hive_gatt_init(void);

// Send notification on Sync State characteristic
esp_err_t hive_gatt_notify_sync_state(uint16_t conn_handle, const uint8_t *data, uint16_t len);

// Send notification on Status characteristic
esp_err_t hive_gatt_notify_status(uint16_t conn_handle, const uint8_t *data, uint16_t len);

// Update Node Info (called when state changes)
esp_err_t hive_gatt_update_node_info(const uint8_t *data, uint16_t len);

#endif // HIVE_GATT_H
```

#### main/hive_gatt.c

```c
#include "hive_gatt.h"
#include "hive_ble.h"
#include "hive_crdt.h"

#include "esp_log.h"
#include "host/ble_hs.h"
#include "host/ble_uuid.h"
#include "services/gatt/ble_svc_gatt.h"

static const char *TAG = "HIVE_GATT";

// Service and characteristic handles
static uint16_t s_svc_handle;
static uint16_t s_node_info_handle;
static uint16_t s_sync_state_handle;
static uint16_t s_sync_data_handle;
static uint16_t s_command_handle;
static uint16_t s_status_handle;

// Characteristic values
static uint8_t s_node_info[64];
static uint8_t s_node_info_len = 0;
static uint8_t s_sync_state[256];
static uint8_t s_sync_state_len = 0;
static uint8_t s_status[16];
static uint8_t s_status_len = 0;

// GATT access callbacks
static int hive_gatt_access(uint16_t conn_handle, uint16_t attr_handle,
                            struct ble_gatt_access_ctxt *ctxt, void *arg);

// HIVE Service UUID (128-bit)
static const ble_uuid128_t hive_svc_uuid = 
    BLE_UUID128_INIT(0x79, 0xd4, 0xc3, 0xb2, 0x02, 0x0e, 0x67, 0xa5,
                     0x72, 0x43, 0xcc, 0x58, 0x0b, 0xc1, 0x7a, 0xf4);

// Characteristic UUIDs (16-bit)
static const ble_uuid16_t char_node_info_uuid = BLE_UUID16_INIT(0x0001);
static const ble_uuid16_t char_sync_state_uuid = BLE_UUID16_INIT(0x0002);
static const ble_uuid16_t char_sync_data_uuid = BLE_UUID16_INIT(0x0003);
static const ble_uuid16_t char_command_uuid = BLE_UUID16_INIT(0x0004);
static const ble_uuid16_t char_status_uuid = BLE_UUID16_INIT(0x0005);

// GATT service definition
static const struct ble_gatt_svc_def hive_gatt_svcs[] = {
    {
        .type = BLE_GATT_SVC_TYPE_PRIMARY,
        .uuid = &hive_svc_uuid.u,
        .characteristics = (struct ble_gatt_chr_def[]) {
            {
                // Node Info - Read only
                .uuid = &char_node_info_uuid.u,
                .access_cb = hive_gatt_access,
                .flags = BLE_GATT_CHR_F_READ | BLE_GATT_CHR_F_READ_ENC,
                .val_handle = &s_node_info_handle,
            },
            {
                // Sync State - Read + Notify
                .uuid = &char_sync_state_uuid.u,
                .access_cb = hive_gatt_access,
                .flags = BLE_GATT_CHR_F_READ | BLE_GATT_CHR_F_NOTIFY | BLE_GATT_CHR_F_READ_ENC,
                .val_handle = &s_sync_state_handle,
            },
            {
                // Sync Data - Write + Indicate
                .uuid = &char_sync_data_uuid.u,
                .access_cb = hive_gatt_access,
                .flags = BLE_GATT_CHR_F_WRITE | BLE_GATT_CHR_F_INDICATE | BLE_GATT_CHR_F_WRITE_ENC,
                .val_handle = &s_sync_data_handle,
            },
            {
                // Command - Write only
                .uuid = &char_command_uuid.u,
                .access_cb = hive_gatt_access,
                .flags = BLE_GATT_CHR_F_WRITE | BLE_GATT_CHR_F_WRITE_ENC,
                .val_handle = &s_command_handle,
            },
            {
                // Status - Read + Notify
                .uuid = &char_status_uuid.u,
                .access_cb = hive_gatt_access,
                .flags = BLE_GATT_CHR_F_READ | BLE_GATT_CHR_F_NOTIFY | BLE_GATT_CHR_F_READ_ENC,
                .val_handle = &s_status_handle,
            },
            { 0 }  // End of characteristics
        },
    },
    { 0 }  // End of services
};

static int hive_gatt_access(uint16_t conn_handle, uint16_t attr_handle,
                            struct ble_gatt_access_ctxt *ctxt, void *arg) {
    int rc;
    
    switch (ctxt->op) {
        case BLE_GATT_ACCESS_OP_READ_CHR:
            if (attr_handle == s_node_info_handle) {
                rc = os_mbuf_append(ctxt->om, s_node_info, s_node_info_len);
                return rc == 0 ? 0 : BLE_ATT_ERR_INSUFFICIENT_RES;
            }
            else if (attr_handle == s_sync_state_handle) {
                rc = os_mbuf_append(ctxt->om, s_sync_state, s_sync_state_len);
                return rc == 0 ? 0 : BLE_ATT_ERR_INSUFFICIENT_RES;
            }
            else if (attr_handle == s_status_handle) {
                rc = os_mbuf_append(ctxt->om, s_status, s_status_len);
                return rc == 0 ? 0 : BLE_ATT_ERR_INSUFFICIENT_RES;
            }
            break;
            
        case BLE_GATT_ACCESS_OP_WRITE_CHR:
            if (attr_handle == s_sync_data_handle) {
                uint16_t len = OS_MBUF_PKTLEN(ctxt->om);
                uint8_t data[256];
                
                if (len > sizeof(data)) {
                    return BLE_ATT_ERR_INVALID_ATTR_VALUE_LEN;
                }
                
                rc = ble_hs_mbuf_to_flat(ctxt->om, data, len, NULL);
                if (rc != 0) {
                    return BLE_ATT_ERR_UNLIKELY;
                }
                
                ESP_LOGI(TAG, "Received sync data, len=%d", len);
                
                // Process sync data via CRDT layer
                hive_crdt_process_sync(data, len);
                
                return 0;
            }
            else if (attr_handle == s_command_handle) {
                uint16_t len = OS_MBUF_PKTLEN(ctxt->om);
                uint8_t data[64];
                
                if (len > sizeof(data)) {
                    return BLE_ATT_ERR_INVALID_ATTR_VALUE_LEN;
                }
                
                rc = ble_hs_mbuf_to_flat(ctxt->om, data, len, NULL);
                if (rc != 0) {
                    return BLE_ATT_ERR_UNLIKELY;
                }
                
                ESP_LOGI(TAG, "Received command, len=%d", len);
                
                // Process command
                hive_crdt_process_command(data, len);
                
                return 0;
            }
            break;
            
        default:
            break;
    }
    
    return BLE_ATT_ERR_UNLIKELY;
}

esp_err_t hive_gatt_init(void) {
    int rc;
    
    rc = ble_gatts_count_cfg(hive_gatt_svcs);
    if (rc != 0) {
        ESP_LOGE(TAG, "Failed to count GATT config: %d", rc);
        return ESP_FAIL;
    }
    
    rc = ble_gatts_add_svcs(hive_gatt_svcs);
    if (rc != 0) {
        ESP_LOGE(TAG, "Failed to add GATT services: %d", rc);
        return ESP_FAIL;
    }
    
    ESP_LOGI(TAG, "HIVE GATT service initialized");
    return ESP_OK;
}

esp_err_t hive_gatt_notify_sync_state(uint16_t conn_handle, const uint8_t *data, uint16_t len) {
    struct os_mbuf *om;
    int rc;
    
    // Update cached state
    if (len <= sizeof(s_sync_state)) {
        memcpy(s_sync_state, data, len);
        s_sync_state_len = len;
    }
    
    // Create notification
    om = ble_hs_mbuf_from_flat(data, len);
    if (om == NULL) {
        return ESP_ERR_NO_MEM;
    }
    
    rc = ble_gattc_notify_custom(conn_handle, s_sync_state_handle, om);
    if (rc != 0) {
        ESP_LOGE(TAG, "Failed to send notification: %d", rc);
        return ESP_FAIL;
    }
    
    return ESP_OK;
}

esp_err_t hive_gatt_update_node_info(const uint8_t *data, uint16_t len) {
    if (len > sizeof(s_node_info)) {
        return ESP_ERR_INVALID_SIZE;
    }
    
    memcpy(s_node_info, data, len);
    s_node_info_len = len;
    
    return ESP_OK;
}
```

#### main/hive_crdt.h

```c
#ifndef HIVE_CRDT_H
#define HIVE_CRDT_H

#include <stdint.h>
#include <stdbool.h>
#include "esp_err.h"

// LWW Register (Last-Writer-Wins)
typedef struct {
    uint8_t value[64];
    uint8_t value_len;
    uint64_t timestamp;
    uint32_t node_id;
} hive_lww_register_t;

// G-Counter (Grow-only counter)
typedef struct {
    uint32_t counts[8];  // Max 8 nodes
    uint8_t node_count;
} hive_gcounter_t;

// Node state (minimal CRDT state for HIVE Lite)
typedef struct {
    // Position (LWW)
    hive_lww_register_t position;
    
    // Health (LWW)
    hive_lww_register_t health;
    
    // Button presses (G-Counter)
    hive_gcounter_t button_count;
    
    // LED state (LWW)
    hive_lww_register_t led_color;
    
    // Sequence number for sync
    uint32_t seq_num;
    
} hive_node_state_t;

// Initialize CRDT state
esp_err_t hive_crdt_init(uint32_t node_id);

// Get current state
const hive_node_state_t* hive_crdt_get_state(void);

// Update local state
esp_err_t hive_crdt_update_position(float lat, float lon, float alt);
esp_err_t hive_crdt_update_health(uint8_t battery, uint8_t heart_rate);
esp_err_t hive_crdt_increment_button(void);
esp_err_t hive_crdt_set_led_color(uint8_t r, uint8_t g, uint8_t b);

// Process incoming sync data
esp_err_t hive_crdt_process_sync(const uint8_t *data, uint16_t len);

// Process incoming command
esp_err_t hive_crdt_process_command(const uint8_t *data, uint16_t len);

// Encode state for sync
uint16_t hive_crdt_encode_state(uint8_t *buf, uint16_t max_len);

// Check if state has changed since last sync
bool hive_crdt_has_changes(void);

// Mark state as synced
void hive_crdt_mark_synced(void);

#endif // HIVE_CRDT_H
```

#### main/main.c

```c
#include <stdio.h>
#include <string.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/timers.h"
#include "esp_log.h"
#include "esp_system.h"
#include "nvs_flash.h"

#include "hive_ble.h"
#include "hive_crdt.h"
#include "sensors.h"

// M5Stack Core2 includes
// #include "m5core2.h"  // You'll need M5Stack component

static const char *TAG = "HIVE_MAIN";

// Configuration
#define NODE_ID         0x12345678  // Unique node ID (change per device!)
#define SYNC_INTERVAL_MS 5000       // Sync every 5 seconds
#define SENSOR_INTERVAL_MS 1000     // Read sensors every 1 second

static TimerHandle_t s_sync_timer;
static TimerHandle_t s_sensor_timer;

// Generate unique node ID from MAC address
static uint32_t get_node_id(void) {
    uint8_t mac[6];
    esp_read_mac(mac, ESP_MAC_BT);
    return ((uint32_t)mac[2] << 24) | ((uint32_t)mac[3] << 16) |
           ((uint32_t)mac[4] << 8) | mac[5];
}

// BLE callbacks
static void on_connect(uint16_t conn_handle) {
    ESP_LOGI(TAG, "Connected! Handle: %d", conn_handle);
    
    // Update display to show connected status
    // m5core2_lcd_set_text("BLE Connected");
}

static void on_disconnect(uint16_t conn_handle, int reason) {
    ESP_LOGI(TAG, "Disconnected! Reason: %d", reason);
    
    // Update display to show disconnected status
    // m5core2_lcd_set_text("BLE Disconnected");
}

static void on_sync_data(uint16_t conn_handle, const uint8_t *data, uint16_t len) {
    ESP_LOGI(TAG, "Sync data received, len=%d", len);
    // Handled in GATT callback
}

static void on_command(uint16_t conn_handle, const uint8_t *data, uint16_t len) {
    ESP_LOGI(TAG, "Command received, len=%d", len);
    // Handled in GATT callback
}

// Timer callback: sync state to parent
static void sync_timer_callback(TimerHandle_t timer) {
    if (!hive_ble_is_connected()) {
        return;
    }
    
    if (!hive_crdt_has_changes()) {
        return;
    }
    
    uint8_t buf[256];
    uint16_t len = hive_crdt_encode_state(buf, sizeof(buf));
    
    if (len > 0) {
        // Note: In real implementation, get conn_handle from connection manager
        hive_ble_notify_sync_state(0, buf, len);
        hive_crdt_mark_synced();
        ESP_LOGI(TAG, "Synced %d bytes", len);
    }
}

// Timer callback: read sensors
static void sensor_timer_callback(TimerHandle_t timer) {
    // Read accelerometer
    float ax, ay, az;
    sensors_read_accel(&ax, &ay, &az);
    
    // Update CRDT state (simplified: use accel as "position")
    hive_crdt_update_position(ax, ay, az);
    
    // Read battery (from AXP192 on M5Stack Core2)
    uint8_t battery = hive_ble_get_battery_percent();
    hive_crdt_update_health(battery, 0);
    
    // Check buttons
    if (sensors_button_pressed()) {
        hive_crdt_increment_button();
        ESP_LOGI(TAG, "Button pressed!");
    }
}

void app_main(void) {
    ESP_LOGI(TAG, "=== HIVE-Lite M5Stack Core2 Demo ===");
    
    // Initialize NVS
    esp_err_t ret = nvs_flash_init();
    if (ret == ESP_ERR_NVS_NO_FREE_PAGES || ret == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        ESP_ERROR_CHECK(nvs_flash_erase());
        ret = nvs_flash_init();
    }
    ESP_ERROR_CHECK(ret);
    
    // Initialize M5Stack Core2 hardware
    // m5core2_init();
    
    // Initialize sensors
    sensors_init();
    
    // Get unique node ID
    uint32_t node_id = get_node_id();
    ESP_LOGI(TAG, "Node ID: 0x%08lX", node_id);
    
    // Initialize CRDT state
    hive_crdt_init(node_id);
    
    // Configure BLE
    hive_ble_config_t ble_config = {
        .node_id = node_id,
        .capabilities = CAP_LITE_NODE | CAP_SENSOR_ACCEL | CAP_SENSOR_BUTTON | CAP_ACTUATOR_LED,
        .hierarchy_level = LEVEL_PLATFORM,
        .geohash = 0x9q8yy8,  // Example: Atlanta area
        .adv_interval_ms = 500,
        .tx_power_dbm = 0,
        .conn_interval_min_ms = 30,
        .conn_interval_max_ms = 50,
        .slave_latency = 0,
        .supervision_timeout_ms = 4000,
    };
    
    hive_ble_callbacks_t callbacks = {
        .on_connect = on_connect,
        .on_disconnect = on_disconnect,
        .on_sync_data = on_sync_data,
        .on_command = on_command,
    };
    
    // Initialize BLE
    ESP_ERROR_CHECK(hive_ble_init(&ble_config, &callbacks));
    
    // Create sync timer
    s_sync_timer = xTimerCreate("sync", pdMS_TO_TICKS(SYNC_INTERVAL_MS),
                                pdTRUE, NULL, sync_timer_callback);
    xTimerStart(s_sync_timer, 0);
    
    // Create sensor timer
    s_sensor_timer = xTimerCreate("sensor", pdMS_TO_TICKS(SENSOR_INTERVAL_MS),
                                  pdTRUE, NULL, sensor_timer_callback);
    xTimerStart(s_sensor_timer, 0);
    
    ESP_LOGI(TAG, "HIVE-Lite node started, advertising...");
    
    // Main loop
    while (1) {
        vTaskDelay(pdMS_TO_TICKS(1000));
        
        // Display status on LCD
        // m5core2_lcd_printf("HIVE Node\nID: %08lX\nBatt: %d%%\nConn: %s",
        //     node_id, hive_ble_get_battery_percent(),
        //     hive_ble_is_connected() ? "Yes" : "No");
    }
}
```

---

### Android Test App (Samsung Tablet)

For the Samsung tablet, you'll need an Android app that acts as the GATT Central.

#### Key Components

```kotlin
// HiveBleScanner.kt
class HiveBleScanner(private val context: Context) {
    private val bluetoothManager = context.getSystemService(BluetoothManager::class.java)
    private val bluetoothAdapter = bluetoothManager.adapter
    private val scanner = bluetoothAdapter.bluetoothLeScanner
    
    private val HIVE_SERVICE_UUID = UUID.fromString("f47ac10b-58cc-4372-a567-0e02b2c3d479")
    
    fun startScan(callback: (ScanResult) -> Unit) {
        val filter = ScanFilter.Builder()
            .setServiceUuid(ParcelUuid(HIVE_SERVICE_UUID))
            .build()
        
        val settings = ScanSettings.Builder()
            .setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)
            .build()
        
        scanner.startScan(listOf(filter), settings, object : ScanCallback() {
            override fun onScanResult(callbackType: Int, result: ScanResult) {
                callback(result)
            }
        })
    }
}

// HiveGattClient.kt
class HiveGattClient(private val context: Context) {
    private var gatt: BluetoothGatt? = null
    
    companion object {
        val HIVE_SERVICE_UUID = UUID.fromString("f47ac10b-58cc-4372-a567-0e02b2c3d479")
        val CHAR_NODE_INFO = UUID.fromString("00000001-0000-1000-8000-00805f9b34fb")
        val CHAR_SYNC_STATE = UUID.fromString("00000002-0000-1000-8000-00805f9b34fb")
        val CHAR_SYNC_DATA = UUID.fromString("00000003-0000-1000-8000-00805f9b34fb")
    }
    
    fun connect(device: BluetoothDevice, callback: GattCallback) {
        gatt = device.connectGatt(context, false, object : BluetoothGattCallback() {
            override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
                if (newState == BluetoothProfile.STATE_CONNECTED) {
                    gatt.discoverServices()
                }
                callback.onConnectionStateChange(newState)
            }
            
            override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) {
                val service = gatt.getService(HIVE_SERVICE_UUID)
                service?.let {
                    // Enable notifications on Sync State
                    val syncStateChar = it.getCharacteristic(CHAR_SYNC_STATE)
                    gatt.setCharacteristicNotification(syncStateChar, true)
                    
                    // Write to CCCD to enable notifications
                    val descriptor = syncStateChar.getDescriptor(
                        UUID.fromString("00002902-0000-1000-8000-00805f9b34fb"))
                    descriptor.value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
                    gatt.writeDescriptor(descriptor)
                }
            }
            
            override fun onCharacteristicChanged(
                gatt: BluetoothGatt,
                characteristic: BluetoothGattCharacteristic
            ) {
                if (characteristic.uuid == CHAR_SYNC_STATE) {
                    callback.onSyncStateReceived(characteristic.value)
                }
            }
        })
    }
    
    fun writeSyncData(data: ByteArray) {
        gatt?.let { g ->
            val service = g.getService(HIVE_SERVICE_UUID)
            val char = service?.getCharacteristic(CHAR_SYNC_DATA)
            char?.value = data
            g.writeCharacteristic(char)
        }
    }
}
```

---

## Test Execution Checklist

### Pre-Test Setup

- [ ] M5Stack Core2 #1 flashed with unique NODE_ID
- [ ] M5Stack Core2 #2 flashed with different NODE_ID
- [ ] Samsung tablet has test app installed
- [ ] All devices have sufficient battery
- [ ] Bluetooth enabled on all devices
- [ ] Test area clear of interference

### Test Execution

- [ ] Scenario 1: Basic Discovery - PASS / FAIL
- [ ] Scenario 2: GATT Connection - PASS / FAIL
- [ ] Scenario 3: Sensor Data Sync - PASS / FAIL
- [ ] Scenario 4: Bidirectional Sync - PASS / FAIL
- [ ] Scenario 5: Multi-Node State Visibility - PASS / FAIL
- [ ] Scenario 6: Disconnection Recovery - PASS / FAIL
- [ ] Scenario 7: Power Efficiency - PASS / FAIL

### Metrics Captured

| Metric | M5Stack #1 | M5Stack #2 | Tablet |
|--------|------------|------------|--------|
| Discovery time (s) | | | |
| Connection time (s) | | | |
| Sync latency (ms) | | | |
| MTU negotiated | | | |
| Battery drain (%/hr) | | | |

---

## Troubleshooting

### Common Issues

| Symptom | Cause | Solution |
|---------|-------|----------|
| Node not discovered | Advertising not started | Check `hive_ble_start_advertising()` |
| Connection fails | MTU mismatch | Ensure MTU negotiation |
| No notifications | CCCD not written | Enable notifications on tablet |
| Sync data lost | Buffer overflow | Implement chunked transfer |
| High power drain | Short adv interval | Increase to 500ms+ |

### Debug Logging

```bash
# ESP32: Enable verbose logging
idf.py menuconfig
# Component config -> Log output -> Default log verbosity -> Verbose

# Monitor ESP32 output
idf.py monitor
```

---

## Next Steps

1. Flash M5Stack devices with firmware
2. Build and install Android test app
3. Execute test scenarios
4. Document results
5. Iterate on issues found
