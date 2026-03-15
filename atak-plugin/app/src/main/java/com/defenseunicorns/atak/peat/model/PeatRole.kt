/*
 * Copyright (c) 2026 Defense Unicorns.  All rights reserved.
 */

package com.defenseunicorns.atak.peat.model

/**
 * Represents the user's role within the Peat hierarchy.
 *
 * For the PoC, this is configured manually in plugin settings.
 * Future versions may derive this from Peat protocol or ATAK team configuration.
 */
data class PeatRole(
    /** The hierarchy level this user operates at */
    val level: HierarchyLevel,

    /** Whether this user is a leader at their level */
    val isLeader: Boolean,

    /** The unit ID (cell/formation) at this level */
    val unitId: String,

    /** Display name for the unit */
    val unitName: String = "",

    /** Parent unit ID (e.g., platoon ID for squad member) */
    val parentUnitId: String? = null
) {
    /**
     * Hierarchy levels in military organizational structure
     */
    enum class HierarchyLevel {
        /** Squad level (8-12 personnel) */
        SQUAD,

        /** Platoon level (3-4 squads) */
        PLATOON,

        /** Company level (3-4 platoons) */
        COMPANY,

        /** Battalion level (3-5 companies) */
        BATTALION
    }

    /**
     * Get display string for the role
     */
    fun toDisplayString(): String {
        val levelName = level.name.lowercase().replaceFirstChar { it.uppercase() }
        return if (isLeader) {
            "$levelName Leader"
        } else {
            "$levelName Member"
        }
    }

    companion object {
        /**
         * Default role for new users - squad leader of Atlanta cell
         * Matches the test client's cell-atlanta-001
         */
        fun defaultRole(): PeatRole = PeatRole(
            level = HierarchyLevel.SQUAD,
            isLeader = true,  // Default to leader so squad summary shows
            unitId = "cell-atlanta-001",
            unitName = "Atlanta Squad"
        )

        /**
         * Create a squad leader role
         */
        fun squadLeader(unitId: String, unitName: String): PeatRole = PeatRole(
            level = HierarchyLevel.SQUAD,
            isLeader = true,
            unitId = unitId,
            unitName = unitName
        )

        /**
         * Create a squad member role
         */
        fun squadMember(unitId: String, unitName: String): PeatRole = PeatRole(
            level = HierarchyLevel.SQUAD,
            isLeader = false,
            unitId = unitId,
            unitName = unitName
        )

        /**
         * Create a platoon leader role
         */
        fun platoonLeader(unitId: String, unitName: String): PeatRole = PeatRole(
            level = HierarchyLevel.PLATOON,
            isLeader = true,
            unitId = unitId,
            unitName = unitName
        )
    }
}
