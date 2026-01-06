#!/usr/bin/env python3
"""
Python subject for the Rapace compliance suite.

The harness sets PEER_ADDR (e.g. "127.0.0.1:1234"). We connect, immediately
send Hello, then enforce a small subset of the spec needed by the initial
compliance tests.
"""

import os
import socket
import sys

# Import generated code
import sys
from pathlib import Path

# Add generated directory to path
gen_dir = Path(__file__).parent.parent / "generated"
sys.path.insert(0, str(gen_dir))

from echo import EchoHandler, create_echo_dispatcher


class EchoService(EchoHandler):
    def echo(self, message: str) -> str:
        return message

    def reverse(self, message: str) -> str:
        return message[::-1]


def die(message: str) -> None:
    print(message, file=sys.stderr)
    sys.exit(1)


# --- Varint (LEB128) ---

def encode_varint(value: int) -> bytes:
    if value < 0:
        raise ValueError("negative varint")
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        if value != 0:
            byte |= 0x80
        out.append(byte)
        if value == 0:
            break
    return bytes(out)


def decode_varint(buf: bytes, offset: int) -> tuple[int, int]:
    result = 0
    shift = 0
    i = offset
    while True:
        if i >= len(buf):
            raise ValueError("varint: eof")
        byte = buf[i]
        i += 1
        if shift >= 64:
            raise ValueError("varint: overflow")
        result |= (byte & 0x7F) << shift
        if (byte & 0x80) == 0:
            return result, i
        shift += 7


# --- COBS ---

def cobs_encode(data: bytes) -> bytes:
    out = bytearray()
    code_index = 0
    code = 1
    out.append(0)  # placeholder

    for b in data:
        if b == 0:
            out[code_index] = code
            code_index = len(out)
            out.append(0)
            code = 1
        else:
            out.append(b)
            code += 1
            if code == 0xFF:
                out[code_index] = code
                code_index = len(out)
                out.append(0)
                code = 1

    out[code_index] = code
    return bytes(out)


def cobs_decode(data: bytes) -> bytes:
    out = bytearray()
    i = 0
    while i < len(data):
        code = data[i]
        i += 1
        if code == 0:
            raise ValueError("cobs: zero code")
        n = code - 1
        if i + n > len(data):
            raise ValueError("cobs: overrun")
        out.extend(data[i:i + n])
        i += n
        if code != 0xFF and i < len(data):
            out.append(0)
    return bytes(out)


# --- Postcard helpers ---

def encode_string(s: str) -> bytes:
    b = s.encode("utf-8")
    return encode_varint(len(b)) + b


# --- Protocol ---

LOCAL_MAX_PAYLOAD = 1024 * 1024
LOCAL_INITIAL_CREDIT = 64 * 1024


def encode_hello(max_payload: int, initial_credit: int) -> bytes:
    # Message::Hello (0), Hello::V1 (0)
    return (
        encode_varint(0)
        + encode_varint(0)
        + encode_varint(max_payload)
        + encode_varint(initial_credit)
    )


def encode_goodbye(reason: str) -> bytes:
    # Message::Goodbye (1)
    return encode_varint(1) + encode_string(reason)


def frame(payload: bytes) -> bytes:
    return cobs_encode(payload) + b"\x00"


def send_msg(sock: socket.socket, payload: bytes) -> None:
    sock.sendall(frame(payload))


def send_goodbye_and_exit(sock: socket.socket, reason: str) -> None:
    try:
        send_msg(sock, encode_goodbye(reason))
    except Exception:
        pass
    sock.close()
    sys.exit(0)


