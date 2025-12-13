# Quick Start: M5Stack Core2 + Samsung Tablet HIVE-Lite Test

## Overview

This guide gets you from zero to testing HIVE-Lite BLE sync between:
- **2x M5Stack Core2** (ESP32 sensor nodes)
- **1x Samsung Tablet** (Android aggregator/parent)

**Estimated Setup Time**: 2-3 hours

---

## Prerequisites

### Hardware
- 2x M5Stack Core2 (or Core2 AWS)
- 1x Samsung Tablet (Android 8.0+ with BLE)
- USB-C cables for M5Stack programming
- Computer with USB ports (Linux recommended)

### Software (Development Machine)
```bash
# ESP-IDF (ESP32 development framework)
# Follow: https://docs.espressif.com/projects/esp-idf/en/latest/esp32/get-started/

# Quick install on Ubuntu/Debian:
sudo apt-get install git wget flex bison gperf python3 python3-pip \
    python3-venv cmake ninja-build ccache libffi-dev libssl-dev \
    dfu-util libusb-1.0-0

mkdir -p ~/esp
cd ~/esp
git clone -b v5.1.2 --recursive https://github.com/espressif/esp-idf.git
cd esp-idf
./install.sh esp32
source export.sh

# Android Studio (for tablet app)
# Download from: https://developer.android.com/studio
```

---

## Step 1: Create M5Stack Project

```bash
# Create project directory
mkdir -p ~/hive-btle-test/m5stack-hive-lite
cd ~/hive-btle-test/m5stack-hive-lite

# Initialize ESP-IDF project
idf.py create-project-from-example "espressif/esp-idf-lib^1.0.0:ble_adv"
# Or create manually:
```

### Project Structure

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
│   └── hive_crdt.c
└── partitions.csv
```

### CMakeLists.txt (Root)

```cmake
cmake_minimum_required(VERSION 3.16)
include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(m5stack-hive-lite)
```

### main/CMakeLists.txt

```cmake
idf_component_register(
    SRCS 
        "main.c"
        "hive_ble.c"
        "hive_gatt.c"
        "hive_crdt.c"
    INCLUDE_DIRS "."
    REQUIRES 
        bt
        nvs_flash
)
```

### sdkconfig.defaults

```
# BLE Configuration
CONFIG_BT_ENABLED=y
CONFIG_BT_NIMBLE_ENABLED=y
CONFIG_BT_NIMBLE_MAX_CONNECTIONS=3
CONFIG_BT_NIMBLE_ROLE_PERIPHERAL=y
CONFIG_BT_NIMBLE_ROLE_BROADCASTER=y
CONFIG_BT_NIMBLE_ATT_PREFERRED_MTU=256

# Logging
CONFIG_LOG_DEFAULT_LEVEL_INFO=y
CONFIG_LOG_MAXIMUM_LEVEL_DEBUG=y

# Power Management (optional)
# CONFIG_PM_ENABLE=y
# CONFIG_FREERTOS_USE_TICKLESS_IDLE=y
```

---

## Step 2: Minimal M5Stack Firmware

Create these files in the `main/` directory:

### main/hive_ble.h (Simplified)

```c
#ifndef HIVE_BLE_H
#define HIVE_BLE_H

#include <stdint.h>
#include <stdbool.h>
#include "esp_err.h"

// HIVE Service UUID
#define HIVE_SERVICE_UUID_128 \
    0x79, 0xd4, 0xc3, 0xb2, 0x02, 0x0e, 0x67, 0xa5, \
    0x72, 0x43, 0xcc, 0x58, 0x0b, 0xc1, 0x7a, 0xf4

typedef struct {
    uint32_t node_id;
    uint16_t adv_interval_ms;
} hive_ble_config_t;

typedef void (*hive_ble_sync_cb_t)(const uint8_t *data, uint16_t len);

esp_err_t hive_ble_init(const hive_ble_config_t *config);
esp_err_t hive_ble_set_sync_callback(hive_ble_sync_cb_t cb);
bool hive_ble_is_connected(void);
esp_err_t hive_ble_send_sync(const uint8_t *data, uint16_t len);

