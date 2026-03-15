/*
 * Copyright (c) 2026 Defense Unicorns.  All rights reserved.
 */

package com.defenseunicorns.atak.peat.model

/**
 * Represents an individual Peat platform (UAV, UGV, etc.) for display in ATAK.
 */
data class PeatPlatform(
    /** Unique platform identifier */
    val id: String,

    /** Human-readable callsign */
    val callsign: String,

    /** Platform type (UAV, UGV, etc.) */
    val platformType: PlatformType,

    /** Current position latitude (WGS84) */
    val lat: Double,

    /** Current position longitude (WGS84) */
    val lon: Double,

    /** Height above ellipsoid in meters */
    val hae: Double? = null,

    /** Current heading in degrees (0 = North) */
    val heading: Double? = null,

    /** Current speed in m/s */
    val speed: Double? = null,

    /** Course over ground in degrees */
    val course: Double? = null,

    /** Vertical speed in m/s */
    val verticalSpeed: Double? = null,

    /** Position accuracy (CEP) in meters */
    val positionAccuracy: Double? = null,

    /** Cell membership */
    val cellId: String? = null,

    /** Platform capabilities */
    val capabilities: List<String> = emptyList(),

    /** Operational status */
    val status: Status = Status.OPERATIONAL,

    /** Battery/fuel percentage (0-100) */
    val batteryPercent: Int? = null,

    /** Estimated battery time remaining in minutes (computed from drain rate) */
    val batteryTimeRemainingMinutes: Int? = null,

    /** Heart rate in BPM (for wearable devices) */
    val heartRate: Int? = null,

    /** Communications quality */
    val commsQuality: CommsQuality? = null,

    /** Sensor status by sensor name */
    val sensorStatus: Map<String, SensorStatus>? = null,

    /** Current task assignment */
    val currentTask: String? = null,

    /** Mission identifier */
    val missionId: String? = null,

    /** Last update timestamp (epoch millis) */
    val lastUpdate: Long = System.currentTimeMillis()
) {
    /**
     * Platform type enumeration
     */
    enum class PlatformType {
        /** Unmanned Aerial Vehicle */
        UAV,

        /** Unmanned Ground Vehicle */
        UGV,

        /** Unmanned Surface Vehicle */
        USV,

        /** Unmanned Underwater Vehicle */
        UUV,

        /** Human operator */
        OPERATOR,

        /** Soldier/dismount (ATAK PLI) */
        SOLDIER,

        /** Fixed sensor */
        SENSOR,

        /** Unknown type */
        UNKNOWN
    }

    /**
     * Platform operational status
     */
    enum class Status {
        /** Fully operational */
        OPERATIONAL,

        /** Reduced capability */
        DEGRADED,

        /** Not communicating */
        LOST_COMMS,

        /** Battery/fuel critical */
        LOW_POWER,

        /** Mission complete, returning */
        RTB,

        /** Offline */
        OFFLINE,

        /** Emergency/SOS active - highest priority */
        EMERGENCY
    }

    /**
     * Generate a CoT UID for this platform
     */
    fun toCotUid(): String = "Peat-PLAT-$id"

    /**
     * Generate a CoT type string based on platform type
     */
    fun toCotType(): String = when (platformType) {
        PlatformType.UAV -> "a-f-A-M-F-Q"      // Friendly Air - Military Fixed Wing - UAV
        PlatformType.UGV -> "a-f-G-U-C"        // Friendly Ground Unit - Combat
        PlatformType.USV -> "a-f-S-X"          // Friendly Surface - Other
        PlatformType.UUV -> "a-f-U-X"          // Friendly Subsurface - Other
        PlatformType.OPERATOR -> "a-f-G-U-C-I" // Friendly Ground Unit - Infantry
        PlatformType.SOLDIER -> "a-f-G-U-C-I"  // Friendly Ground Unit - Infantry (PLI)
        PlatformType.SENSOR -> "a-f-G-E-S"     // Friendly Ground Equipment - Sensor
        PlatformType.UNKNOWN -> "a-u-G"        // Unknown Ground
    }

    /**
     * Check if this platform has stale data (older than threshold)
     */
    fun isStale(thresholdMs: Long = 60_000): Boolean =
        System.currentTimeMillis() - lastUpdate > thresholdMs

    /**
     * Get staleness as human-readable string
     */
    fun getStalenessString(): String {
        val ageMs = System.currentTimeMillis() - lastUpdate
        return when {
            ageMs < 5_000 -> "Just now"
            ageMs < 60_000 -> "${ageMs / 1000}s ago"
            ageMs < 3_600_000 -> "${ageMs / 60_000}m ago"
            else -> "${ageMs / 3_600_000}h ago"
        }
    }
}

/**
 * Communications quality levels
 */
enum class CommsQuality {
    /** Excellent signal/connectivity */
    EXCELLENT,
    /** Good signal/connectivity */
    GOOD,
    /** Degraded but usable */
    DEGRADED,
    /** Poor connectivity */
    POOR,
    /** Connection lost */
    LOST
}

/**
 * Sensor operational status
 */
enum class SensorStatus {
    /** Sensor active and reporting */
    ACTIVE,
    /** Sensor available but idle */
    IDLE,
    /** Sensor degraded performance */
    DEGRADED,
    /** Sensor offline/failed */
    OFFLINE,
    /** Sensor status unknown */
    UNKNOWN
}