def handle_message(
    sock: socket.socket,
    payload: bytes,
    state: dict,
) -> None:
    try:
        o = 0
        msg_disc, o = decode_varint(payload, o)

        if msg_disc == 0:
            # Hello
            hello_disc, o = decode_varint(payload, o)
            if hello_disc != 0:
                send_goodbye_and_exit(sock, "message.hello.unknown-version")
            remote_max, o = decode_varint(payload, o)
            _initial_credit, o = decode_varint(payload, o)
            state["negotiated_max_payload"] = min(LOCAL_MAX_PAYLOAD, remote_max)
            state["have_received_hello"] = True
            return

        if not state["have_received_hello"]:
            return

        if msg_disc == 2:
            # Request { request_id, method_id, metadata, payload }
            request_id, o = decode_varint(payload, o)
            method_id, o = decode_varint(payload, o)

            # metadata: Vec<(String, MetadataValue)>
            md_len, o = decode_varint(payload, o)
            for _ in range(md_len):
                k_len, o = decode_varint(payload, o)
                o += k_len
                v_disc, o = decode_varint(payload, o)
                if v_disc == 0:  # String
                    s_len, o = decode_varint(payload, o)
                    o += s_len
                elif v_disc == 1:  # Bytes
                    b_len, o = decode_varint(payload, o)
                    o += b_len
                elif v_disc == 2:  # U64
                    _u, o = decode_varint(payload, o)
                else:
                    raise ValueError("unknown MetadataValue")

            p_len, o = decode_varint(payload, o)
            if p_len > state["negotiated_max_payload"]:
                send_goodbye_and_exit(sock, "flow.unary.payload-limit")

            # Extract request payload
            request_payload = payload[o:]

            # Call dispatcher
            dispatcher = state["dispatcher"]
            response_payload = dispatcher(method_id, request_payload)

            # Send Response message
            resp_msg = encode_varint(3)  # Message::Response
            resp_msg += encode_varint(request_id)
            resp_msg += encode_varint(0)  # metadata length = 0
            resp_msg += encode_varint(len(response_payload))
            resp_msg += response_payload
            send_msg(sock, resp_msg)
            return

        if msg_disc == 3:
            # Response { request_id, metadata, payload }
            _request_id, o = decode_varint(payload, o)

            md_len, o = decode_varint(payload, o)
            for _ in range(md_len):
                k_len, o = decode_varint(payload, o)
                o += k_len
                v_disc, o = decode_varint(payload, o)
                if v_disc == 0:
                    s_len, o = decode_varint(payload, o)
                    o += s_len
                elif v_disc == 1:
                    b_len, o = decode_varint(payload, o)
                    o += b_len
                elif v_disc == 2:
                    _u, o = decode_varint(payload, o)
                else:
                    raise ValueError("unknown MetadataValue")

            p_len, o = decode_varint(payload, o)
            if p_len > state["negotiated_max_payload"]:
                send_goodbye_and_exit(sock, "flow.unary.payload-limit")
            return

        if msg_disc in (6, 7):
            # Close/Reset { stream_id }
            stream_id, o = decode_varint(payload, o)
            if stream_id == 0:
                send_goodbye_and_exit(sock, "streaming.id.zero-reserved")
            return

    except Exception:
        send_goodbye_and_exit(sock, "message.decode-error")


def main() -> None:
    peer_addr = os.environ.get("PEER_ADDR")
    if not peer_addr:
        die("PEER_ADDR is not set")

    last_colon = peer_addr.rfind(":")
    if last_colon < 0:
        die(f"Invalid PEER_ADDR {peer_addr}")

    host = peer_addr[:last_colon]
    port_str = peer_addr[last_colon + 1:]
    try:
        port = int(port_str)
        if not (0 < port <= 65535):
            raise ValueError()
    except ValueError:
        die(f"Invalid port in PEER_ADDR {peer_addr}")

    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        sock.connect((host, port))
    except Exception as e:
        die(f"connect() failed to {host}:{port}: {e}")

    # r[message.hello.timing]: send Hello immediately after connection.
    send_msg(sock, encode_hello(LOCAL_MAX_PAYLOAD, LOCAL_INITIAL_CREDIT))

    # Create dispatcher
    handler = EchoService()
    dispatcher = create_echo_dispatcher(handler)

    state = {
        "negotiated_max_payload": LOCAL_MAX_PAYLOAD,
        "have_received_hello": False,
        "dispatcher": dispatcher,
    }

    buf = b""
    while True:
        try:
            chunk = sock.recv(4096)
        except Exception:
            break
        if not chunk:
            break
        buf += chunk

        while True:
            idx = buf.find(b"\x00")
            if idx < 0:
                break
            frame_bytes = buf[:idx]
            buf = buf[idx + 1:]
            if not frame_bytes:
                continue
            try:
                decoded = cobs_decode(frame_bytes)
            except Exception:
                send_goodbye_and_exit(sock, "message.decode-error")
            handle_message(sock, decoded, state)

    sock.close()


if __name__ == "__main__":
    main()