#endif
```

### main/hive_ble.c (Simplified)

```c
#include "hive_ble.h"
#include "esp_log.h"
#include "nvs_flash.h"
#include "esp_nimble_hci.h"
#include "nimble/nimble_port.h"
#include "nimble/nimble_port_freertos.h"
#include "host/ble_hs.h"
#include "host/util/util.h"
#include "services/gap/ble_svc_gap.h"
#include "services/gatt/ble_svc_gatt.h"

static const char *TAG = "HIVE_BLE";

static hive_ble_config_t s_config;
static hive_ble_sync_cb_t s_sync_cb = NULL;
static uint16_t s_conn_handle = BLE_HS_CONN_HANDLE_NONE;
static uint8_t s_own_addr_type;
static uint16_t s_sync_state_handle;

// GATT service definition
static const ble_uuid128_t hive_svc_uuid = 
    BLE_UUID128_INIT(HIVE_SERVICE_UUID_128);
static const ble_uuid16_t char_sync_uuid = BLE_UUID16_INIT(0x0002);

static int gatt_access_cb(uint16_t conn_handle, uint16_t attr_handle,
                          struct ble_gatt_access_ctxt *ctxt, void *arg);

static const struct ble_gatt_svc_def gatt_svcs[] = {
    {
        .type = BLE_GATT_SVC_TYPE_PRIMARY,
        .uuid = &hive_svc_uuid.u,
        .characteristics = (struct ble_gatt_chr_def[]) {
            {
                .uuid = &char_sync_uuid.u,
                .access_cb = gatt_access_cb,
                .flags = BLE_GATT_CHR_F_READ | BLE_GATT_CHR_F_WRITE | BLE_GATT_CHR_F_NOTIFY,
                .val_handle = &s_sync_state_handle,
            },
            { 0 }
        },
    },
    { 0 }
};

static uint8_t s_sync_data[128];
static uint8_t s_sync_data_len = 0;

static int gatt_access_cb(uint16_t conn_handle, uint16_t attr_handle,
                          struct ble_gatt_access_ctxt *ctxt, void *arg) {
    if (ctxt->op == BLE_GATT_ACCESS_OP_READ_CHR) {
        os_mbuf_append(ctxt->om, s_sync_data, s_sync_data_len);
        return 0;
    }
    
    if (ctxt->op == BLE_GATT_ACCESS_OP_WRITE_CHR) {
        uint16_t len = OS_MBUF_PKTLEN(ctxt->om);
        uint8_t buf[128];
        ble_hs_mbuf_to_flat(ctxt->om, buf, len, NULL);
        
        ESP_LOGI(TAG, "Received %d bytes", len);
        if (s_sync_cb) {
            s_sync_cb(buf, len);
        }
        return 0;
    }
    
    return 0;
}

static int gap_event_cb(struct ble_gap_event *event, void *arg);

static void start_advertising(void) {
    struct ble_gap_adv_params adv_params = {
        .conn_mode = BLE_GAP_CONN_MODE_UND,
        .disc_mode = BLE_GAP_DISC_MODE_GEN,
    };
    
    // Build advertising data
    struct ble_hs_adv_fields fields = {
        .flags = BLE_HS_ADV_F_DISC_GEN | BLE_HS_ADV_F_BREDR_UNSUP,
        .uuids128 = (ble_uuid128_t[]) { hive_svc_uuid },
        .num_uuids128 = 1,
        .uuids128_is_complete = 1,
    };
    
    ble_gap_adv_set_fields(&fields);
    ble_gap_adv_start(s_own_addr_type, NULL, BLE_HS_FOREVER, &adv_params, gap_event_cb, NULL);
    
    ESP_LOGI(TAG, "Advertising started");
}

static int gap_event_cb(struct ble_gap_event *event, void *arg) {
    switch (event->type) {
        case BLE_GAP_EVENT_CONNECT:
            if (event->connect.status == 0) {
                s_conn_handle = event->connect.conn_handle;
                ESP_LOGI(TAG, "Connected, handle=%d", s_conn_handle);
            } else {
                start_advertising();
            }
            break;
            
        case BLE_GAP_EVENT_DISCONNECT:
            s_conn_handle = BLE_HS_CONN_HANDLE_NONE;
            ESP_LOGI(TAG, "Disconnected");
            start_advertising();
            break;
    }
    return 0;
}

