import SwiftUI

struct MeshStatusView: View {
    @Bindable var viewModel: MeshViewModel

    var body: some View {
        NavigationStack {
            List {
                nodeInfoSection
                transportSection
                peersSection
                syncLogSection
            }
            .navigationTitle("Mesh Status")
            .task {
                await viewModel.startMonitoring()
            }
            .onDisappear {
                viewModel.stopMonitoring()
            }
            .refreshable {
                await viewModel.refresh()
            }
        }
    }

    private var nodeInfoSection: some View {
        Section {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text(viewModel.nodeName.isEmpty ? "Unknown" : viewModel.nodeName)
                        .font(.headline)
                    Text("Node: \(viewModel.nodeId.prefix(8))")
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                }
                Spacer()
                SyncStatusBadge(status: viewModel.syncStatus)
            }

            if let cell = viewModel.cellInfo {
                HStack {
                    Label("Cell", systemImage: "circle.hexagongrid.fill")
                        .foregroundStyle(.secondary)
                    Spacer()
                    VStack(alignment: .trailing) {
                        Text(cell.cellName)
                            .font(.subheadline.bold())
                        Text("\(cell.memberCount) members \u{2022} \(cell.role.rawValue)")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }

            HStack {
                Label("Last Sync", systemImage: "clock")
                    .foregroundStyle(.secondary)
                Spacer()
                Text(viewModel.lastSyncTimeFormatted)
                    .font(.subheadline)
            }
        } header: {
            Text("This Node")
        }
    }

    private var transportSection: some View {
        Section {
            if viewModel.activeTransports.isEmpty {
                HStack {
                    Image(systemName: "wifi.slash")
                        .foregroundStyle(MilitaryTheme.alertRed)
                    Text("No active transports")
                        .foregroundStyle(.secondary)
                }
            } else {
                ForEach(TransportType.allCases, id: \.self) { transport in
                    let isActive = viewModel.activeTransports.contains(transport)
                    HStack {
                        Image(systemName: transport.sfSymbol)
                            .foregroundStyle(isActive ? MilitaryTheme.statusGreen : .secondary)
                            .frame(width: 24)
                        Text(transport.rawValue)
                        Spacer()
                        Text(isActive ? "Active" : "Inactive")
                            .font(.caption)
                            .foregroundStyle(isActive ? MilitaryTheme.statusGreen : .secondary)
                        Circle()
                            .fill(isActive ? MilitaryTheme.statusGreen : Color(.systemGray4))
                            .frame(width: 8, height: 8)
                    }
                }
            }
        } header: {
            Text("Transports")
        }
    }

    private var peersSection: some View {
        Section {
            if viewModel.peers.isEmpty {
                HStack {
                    Image(systemName: "person.slash")
                        .foregroundStyle(.secondary)
                    Text("No peers discovered")
                        .foregroundStyle(.secondary)
                }
            } else {
                ForEach(viewModel.peers) { peer in
                    PeerRowView(peer: peer)
                }
            }
        } header: {
            Text("Peers (\(viewModel.peers.count))")
        }
    }

    private var syncLogSection: some View {
        Section {
            if viewModel.syncEvents.isEmpty {
                Text("No sync events yet")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(viewModel.syncEvents.prefix(20)) { event in
                    SyncEventRow(event: event)
                }
            }
        } header: {
            Text("Sync Log")
        }
    }
}

struct PeerRowView: View {
    let peer: PeerInfo

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "person.circle.fill")
                .font(.title2)
                .foregroundStyle(peer.isCellMember ? MilitaryTheme.odGreen : .secondary)

            VStack(alignment: .leading, spacing: 2) {
                Text(peer.name)
                    .font(.subheadline.bold())
                Text("ID: \(peer.nodeId.prefix(8))")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                Text("Last seen: \(peer.lastSeen.formatted(date: .omitted, time: .shortened))")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            VStack(alignment: .trailing, spacing: 4) {
                TransportBadge(transport: peer.transportType)
                SignalBars(strength: peer.signalStrength)
            }
        }
        .padding(.vertical, 2)
    }
}

struct SyncEventRow: View {
    let event: SyncEvent

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: event.eventType.sfSymbol)
                .font(.caption)
                .foregroundStyle(MilitaryTheme.odGreen)
                .frame(width: 20)

            VStack(alignment: .leading, spacing: 2) {
                HStack {
                    Text(event.peerName)
                        .font(.caption.bold())
                    Text(event.eventType.rawValue)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Text(event.detail)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Text(event.timestamp.formatted(date: .omitted, time: .shortened))
                .font(.caption2.monospaced())
                .foregroundStyle(.secondary)
        }
    }
}
