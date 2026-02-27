//! Minimal Iroh Connection Test
//!
//! This test isolates the Iroh connection mechanism to debug why connections
//! are failing in the full E2E tests.

#![cfg(feature = "automerge-backend")]

use std::net::SocketAddr;
use tokio::time::{timeout, Duration};

const ALPN: &[u8] = b"test/minimal/0";

#[tokio::test]
async fn test_minimal_iroh_connection() {
    println!("=== Minimal Iroh Connection Test ===");

    // Create two endpoints
    let addr1: SocketAddr = "127.0.0.1:29001".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:29002".parse().unwrap();

    println!("  Creating endpoints...");

    let ep1 = iroh::Endpoint::builder()
        .alpns(vec![ALPN.to_vec()])
        .bind_addr_v4(match addr1 {
            SocketAddr::V4(a) => a,
            _ => unreachable!(),
        })
        .bind()
        .await
        .unwrap();

    let ep2 = iroh::Endpoint::builder()
        .alpns(vec![ALPN.to_vec()])
        .bind_addr_v4(match addr2 {
            SocketAddr::V4(a) => a,
            _ => unreachable!(),
        })
        .bind()
        .await
        .unwrap();

    println!("  EP1 ID: {:?}", ep1.id());
    println!("  EP1 Addr: {}", addr1);
    println!("  EP2 ID: {:?}", ep2.id());
    println!("  EP2 Addr: {}", addr2);

    // Spawn accept task for EP2
    println!("  Spawning accept task on EP2...");
    let ep2_clone = ep2.clone();
    let accept_task = tokio::spawn(async move {
        println!("  [EP2] Waiting for incoming connection...");
        let incoming = ep2_clone.accept().await.expect("Accept returned None");
        println!("  [EP2] Got incoming connection!");
        incoming.await
    });

    // Give accept task a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // EP1 connects to EP2
    println!("  EP1 connecting to EP2...");
    let ep2_addr = iroh::EndpointAddr::new(ep2.id()).with_ip_addr(addr2);

    let connect_result = timeout(Duration::from_secs(5), ep1.connect(ep2_addr, ALPN)).await;

    match connect_result {
        Ok(Ok(conn)) => {
            println!("  ✓ EP1 connected successfully!");
            println!("    Remote ID: {:?}", conn.remote_id());
        }
        Ok(Err(e)) => {
            println!("  ✗ EP1 connection failed: {}", e);
        }
        Err(_) => {
            println!("  ✗ EP1 connection timed out after 5s");
        }
    }

    // Check if accept task succeeded
    match timeout(Duration::from_secs(1), accept_task).await {
        Ok(Ok(Ok(_conn))) => {
            println!("  ✓ EP2 accepted connection successfully!");
        }
        Ok(Ok(Err(e))) => {
            println!("  ✗ EP2 accept failed: {}", e);
        }
        Ok(Err(e)) => {
            println!("  ✗ EP2 accept task panicked: {}", e);
        }
        Err(_) => {
            println!("  ⚠ EP2 accept task still waiting");
        }
    }

    // Cleanup
    ep1.close().await;
    ep2.close().await;
}