static void on_sync(void) {
    ble_hs_id_infer_auto(0, &s_own_addr_type);
    start_advertising();
}

static void nimble_host_task(void *param) {
    nimble_port_run();
    nimble_port_freertos_deinit();
}

esp_err_t hive_ble_init(const hive_ble_config_t *config) {
    s_config = *config;
    
    esp_err_t ret = nimble_port_init();
    if (ret != ESP_OK) return ret;
    
    ble_svc_gap_init();
    ble_svc_gatt_init();
    
    ble_gatts_count_cfg(gatt_svcs);
    ble_gatts_add_svcs(gatt_svcs);
    
    char name[32];
    snprintf(name, sizeof(name), "HIVE-%08lX", config->node_id);
    ble_svc_gap_device_name_set(name);
    
    ble_hs_cfg.sync_cb = on_sync;
    
    nimble_port_freertos_init(nimble_host_task);
    
    ESP_LOGI(TAG, "BLE initialized, node_id=0x%08lX", config->node_id);
    return ESP_OK;
}

esp_err_t hive_ble_set_sync_callback(hive_ble_sync_cb_t cb) {
    s_sync_cb = cb;
    return ESP_OK;
}

bool hive_ble_is_connected(void) {
    return s_conn_handle != BLE_HS_CONN_HANDLE_NONE;
}

esp_err_t hive_ble_send_sync(const uint8_t *data, uint16_t len) {
    if (s_conn_handle == BLE_HS_CONN_HANDLE_NONE) {
        return ESP_ERR_INVALID_STATE;
    }
    
    // Update local cache
    memcpy(s_sync_data, data, len);
    s_sync_data_len = len;
    
    // Send notification
    struct os_mbuf *om = ble_hs_mbuf_from_flat(data, len);
    ble_gattc_notify_custom(s_conn_handle, s_sync_state_handle, om);
    
    return ESP_OK;
}
```

### main/main.c (Simplified)

```c
#include <stdio.h>
#include <string.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "esp_log.h"
#include "nvs_flash.h"
#include "hive_ble.h"

static const char *TAG = "HIVE_MAIN";

// Simple sensor data structure
typedef struct __attribute__((packed)) {
    uint32_t node_id;
    uint32_t timestamp;
    int16_t accel_x;
    int16_t accel_y;
    int16_t accel_z;
    uint8_t battery;
    uint8_t button_count;
} sensor_data_t;

static sensor_data_t s_sensor_data;
static uint8_t s_button_count = 0;

// Generate unique node ID from MAC
static uint32_t get_node_id(void) {
    uint8_t mac[6];
    esp_read_mac(mac, ESP_MAC_BT);
    return ((uint32_t)mac[2] << 24) | ((uint32_t)mac[3] << 16) |
           ((uint32_t)mac[4] << 8) | mac[5];
}

// Callback when sync data received from parent
static void on_sync_received(const uint8_t *data, uint16_t len) {
    ESP_LOGI(TAG, "Received sync data: %d bytes", len);
    ESP_LOG_BUFFER_HEX(TAG, data, len);
    
    // Parse command if applicable
    if (len >= 1) {
        uint8_t cmd = data[0];
        if (cmd == 0x01) {  // LED command
            ESP_LOGI(TAG, "LED command received!");
            // TODO: Set LED color on M5Stack
        }
    }
}

// Simulate sensor reading
static void read_sensors(void) {
    // TODO: Replace with actual M5Stack sensor reading
    // Using random values for now
    s_sensor_data.accel_x = (esp_random() % 2000) - 1000;
    s_sensor_data.accel_y = (esp_random() % 2000) - 1000;
    s_sensor_data.accel_z = (esp_random() % 2000) - 1000;
    s_sensor_data.battery = 80 + (esp_random() % 20);
    s_sensor_data.button_count = s_button_count;
}

