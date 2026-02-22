#!/usr/bin/env python3
"""
HIVE-Lite OTA Sender — Test tool for pushing firmware to an ESP32 device
over the HIVE-Lite UDP protocol.

Usage:
    python3 scripts/ota-sender.py <firmware.bin> [--target IP] [--port 5555]

If --target is not specified, broadcasts the offer and discovers the device
from its OtaAccept response.
"""

import argparse
import hashlib
import socket
import struct
import sys
import time

# ---------------------------------------------------------------------------
# Protocol constants (must match hive-lite-protocol)
# ---------------------------------------------------------------------------
MAGIC = b"HIVE"
PROTOCOL_VERSION = 1
HEADER_SIZE = 16
MAX_PACKET_SIZE = 512
DEFAULT_PORT = 5555

# Message types
MSG_HEARTBEAT = 0x02
MSG_OTA_OFFER = 0x10
MSG_OTA_ACCEPT = 0x11
MSG_OTA_DATA = 0x12
MSG_OTA_ACK = 0x13
MSG_OTA_COMPLETE = 0x14
MSG_OTA_RESULT = 0x15
MSG_OTA_ABORT = 0x16

# OTA chunk data size (payload budget: 496 - 6 bytes framing)
OTA_CHUNK_DATA_SIZE = 448

# Result codes
RESULT_SUCCESS = 0x00
RESULT_HASH_MISMATCH = 0x01
RESULT_FLASH_ERROR = 0x02
RESULT_INVALID_OFFER = 0x03
RESULT_SIG_INVALID = 0x04
RESULT_SIG_REQUIRED = 0x05

RESULT_NAMES = {
    0x00: "SUCCESS",
    0x01: "HASH_MISMATCH",
    0x02: "FLASH_ERROR",
    0x03: "INVALID_OFFER",
    0x04: "SIGNATURE_INVALID",
    0x05: "SIGNATURE_REQUIRED",
}

# Sender node ID
SENDER_NODE_ID = 0x4F544153  # "OTAS" in ASCII


def encode_header(msg_type: int, node_id: int = SENDER_NODE_ID,
                  seq_num: int = 0, flags: int = 0) -> bytes:
    """Encode a 16-byte HIVE-Lite header."""
    return struct.pack("<4sBBHII",
                       MAGIC, PROTOCOL_VERSION, msg_type, flags,
                       node_id, seq_num)


def decode_header(data: bytes):
    """Decode header, return (msg_type, flags, node_id, seq_num, payload)."""
    if len(data) < HEADER_SIZE:
        return None
    magic, ver, msg_type, flags, node_id, seq_num = struct.unpack(
        "<4sBBHII", data[:HEADER_SIZE])
    if magic != MAGIC or ver != PROTOCOL_VERSION:
        return None
    return msg_type, flags, node_id, seq_num, data[HEADER_SIZE:]


def build_ota_offer(version_str: str, firmware_size: int, total_chunks: int,
                    chunk_size: int, sha256: bytes, session_id: int) -> bytes:
    """Build OtaOffer payload (76 bytes, unsigned)."""
    ver = version_str.encode("utf-8")[:16].ljust(16, b"\x00")
    payload = ver
    payload += struct.pack("<I", firmware_size)
    payload += struct.pack("<H", total_chunks)
    payload += struct.pack("<H", chunk_size)
    payload += sha256  # 32 bytes
    payload += struct.pack("<H", session_id)
    payload += struct.pack("<H", 0)  # flags = 0 (unsigned)
    # Pad to 76 bytes (16 reserved bytes at end of offer)
    payload += b"\x00" * (76 - len(payload))
    return payload


def build_ota_data(session_id: int, chunk_num: int, data: bytes) -> bytes:
    """Build OtaData payload: session_id(2) + chunk_num(2) + len(2) + data."""
    return struct.pack("<HHH", session_id, chunk_num, len(data)) + data


def build_ota_complete(session_id: int) -> bytes:
    """Build OtaComplete payload."""
    return struct.pack("<H", session_id)


