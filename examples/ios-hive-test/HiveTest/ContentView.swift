//
//  ContentView.swift
//  HiveTest
//
//  Main view for HIVE BLE mesh demo
//  Mirrors the Android HiveDemo MainActivity layout
//

import SwiftUI

struct ContentView: View {
    @EnvironmentObject var viewModel: HiveViewModel

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                // Status section
                StatusHeaderView()

                Divider()

                // Peer list
                PeerListView()

                Divider()

                // ACK status panel (only visible during active alert)
                if viewModel.ackStatus.isActive {
                    AckStatusView()
                }

                // Action buttons
                ActionButtonsView()
            }
            .navigationTitle("HIVE Demo")
            #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
            #endif
            .onAppear {
                viewModel.startMesh()
            }
            .onDisappear {
                viewModel.shutdown()
            }
        }
        .overlay(alignment: .top) {
            // Toast overlay
            if let toast = viewModel.toastMessage {
                ToastView(message: toast)
                    .transition(.move(edge: .top).combined(with: .opacity))
                    .animation(.easeInOut(duration: 0.3), value: viewModel.toastMessage)
                    .padding(.top, 60)
            }
        }
    }
}

// MARK: - Status Header

struct StatusHeaderView: View {
    @EnvironmentObject var viewModel: HiveViewModel

    var body: some View {
        VStack(spacing: 8) {
            // Mesh ID badge
            Text("Mesh: \(HiveViewModel.MESH_ID)")
                .font(.caption)
                .fontWeight(.bold)
                .padding(.horizontal, 8)
                .padding(.vertical, 2)
                .background(Color.blue.opacity(0.2))
                .cornerRadius(4)
                .padding(.top, 8)

            // Local node ID
            Text("This device: \(viewModel.localDisplayName)")
                .font(.caption)
                .foregroundColor(.secondary)

            // Status message
            Text(viewModel.statusMessage)
                .font(.headline)
                .foregroundColor(viewModel.ackStatus.isActive ? .red : .primary)

            // Connected count
            if !viewModel.peers.isEmpty {
                Text("\(viewModel.connectedCount)/\(viewModel.totalPeerCount) peers connected")
                    .font(.subheadline)
                    .foregroundColor(.secondary)
            }
        }
        .padding(.horizontal)
        .padding(.bottom, 8)
    }
}

// MARK: - Peer List

struct PeerListView: View {
    @EnvironmentObject var viewModel: HiveViewModel

    var body: some View {
        List {
            if viewModel.peers.isEmpty {
                HStack {
                    ProgressView()
                        .padding(.trailing, 8)
                    Text("Scanning for HIVE peers...")
                        .foregroundColor(.secondary)
                }
            } else {
                ForEach(viewModel.peers) { peer in
                    PeerRowView(peer: peer)
                }
            }
        }
        .listStyle(.plain)
    }
}

struct PeerRowView: View {
    let peer: HivePeer

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(peer.displayName)
                    .font(.headline)
                    .foregroundColor(peer.isConnected ? .green : .primary)

                Spacer()

                Text("\(peer.rssi) dBm")
                    .font(.subheadline)
                    .foregroundColor(.secondary)

                // Connection status indicator
                if peer.isConnected {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundColor(.green)
                } else {
                    ProgressView()
                        .scaleEffect(0.8)
                }
            }

            // Show metadata
            VStack(alignment: .leading, spacing: 2) {
                if let advName = peer.advertisedName {
                    Text("Advertised: \(advName)")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }

                HStack {
                    Text("Node: 0x\(String(format: "%08X", peer.nodeId))")
                        .font(.caption2)
                        .foregroundColor(.secondary)

                    if let meshId = peer.meshId {
                        Text("Mesh: \(meshId)")
                            .font(.caption2)
                            .padding(.horizontal, 4)
                            .background(meshId == HiveViewModel.MESH_ID ? Color.green.opacity(0.2) : Color.orange.opacity(0.2))
                            .cornerRadius(2)
                    }
                }

                Text("UUID: \(peer.identifier)")
                    .font(.caption2)
                    .foregroundColor(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)

                HStack {
                    Text(peer.isConnected ? "● Connected" : "○ Connecting...")
                        .font(.caption)
                        .foregroundColor(peer.isConnected ? .green : .orange)

                    Spacer()

                    Text("Last seen: \(peer.lastSeen.formatted(.relative(presentation: .numeric)))")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }
            }
        }
        .padding(.vertical, 6)
    }
}

// MARK: - ACK Status Panel

struct AckStatusView: View {
    @EnvironmentObject var viewModel: HiveViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if !viewModel.ackStatus.ackedNodes.isEmpty {
                HStack {
                    Text("✓ ACK'd:")
                        .foregroundColor(.green)
                    Text(viewModel.ackStatus.ackedNodes.map { String(format: "HIVE-%08X", $0) }.joined(separator: ", "))
                        .font(.caption)
                }
            }

            if !viewModel.ackStatus.waitingNodes.isEmpty {
                HStack {
                    Text("⏳ Waiting:")
                        .foregroundColor(.orange)
                    Text(viewModel.ackStatus.waitingNodes.map { String(format: "HIVE-%08X", $0) }.joined(separator: ", "))
                        .font(.caption)
                }
            }
        }
        .padding()
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.yellow.opacity(0.2))
    }
}

// MARK: - Action Buttons

struct ActionButtonsView: View {
    @EnvironmentObject var viewModel: HiveViewModel

    var body: some View {
        VStack(spacing: 12) {
            // Emergency button
            Button(action: { viewModel.sendEmergency() }) {
                HStack {
                    Image(systemName: "exclamationmark.triangle.fill")
                    Text("EMERGENCY")
                        .fontWeight(.bold)
                }
                .frame(maxWidth: .infinity)
                .padding()
                .background(Color.red)
                .foregroundColor(.white)
                .cornerRadius(10)
            }
            .disabled(!viewModel.isMeshActive)

            HStack(spacing: 12) {
                // ACK button
                Button(action: { viewModel.sendAck() }) {
                    HStack {
                        Image(systemName: "checkmark.circle.fill")
                        Text("ACK")
                            .fontWeight(.bold)
                    }
                    .frame(maxWidth: .infinity)
                    .padding()
                    .background(Color.green)
                    .foregroundColor(.white)
                    .cornerRadius(10)
                }
                .disabled(!viewModel.isMeshActive || !viewModel.ackStatus.isActive)

                // Reset button
                Button(action: { viewModel.resetAlert() }) {
                    HStack {
                        Image(systemName: "xmark.circle.fill")
                        Text("RESET")
                            .fontWeight(.bold)
                    }
                    .frame(maxWidth: .infinity)
                    .padding()
                    .background(Color.gray)
                    .foregroundColor(.white)
                    .cornerRadius(10)
                }
                .disabled(!viewModel.ackStatus.isActive)
            }
        }
        .padding()
    }
}

// MARK: - Toast View

struct ToastView: View {
    let message: String

    var body: some View {
        Text(message)
            .font(.subheadline)
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .background(Color.black.opacity(0.8))
            .foregroundColor(.white)
            .cornerRadius(20)
    }
}

#Preview {
    ContentView()
        .environmentObject(HiveViewModel())
}
