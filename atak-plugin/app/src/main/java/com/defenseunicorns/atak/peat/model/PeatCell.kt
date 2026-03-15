/*
 * Copyright (c) 2026 Defense Unicorns.  All rights reserved.
 */

package com.defenseunicorns.atak.peat.model

/**
 * Represents a Peat cell (squad/team) for display in ATAK.
 *
 * A cell is a group of autonomous platforms that coordinate via the Peat protocol.
 * Cells have aggregated capabilities and a geographic center.
 */
data class PeatCell(
    /** Unique identifier for the cell */
    val id: String,

    /** Human-readable name (e.g., "Alpha Team") */
    val name: String,

    /** Current operational status */
    val status: Status,

    /** Number of platforms in this cell */
    val platformCount: Int,

    /** Geographic center latitude (WGS84) */
    val centerLat: Double,

    /** Geographic center longitude (WGS84) */
    val centerLon: Double,

    /** List of aggregated capabilities */
    val capabilities: List<String> = emptyList(),

    /** Optional parent formation ID */
    val formationId: String? = null,

    /** Cell leader platform ID */
    val leaderId: String? = null,

    /** Last update timestamp (epoch millis) */
    val lastUpdate: Long = System.currentTimeMillis()
) {
    /**
     * Cell operational status
     */
    enum class Status {
        /** Cell is fully operational */
        ACTIVE,

        /** Cell is still forming (platforms joining) */
        FORMING,

        /** Cell has degraded capability (lost platforms) */
        DEGRADED,

        /** Cell is offline (no communication) */
        OFFLINE
    }

    /**
     * Generate a CoT UID for this cell
     */
    fun toCotUid(): String = "Peat-CELL-$id"

    /**
     * Generate a CoT type string for this cell
     * Uses MIL-STD-2525: a-f-G-U-C (friendly ground unit - combat)
     */
    fun toCotType(): String = "a-f-G-U-C"
}
