import uniffi.hive_ffi.*

fun main() {
    println("=== HIVE FFI Kotlin Test ===")
    println()

    // Test 1: Get version
    val version = hiveVersion()
    println("HIVE Version: $version")
    println()

    // Test 2: Create a track and encode to CoT
    val position = createPosition(34.0522, -118.2437, 100.0)
    val velocity = createVelocity(90.0, 15.0)

    val track = TrackData(
        trackId = "track-001",
        sourcePlatform = "kotlin-test-app",
        position = position,
        velocity = velocity,
        classification = "a-f-G-U-C",
        confidence = 0.95,
        cellId = "cell-alpha",
        formationId = null
    )

    println("Encoding track to CoT XML...")
    try {
        val cotXml = encodeTrackToCot(track)
        println("Success! CoT XML (first 500 chars):")
        println(cotXml.take(500))
        if (cotXml.length > 500) println("...")
    } catch (e: HiveException) {
        println("Error: ${e.message}")
    }

    println()
    println("=== Test Complete ===")
}