def discover_device(sock, port, timeout=10):
    """Listen for heartbeats to discover device IP."""
    print(f"Listening for HIVE-Lite heartbeats on port {port}...")
    sock.settimeout(timeout)
    start = time.time()
    while time.time() - start < timeout:
        try:
            data, addr = sock.recvfrom(MAX_PACKET_SIZE)
            result = decode_header(data)
            if result and result[0] == MSG_HEARTBEAT:
                print(f"  Found device: {addr[0]} (node_id=0x{result[2]:08X})")
                return addr[0]
        except socket.timeout:
            pass
    return None


def send_ota(firmware_path: str, target_ip: str = None, port: int = DEFAULT_PORT,
             version: str = "0.1.0", retries: int = 5, timeout: float = 3.0):
    """Send firmware to a HIVE-Lite device via OTA."""

    # Read firmware
    with open(firmware_path, "rb") as f:
        firmware = f.read()

    fw_size = len(firmware)
    sha256 = hashlib.sha256(firmware).digest()
    total_chunks = (fw_size + OTA_CHUNK_DATA_SIZE - 1) // OTA_CHUNK_DATA_SIZE
    session_id = int(time.time()) & 0xFFFF

    print(f"Firmware: {firmware_path}")
    print(f"  Size:     {fw_size:,} bytes")
    print(f"  SHA256:   {sha256.hex()[:16]}...")
    print(f"  Chunks:   {total_chunks} x {OTA_CHUNK_DATA_SIZE} bytes")
    print(f"  Session:  {session_id}")
    print(f"  Version:  {version}")
    print()

    # Create UDP socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(("0.0.0.0", port))
    sock.settimeout(timeout)

    # Discover device if no target specified
    if not target_ip:
        target_ip = discover_device(sock, port, timeout=15)
        if not target_ip:
            print("ERROR: No device found. Specify --target IP.")
            sock.close()
            return False

    target = (target_ip, port)
    device_node_id = None

    # --- Phase 1: Send OtaOffer ---
    print(f"[1/3] Sending OtaOffer to {target_ip}:{port}...")
    offer_payload = build_ota_offer(version, fw_size, total_chunks,
                                     OTA_CHUNK_DATA_SIZE, sha256, session_id)
    offer_pkt = encode_header(MSG_OTA_OFFER) + offer_payload

    accepted = False
    for attempt in range(retries):
        sock.sendto(offer_pkt, target)
        try:
            while True:
                data, addr = sock.recvfrom(MAX_PACKET_SIZE)
                result = decode_header(data)
                if not result:
                    continue
                msg_type, flags, node_id, seq_num, payload = result

                if msg_type == MSG_OTA_ACCEPT:
                    resp_session = struct.unpack("<H", payload[:2])[0]
                    resume_chunk = struct.unpack("<H", payload[2:4])[0] if len(payload) >= 4 else 0
                    if resp_session == session_id:
                        device_node_id = node_id
                        print(f"  ACCEPTED by node 0x{node_id:08X} (resume_chunk={resume_chunk})")
                        accepted = True
                        break
                elif msg_type == MSG_OTA_RESULT:
                    # Immediate rejection
                    resp_session = struct.unpack("<H", payload[:2])[0]
                    result_code = payload[2] if len(payload) >= 3 else 0xFF
                    print(f"  REJECTED: {RESULT_NAMES.get(result_code, f'0x{result_code:02X}')}")
                    sock.close()
                    return False
                # Ignore heartbeats and other traffic
        except socket.timeout:
            print(f"  Timeout (attempt {attempt + 1}/{retries})")

        if accepted:
            break

    if not accepted:
        print("ERROR: No OtaAccept received.")
        sock.close()
        return False

    # --- Phase 2: Send OtaData chunks ---
    print(f"[2/3] Sending {total_chunks} firmware chunks...")
    start_time = time.time()
    last_progress = -1

    for chunk_num in range(total_chunks):
        offset = chunk_num * OTA_CHUNK_DATA_SIZE
        chunk_data = firmware[offset:offset + OTA_CHUNK_DATA_SIZE]
        data_payload = build_ota_data(session_id, chunk_num, chunk_data)
        data_pkt = encode_header(MSG_OTA_DATA) + data_payload

        acked = False
        for attempt in range(retries):
            sock.sendto(data_pkt, target)
            try:
                while True:
                    resp_data, addr = sock.recvfrom(MAX_PACKET_SIZE)
                    result = decode_header(resp_data)
                    if not result:
                        continue
                    msg_type, flags, node_id, seq_num, payload = result

                    if msg_type == MSG_OTA_ACK and len(payload) >= 4:
                        ack_session = struct.unpack("<H", payload[:2])[0]
                        ack_chunk = struct.unpack("<H", payload[2:4])[0]
                        if ack_session == session_id and ack_chunk == chunk_num:
                            acked = True
                            break
                    elif msg_type == MSG_OTA_ABORT:
                        reason = payload[2] if len(payload) >= 3 else 0
                        print(f"\n  ABORTED by device (reason={reason})")
                        sock.close()
                        return False
            except socket.timeout:
                if attempt < retries - 1:
                    pass  # retry silently

            if acked:
                break

        if not acked:
            print(f"\n  ERROR: Chunk {chunk_num} not acknowledged after {retries} attempts")
            # Send abort
            abort_pkt = encode_header(MSG_OTA_ABORT) + struct.pack("<HBB", session_id, 0x04, 0)
            sock.sendto(abort_pkt, target)
            sock.close()
            return False

        # Progress display
        progress = ((chunk_num + 1) * 100) // total_chunks
        if progress != last_progress:
            elapsed = time.time() - start_time
            bytes_sent = offset + len(chunk_data)
            rate = bytes_sent / elapsed if elapsed > 0 else 0
            bar_len = 40
            filled = (progress * bar_len) // 100
            bar = "#" * filled + "-" * (bar_len - filled)
            eta = (fw_size - bytes_sent) / rate if rate > 0 else 0
            sys.stdout.write(f"\r  [{bar}] {progress}% "
                           f"({bytes_sent:,}/{fw_size:,}) "
                           f"{rate / 1024:.1f} KB/s  ETA {eta:.0f}s  ")
            sys.stdout.flush()
            last_progress = progress

    elapsed = time.time() - start_time
    print(f"\n  Transfer complete: {fw_size:,} bytes in {elapsed:.1f}s "
          f"({fw_size / elapsed / 1024:.1f} KB/s)")

    # --- Phase 3: Send OtaComplete ---
    print("[3/3] Sending OtaComplete...")
    complete_pkt = encode_header(MSG_OTA_COMPLETE) + build_ota_complete(session_id)

    final_result = None
    for attempt in range(retries):
        sock.sendto(complete_pkt, target)
        try:
            while True:
                data, addr = sock.recvfrom(MAX_PACKET_SIZE)
                result = decode_header(data)
                if not result:
                    continue
                msg_type, flags, node_id, seq_num, payload = result

                if msg_type == MSG_OTA_RESULT and len(payload) >= 3:
                    resp_session = struct.unpack("<H", payload[:2])[0]
                    result_code = payload[2]
                    if resp_session == session_id:
                        final_result = result_code
                        break
        except socket.timeout:
            print(f"  Timeout (attempt {attempt + 1}/{retries})")

        if final_result is not None:
            break

    sock.close()

    if final_result is None:
        print("  WARNING: No OtaResult received (device may have rebooted)")
        return True  # Optimistic — device may have rebooted before responding
    elif final_result == RESULT_SUCCESS:
        print(f"  SUCCESS! Device will reboot into new firmware.")
        return True
    else:
        name = RESULT_NAMES.get(final_result, f"0x{final_result:02X}")
        print(f"  FAILED: {name}")
        return False


def main():
    parser = argparse.ArgumentParser(
        description="HIVE-Lite OTA Sender — push firmware to ESP32 over UDP")
    parser.add_argument("firmware", help="Path to firmware binary (.bin)")
    parser.add_argument("--target", "-t", help="Device IP (auto-discover if omitted)")
    parser.add_argument("--port", "-p", type=int, default=DEFAULT_PORT,
                        help=f"UDP port (default: {DEFAULT_PORT})")
    parser.add_argument("--version", "-v", default="0.1.0",
                        help="Firmware version string (max 16 chars)")
    parser.add_argument("--retries", "-r", type=int, default=5,
                        help="Max retries per chunk (default: 5)")
    parser.add_argument("--timeout", type=float, default=3.0,
                        help="ACK timeout in seconds (default: 3.0)")
    args = parser.parse_args()

    ok = send_ota(args.firmware, target_ip=args.target, port=args.port,
                  version=args.version, retries=args.retries,
                  timeout=args.timeout)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
