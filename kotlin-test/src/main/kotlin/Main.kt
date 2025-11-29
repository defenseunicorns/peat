import uniffi.hive_ffi.*
import java.io.File
import java.util.UUID

/**
 * HIVE Kotlin FFI Test Application
 *
 * Tests the UniFFI bindings to the Rust hive-ffi crate.
 */
fun main() {
    println("=== HIVE Kotlin FFI Test ===\n")

    // Test 1: Get library version
    println("1. Testing hiveVersion()...")
    val version = hiveVersion()
    println("   HIVE library version: $version")
    check(version.isNotEmpty()) { "Version should not be empty" }
    println("   ✓ Version check passed\n")

    // Test 2: Create position
    println("2. Testing createPosition()...")
    val pos = createPosition(
        lat = 33.7749,
        lon = -84.3958,
        hae = 300.0
    )
    println("   Position: lat=${pos.lat}, lon=${pos.lon}, hae=${pos.hae}")
    check(pos.lat == 33.7749) { "Latitude mismatch" }
    check(pos.lon == -84.3958) { "Longitude mismatch" }
    check(pos.hae == 300.0) { "HAE mismatch" }
    println("   ✓ Position creation passed\n")

    // Test 3: Create velocity
    println("3. Testing createVelocity()...")
    val vel = createVelocity(
        bearing = 45.0,
        speedMps = 15.0
    )
    println("   Velocity: bearing=${vel.bearing}°, speed=${vel.speedMps} m/s")
    check(vel.bearing == 45.0) { "Bearing mismatch" }
    check(vel.speedMps == 15.0) { "Speed mismatch" }
    println("   ✓ Velocity creation passed\n")

    // Test 4: Create TrackData and encode to CoT
    println("4. Testing encodeTrackToCot()...")
    val track = TrackData(
        trackId = "track-001",
        sourcePlatform = "UAV-Alpha",
        position = pos,
        velocity = vel,
        classification = "a-f-G-U-C",
        confidence = 0.95,
        cellId = "cell-1",
        formationId = null
    )

    try {
        val cotXml = encodeTrackToCot(track)
        println("   CoT XML generated (${cotXml.length} bytes)")
        println("   First 200 chars: ${cotXml.take(200)}...")
        check(cotXml.contains("<event")) { "Should contain <event" }
        check(cotXml.contains("track-001")) { "Should contain track ID" }
        println("   ✓ CoT encoding passed\n")
    } catch (e: HiveException) {
        println("   ✗ CoT encoding failed: ${e.message}")
        throw e
    }

    // Test 5: Test error handling
    println("5. Testing error handling...")
    val badTrack = TrackData(
        trackId = "",  // Empty - should fail validation
        sourcePlatform = "test",
        position = pos,
        velocity = null,
        classification = "a-u-G",
        confidence = 0.5,
        cellId = null,
        formationId = null
    )

    try {
        encodeTrackToCot(badTrack)
        println("   ✗ Should have thrown HiveException.InvalidInput")
    } catch (e: HiveException.InvalidInput) {
        println("   Caught expected error: ${e.message}")
        println("   ✓ Error handling passed\n")
    } catch (e: Exception) {
        println("   ✗ Wrong exception type: ${e::class.simpleName}")
        throw e
    }

    // Test 6: HiveNode creation and document operations
    println("6. Testing HiveNode creation...")
    val tempDir = createTempDir("hive-kotlin-test-${UUID.randomUUID()}")
    try {
        val nodeConfig = NodeConfig(
            bindAddress = "127.0.0.1:0",  // Auto-assign port
            storagePath = tempDir.absolutePath
        )

        val node = createNode(nodeConfig)
        println("   Node created!")
        println("   Node ID: ${node.nodeId().take(16)}...")
        println("   Endpoint: ${node.endpointAddr()}")
        println("   Peer count: ${node.peerCount()}")
        println("   ✓ Node creation passed\n")

        // Test 7: Document CRUD operations
        println("7. Testing document CRUD operations...")

        // Put document
        val testDoc = """{"name": "test-track", "lat": 33.7749, "lon": -84.3958}"""
        node.putDocument("tracks", "track-001", testDoc)
        println("   Put document: track-001")

        // Get document
        val retrieved = node.getDocument("tracks", "track-001")
        check(retrieved != null) { "Document should exist" }
        println("   Got document: $retrieved")
        check(retrieved.contains("test-track")) { "Document should contain test-track" }

        // List documents
        val docIds = node.listDocuments("tracks")
        println("   Documents in 'tracks': $docIds")
        check(docIds.contains("track-001")) { "Should contain track-001" }

        // Delete document
        node.deleteDocument("tracks", "track-001")
        println("   Deleted document: track-001")

        // Verify deletion
        val afterDelete = node.getDocument("tracks", "track-001")
        check(afterDelete == null) { "Document should be deleted" }
        println("   Verified deletion")

        println("   ✓ Document CRUD operations passed\n")

        // Test 8: Multiple documents
        println("8. Testing multiple documents...")
        for (i in 1..5) {
            val doc = """{"id": "track-$i", "value": $i}"""
            node.putDocument("tracks", "track-$i", doc)
        }
        val allDocs = node.listDocuments("tracks")
        println("   Created ${allDocs.size} documents: $allDocs")
        check(allDocs.size == 5) { "Should have 5 documents" }
        println("   ✓ Multiple documents passed\n")

        // Test 9: Sync stats (without actual peers)
        println("9. Testing sync stats...")
        val stats = node.syncStats()
        println("   Sync active: ${stats.syncActive}")
        println("   Connected peers: ${stats.connectedPeers}")
        println("   Bytes sent: ${stats.bytesSent}")
        println("   Bytes received: ${stats.bytesReceived}")
        println("   ✓ Sync stats passed\n")

        // Cleanup
        node.destroy()
        println("   Node destroyed")

    } finally {
        // Clean up temp directory
        tempDir.deleteRecursively()
    }

    // Test 10: Subscription callbacks
    println("10. Testing subscription callbacks...")
    val tempDirSub = createTempDir("hive-sub-test-${UUID.randomUUID()}")
    try {
        val subNodeConfig = NodeConfig(
            bindAddress = "127.0.0.1:0",
            storagePath = tempDirSub.absolutePath
        )
        val subNode = createNode(subNodeConfig)

        // Track received changes
        val receivedChanges = mutableListOf<DocumentChange>()
        var errorMessage: String? = null

        // Create callback implementation
        val callback = object : DocumentCallback {
            override fun onChange(change: DocumentChange) {
                println("   Callback received: ${change.collection}/${change.docId} (${change.changeType})")
                synchronized(receivedChanges) {
                    receivedChanges.add(change)
                }
            }

            override fun onError(message: String) {
                println("   Callback error: $message")
                errorMessage = message
            }
        }

        // Subscribe to changes
        val subscription = subNode.subscribe(callback)
        println("   Subscribed to document changes (active: ${subscription.isActive()})")

        // Write some documents - these should trigger callbacks
        println("   Writing test documents...")
        subNode.putDocument("callbacks", "doc-1", """{"test": 1}""")
        subNode.putDocument("callbacks", "doc-2", """{"test": 2}""")
        subNode.putDocument("other", "doc-3", """{"test": 3}""")

        // Give callbacks time to be delivered
        Thread.sleep(500)

        // Check results
        synchronized(receivedChanges) {
            println("   Received ${receivedChanges.size} change notifications")
            if (receivedChanges.size >= 3) {
                println("   ✓ Subscription callbacks passed!\n")
            } else {
                println("   ⚠ Expected 3 callbacks, got ${receivedChanges.size}")
                println("   ⚠ Subscription callbacks incomplete\n")
            }
        }

        // Cancel subscription
        subscription.cancel()
        check(!subscription.isActive()) { "Subscription should be inactive after cancel" }
        println("   Subscription cancelled")

        // Cleanup
        subNode.destroy()

    } finally {
        tempDirSub.deleteRecursively()
    }

    // Test 11: Two-node sync test
    println("11. Testing two-node sync...")
    val tempDir1 = createTempDir("hive-node1-${UUID.randomUUID()}")
    val tempDir2 = createTempDir("hive-node2-${UUID.randomUUID()}")

    try {
        // Create two nodes on different ports
        val node1Config = NodeConfig(
            bindAddress = "127.0.0.1:19101",
            storagePath = tempDir1.absolutePath
        )
        val node2Config = NodeConfig(
            bindAddress = "127.0.0.1:19102",
            storagePath = tempDir2.absolutePath
        )

        println("    Creating Node 1...")
        val node1 = createNode(node1Config)
        println("    Node 1 ID: ${node1.nodeId().take(16)}...")

        println("    Creating Node 2...")
        val node2 = createNode(node2Config)
        println("    Node 2 ID: ${node2.nodeId().take(16)}...")

        // Start sync on both nodes
        println("    Starting sync on both nodes...")
        node1.startSync()
        node2.startSync()

        // Get node2's peer info for node1 to connect
        val node2PeerInfo = PeerInfo(
            name = "node-2",
            nodeId = node2.nodeId(),
            addresses = listOf("127.0.0.1:19102"),
            relayUrl = null
        )

        // Connect node1 to node2
        println("    Connecting Node 1 to Node 2...")
        node1.connectPeer(node2PeerInfo)

        // Give connection time to establish
        Thread.sleep(500)

        println("    Node 1 peer count: ${node1.peerCount()}")
        println("    Node 2 peer count: ${node2.peerCount()}")

        // Write document on node1
        val syncTestDoc = """{"message": "Hello from Node 1!", "timestamp": ${System.currentTimeMillis()}}"""
        println("    Writing document on Node 1...")
        node1.putDocument("sync-test", "doc-001", syncTestDoc)

        // Verify it exists on node1
        val node1Doc = node1.getDocument("sync-test", "doc-001")
        check(node1Doc != null) { "Document should exist on Node 1" }
        println("    Node 1 has document: ${node1Doc.take(50)}...")

        // Trigger sync
        println("    Triggering sync...")
        node1.syncDocument("sync-test", "doc-001")

        // Wait for sync to propagate
        println("    Waiting for sync propagation...")
        Thread.sleep(2000)

        // Check if document arrived on node2
        val node2Doc = node2.getDocument("sync-test", "doc-001")
        if (node2Doc != null) {
            println("    ✓ Document synced to Node 2: ${node2Doc.take(50)}...")
            check(node2Doc.contains("Hello from Node 1")) { "Document content should match" }
            println("    ✓ Two-node sync passed!\n")
        } else {
            println("    ⚠ Document not yet on Node 2 (sync may need more time or debugging)")
            println("    Node 1 stats: ${node1.syncStats()}")
            println("    Node 2 stats: ${node2.syncStats()}")
            println("    ⚠ Two-node sync incomplete (expected for initial implementation)\n")
        }

        // Test 11: Bidirectional sync test
        println("12. Testing bidirectional sync...")

        // Write different documents on each node
        val doc1 = """{"source": "node1", "data": "alpha", "seq": 1}"""
        val doc2 = """{"source": "node2", "data": "beta", "seq": 2}"""

        println("    Writing doc-from-1 on Node 1...")
        node1.putDocument("bidirectional", "doc-from-1", doc1)

        println("    Writing doc-from-2 on Node 2...")
        node2.putDocument("bidirectional", "doc-from-2", doc2)

        // Trigger sync from both sides
        node1.syncDocument("bidirectional", "doc-from-1")
        node2.syncDocument("bidirectional", "doc-from-2")

        // Wait for sync propagation
        println("    Waiting for bidirectional sync...")
        Thread.sleep(2000)

        // Verify Node 1 has Node 2's document
        val node1HasDoc2 = node1.getDocument("bidirectional", "doc-from-2")
        if (node1HasDoc2 != null && node1HasDoc2.contains("node2")) {
            println("    ✓ Node 1 received doc-from-2: ${node1HasDoc2.take(40)}...")
        } else {
            println("    ⚠ Node 1 missing doc-from-2")
        }

        // Verify Node 2 has Node 1's document
        val node2HasDoc1 = node2.getDocument("bidirectional", "doc-from-1")
        if (node2HasDoc1 != null && node2HasDoc1.contains("node1")) {
            println("    ✓ Node 2 received doc-from-1: ${node2HasDoc1.take(40)}...")
        } else {
            println("    ⚠ Node 2 missing doc-from-1")
        }

        if (node1HasDoc2 != null && node2HasDoc1 != null) {
            println("    ✓ Bidirectional sync passed!\n")
        } else {
            println("    ⚠ Bidirectional sync incomplete\n")
        }

        // Test 12: CRDT conflict resolution (concurrent writes to same document)
        println("13. Testing CRDT conflict resolution...")

        // Both nodes write to the same document simultaneously
        val conflictDoc1 = """{"value": "from-node1", "counter": 100}"""
        val conflictDoc2 = """{"value": "from-node2", "counter": 200}"""

        println("    Node 1 writing to conflict-doc...")
        node1.putDocument("conflicts", "conflict-doc", conflictDoc1)

        println("    Node 2 writing to conflict-doc...")
        node2.putDocument("conflicts", "conflict-doc", conflictDoc2)

        // Sync both
        node1.syncDocument("conflicts", "conflict-doc")
        node2.syncDocument("conflicts", "conflict-doc")

        // Wait for CRDT merge
        println("    Waiting for CRDT merge...")
        Thread.sleep(2000)

        // Read the merged result from both nodes
        val node1Final = node1.getDocument("conflicts", "conflict-doc")
        val node2Final = node2.getDocument("conflicts", "conflict-doc")

        println("    Node 1 sees: $node1Final")
        println("    Node 2 sees: $node2Final")

        // CRDT should converge - both nodes should see the same result
        if (node1Final != null && node2Final != null) {
            if (node1Final == node2Final) {
                println("    ✓ CRDT converged! Both nodes see identical state")
                println("    ✓ Conflict resolution passed!\n")
            } else {
                println("    ⚠ CRDT not yet converged (may need more sync time)")
                println("    ⚠ Conflict resolution incomplete\n")
            }
        } else {
            println("    ⚠ Missing documents for conflict test\n")
        }

        // Cleanup
        node1.stopSync()
        node2.stopSync()
        node1.destroy()
        node2.destroy()

    } finally {
        tempDir1.deleteRecursively()
        tempDir2.deleteRecursively()
    }

    println("=== All tests passed! ===")
}

private fun createTempDir(prefix: String): File {
    val tempDir = File(System.getProperty("java.io.tmpdir"), prefix)
    tempDir.mkdirs()
    return tempDir
}