void app_main(void) {
    ESP_LOGI(TAG, "=== HIVE-Lite M5Stack Demo ===");
    
    // Initialize NVS
    esp_err_t ret = nvs_flash_init();
    if (ret == ESP_ERR_NVS_NO_FREE_PAGES || ret == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        nvs_flash_erase();
        nvs_flash_init();
    }
    
    // Get node ID
    uint32_t node_id = get_node_id();
    ESP_LOGI(TAG, "Node ID: 0x%08lX", (unsigned long)node_id);
    
    // Initialize sensor data
    s_sensor_data.node_id = node_id;
    
    // Initialize BLE
    hive_ble_config_t config = {
        .node_id = node_id,
        .adv_interval_ms = 500,
    };
    hive_ble_init(&config);
    hive_ble_set_sync_callback(on_sync_received);
    
    // Main loop
    uint32_t counter = 0;
    while (1) {
        vTaskDelay(pdMS_TO_TICKS(1000));
        
        // Update timestamp
        s_sensor_data.timestamp = counter++;
        
        // Read sensors
        read_sensors();
        
        // Send sync data if connected
        if (hive_ble_is_connected()) {
            hive_ble_send_sync((uint8_t*)&s_sensor_data, sizeof(s_sensor_data));
            ESP_LOGI(TAG, "Sent sync: ts=%lu, ax=%d, ay=%d, az=%d, batt=%d",
                     (unsigned long)s_sensor_data.timestamp,
                     s_sensor_data.accel_x,
                     s_sensor_data.accel_y,
                     s_sensor_data.accel_z,
                     s_sensor_data.battery);
        } else {
            ESP_LOGI(TAG, "Not connected, waiting...");
        }
    }
}
```

---

## Step 3: Build and Flash M5Stack

```bash
# Navigate to project
cd ~/hive-btle-test/m5stack-hive-lite

# Set target
idf.py set-target esp32

# Configure (optional - for customization)
idf.py menuconfig

# Build
idf.py build

# Flash to M5Stack (connect via USB-C)
# Find port with: ls /dev/ttyUSB* or ls /dev/ttyACM*
idf.py -p /dev/ttyUSB0 flash monitor

# Repeat for second M5Stack with different USB port
```

### Expected Output

```
I (xxx) HIVE_MAIN: === HIVE-Lite M5Stack Demo ===
I (xxx) HIVE_MAIN: Node ID: 0xABCD1234
I (xxx) HIVE_BLE: BLE initialized, node_id=0xABCD1234
I (xxx) HIVE_BLE: Advertising started
I (xxx) HIVE_MAIN: Not connected, waiting...
...
I (xxx) HIVE_BLE: Connected, handle=0
I (xxx) HIVE_MAIN: Sent sync: ts=42, ax=123, ay=-456, az=789, batt=85
```

---

## Step 4: Android Test App (Simple Version)

For quick testing, use the **nRF Connect** app from Nordic Semiconductor:
1. Install from Play Store: "nRF Connect for Mobile"
2. Scan for devices
3. Find "HIVE-XXXXXXXX" devices
4. Connect and explore GATT services
5. Read/Write to characteristics

### Custom App (Optional)

Create a minimal Android app with Kotlin:

```kotlin
// MainActivity.kt
class MainActivity : AppCompatActivity() {
    private val scanner = BluetoothAdapter.getDefaultAdapter().bluetoothLeScanner
    private val devices = mutableMapOf<String, BluetoothDevice>()
    
