package com.revolveteam.atak.hive.model

/**
 * Represents an individual HIVE platform (UAV, UGV, etc.) for display in ATAK.
 */
data class HivePlatform(
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

    /** Cell membership */
    val cellId: String? = null,

    /** Platform capabilities */
    val capabilities: List<String> = emptyList(),

    /** Operational status */
    val status: Status = Status.OPERATIONAL,

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
        OFFLINE
    }

    /**
     * Generate a CoT UID for this platform
     */
    fun toCotUid(): String = "HIVE-PLAT-$id"

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
}
