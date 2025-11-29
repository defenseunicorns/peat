package com.atakmap.android.hive.plugin.model

/**
 * Represents a track detected/maintained by HIVE for display in ATAK.
 *
 * Tracks are entities detected by HIVE platforms (persons, vehicles, etc.)
 * and correlated across multiple sensors.
 */
data class HiveTrack(
    /** Unique track identifier */
    val id: String,

    /** Source platform that detected this track */
    val sourcePlatform: String,

    /** Cell that owns this track */
    val cellId: String? = null,

    /** Formation that owns this track */
    val formationId: String? = null,

    /** Track position latitude (WGS84) */
    val lat: Double,

    /** Track position longitude (WGS84) */
    val lon: Double,

    /** Height above ellipsoid in meters */
    val hae: Double? = null,

    /** Circular Error Probable in meters */
    val cep: Double? = null,

    /** Track heading in degrees (0 = North) */
    val heading: Double? = null,

    /** Track speed in m/s */
    val speed: Double? = null,

    /** MIL-STD-2525 classification (e.g., "a-h-G" for hostile ground) */
    val classification: String,

    /** Detection confidence (0.0 - 1.0) */
    val confidence: Double,

    /** Track category */
    val category: Category = Category.UNKNOWN,

    /** Additional attributes */
    val attributes: Map<String, String> = emptyMap(),

    /** Track creation timestamp (epoch millis) */
    val createdAt: Long = System.currentTimeMillis(),

    /** Last update timestamp (epoch millis) */
    val lastUpdate: Long = System.currentTimeMillis()
) {
    /**
     * Track category enumeration
     */
    enum class Category {
        /** Person of interest */
        PERSON,

        /** Vehicle */
        VEHICLE,

        /** Aircraft */
        AIRCRAFT,

        /** Maritime vessel */
        VESSEL,

        /** Fixed installation */
        INSTALLATION,

        /** Unknown/unclassified */
        UNKNOWN
    }

    /**
     * Generate a CoT UID for this track
     */
    fun toCotUid(): String = "HIVE-TRACK-$id"

    /**
     * Get CoT type from classification or derive from category
     */
    fun toCotType(): String {
        // Use classification if it's already in CoT format
        if (classification.startsWith("a-")) {
            return classification
        }

        // Otherwise, derive from category
        return when (category) {
            Category.PERSON -> "a-u-G-U-C-I"    // Unknown Ground Unit - Infantry
            Category.VEHICLE -> "a-u-G-E-V"     // Unknown Ground Equipment - Vehicle
            Category.AIRCRAFT -> "a-u-A"        // Unknown Air
            Category.VESSEL -> "a-u-S"          // Unknown Surface
            Category.INSTALLATION -> "a-u-G-I"  // Unknown Ground Installation
            Category.UNKNOWN -> "a-u-G"         // Unknown Ground
        }
    }

    /**
     * Check if this track is considered stale (no updates in threshold)
     */
    fun isStale(staleThresholdMs: Long = 60_000): Boolean {
        return System.currentTimeMillis() - lastUpdate > staleThresholdMs
    }
}