    private val HIVE_SERVICE = UUID.fromString("f47ac10b-58cc-4372-a567-0e02b2c3d479")
    private val HIVE_SYNC_CHAR = UUID.fromString("00000002-0000-1000-8000-00805f9b34fb")
    
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        
        // Request permissions first!
        requestBlePermissions()
    }
    
    private fun startScan() {
        val filter = ScanFilter.Builder()
            .setServiceUuid(ParcelUuid(HIVE_SERVICE))
            .build()
            
        scanner.startScan(listOf(filter), ScanSettings.Builder().build(),
            object : ScanCallback() {
                override fun onScanResult(callbackType: Int, result: ScanResult) {
                    val device = result.device
                    devices[device.address] = device
                    Log.d("HIVE", "Found: ${device.name} (${device.address})")
                }
            })
    }
    
    private fun connectToDevice(device: BluetoothDevice) {
        device.connectGatt(this, false, object : BluetoothGattCallback() {
            override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
                if (newState == BluetoothProfile.STATE_CONNECTED) {
                    Log.d("HIVE", "Connected to ${gatt.device.name}")
                    gatt.discoverServices()
                }
            }
            
            override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) {
                val service = gatt.getService(HIVE_SERVICE)
                val char = service?.getCharacteristic(HIVE_SYNC_CHAR)
                
                // Enable notifications
                gatt.setCharacteristicNotification(char, true)
                val descriptor = char?.getDescriptor(
                    UUID.fromString("00002902-0000-1000-8000-00805f9b34fb"))
                descriptor?.value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
                gatt.writeDescriptor(descriptor)
            }
            
            override fun onCharacteristicChanged(gatt: BluetoothGatt, char: BluetoothGattCharacteristic) {
                val data = char.value
                Log.d("HIVE", "Received: ${data.toHexString()}")
                
                // Parse sensor data
                if (data.size >= 16) {
                    val nodeId = ByteBuffer.wrap(data, 0, 4).order(ByteOrder.LITTLE_ENDIAN).int
                    val timestamp = ByteBuffer.wrap(data, 4, 4).order(ByteOrder.LITTLE_ENDIAN).int
                    val ax = ByteBuffer.wrap(data, 8, 2).order(ByteOrder.LITTLE_ENDIAN).short
                    val ay = ByteBuffer.wrap(data, 10, 2).order(ByteOrder.LITTLE_ENDIAN).short
                    val az = ByteBuffer.wrap(data, 12, 2).order(ByteOrder.LITTLE_ENDIAN).short
                    val battery = data[14].toInt() and 0xFF
                    
                    Log.d("HIVE", "Node: ${nodeId.toString(16)}, TS: $timestamp, " +
                          "Accel: ($ax, $ay, $az), Batt: $battery%")
                }
            }
        })
    }
}
```

---

## Step 5: Run the Test

### Test Sequence

1. **Power on M5Stack #1** - observe "Advertising started" in serial monitor
2. **Power on M5Stack #2** - observe "Advertising started" in serial monitor  
3. **Open nRF Connect on Samsung tablet**
4. **Scan** - both HIVE devices should appear
5. **Connect to HIVE-XXXXXXXX (#1)** 
   - Observe "Connected" in M5Stack serial monitor
   - See GATT service discovered
6. **Navigate to HIVE service (UUID f47ac10b...)**
7. **Enable notifications** on characteristic 0x0002
8. **Observe incoming data** - sensor readings should appear
9. **Repeat for M5Stack #2** (connect simultaneously)
10. **Write data** to characteristic - should trigger callback on M5Stack

### Expected Behavior

| Action | M5Stack Output | Tablet Output |
|--------|----------------|---------------|
| Scan | (no change) | Finds "HIVE-XXXX" |
| Connect | "Connected, handle=0" | "Connected" |
| Enable notify | (no change) | Notifications enabled |
| Wait 1s | "Sent sync: ts=N..." | Receives hex data |
| Write 0x01 | "LED command received!" | Write successful |

---

## Troubleshooting

| Issue | Solution |
|-------|----------|
| Device not found | Check BLE enabled, service UUID correct |
| Connect fails | Reset M5Stack, try again |
| No notifications | Manually enable CCCD descriptor |
| Data corrupted | Check endianness, use `__attribute__((packed))` |

---

## Next Steps

After basic testing works:

1. **Add M5Stack display** - Show connection status, sensor values
2. **Add button handling** - Increment counter on button press
3. **Implement full CRDT** - Replace simple struct with LWW registers
4. **Add bidirectional sync** - Tablet → M5Stack state propagation
5. **Multi-device mesh** - M5Stack #1 sees #2's state via tablet
6. **Power profiling** - Measure actual battery consumption

---

## Files Summary

```
~/hive-btle-test/
├── m5stack-hive-lite/
│   ├── CMakeLists.txt
│   ├── sdkconfig.defaults
│   └── main/
│       ├── CMakeLists.txt
│       ├── main.c
│       ├── hive_ble.h
│       └── hive_ble.c
└── android-hive-test/  (optional custom app)
    └── ...
```

**Total code size**: ~400 lines (enough to validate the concept)

---

**Happy Testing!** 🎉

For questions: Kit Plummer, (r)evolve - https://revolveteam.com
