// Go client for cross-language testing.
//
// Connects to a TCP server, performs Hello exchange, and makes RPC calls.
// Used to test Go client against servers implemented in other languages.

package main

import (
	"bytes"
	"context"
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"sync"

	roam "github.com/bearcove/roam/go/generated"
)

const (
	localMaxPayload    = uint32(1024 * 1024)
	localInitialCredit = uint32(64 * 1024)
)

// TcpConnection implements roam.Connection for TCP transport
type TcpConnection struct {
	conn      net.Conn
	buf       []byte
	mu        sync.Mutex
	requestID uint64
	maxPayload uint32
}

func NewTcpConnection(addr string) (*TcpConnection, error) {
	conn, err := net.Dial("tcp", addr)
	if err != nil {
		return nil, fmt.Errorf("dial %s: %w", addr, err)
	}

	tc := &TcpConnection{
		conn:       conn,
		requestID:  1,
		maxPayload: localMaxPayload,
	}

	// Perform Hello exchange
	if err := tc.doHello(); err != nil {
		conn.Close()
		return nil, fmt.Errorf("hello exchange: %w", err)
	}

	return tc, nil
}

func (tc *TcpConnection) Close() error {
	return tc.conn.Close()
}

func (tc *TcpConnection) doHello() error {
	// Send our Hello
	var payload []byte
	payload = appendUvarint(payload, 0) // Message::Hello
	payload = appendUvarint(payload, 0) // Hello::V1
	payload = appendUvarint(payload, uint64(localMaxPayload))
	payload = appendUvarint(payload, uint64(localInitialCredit))
	if err := tc.writeFrame(payload); err != nil {
		return fmt.Errorf("send hello: %w", err)
	}

	// Receive Hello from server
	msg, err := tc.readFrame()
	if err != nil {
		return fmt.Errorf("read hello: %w", err)
	}

	off := 0
	msgDisc, err := readUvarint(msg, &off)
	if err != nil {
		return err
	}
	if msgDisc != 0 {
		return errors.New("expected Hello message")
	}

	helloDisc, err := readUvarint(msg, &off)
	if err != nil {
		return err
	}
	if helloDisc != 0 {
		return errors.New("unsupported Hello version")
	}

	remoteMax, err := readUvarint(msg, &off)
	if err != nil {
		return err
	}
	if uint32(remoteMax) < tc.maxPayload {
		tc.maxPayload = uint32(remoteMax)
	}

	return nil
}

func (tc *TcpConnection) Call(ctx context.Context, methodID uint64, payload []byte) ([]byte, error) {
	tc.mu.Lock()
	defer tc.mu.Unlock()

	reqID := tc.requestID
	tc.requestID++

	// Build Request message
	var msg []byte
	msg = appendUvarint(msg, 2) // Message::Request
	msg = appendUvarint(msg, reqID)
	msg = appendUvarint(msg, methodID)
	msg = appendUvarint(msg, 0) // metadata length = 0
	msg = appendBytes(msg, payload)

	if err := tc.writeFrame(msg); err != nil {
		return nil, fmt.Errorf("send request: %w", err)
	}

	// Read Response
	for {
		resp, err := tc.readFrame()
		if err != nil {
			return nil, fmt.Errorf("read response: %w", err)
		}

		off := 0
		msgDisc, err := readUvarint(resp, &off)
		if err != nil {
			return nil, err
		}

		if msgDisc == 3 { // Response
			respID, err := readUvarint(resp, &off)
			if err != nil {
				return nil, err
			}
			if respID != reqID {
				continue // Not our response, keep reading
			}

			// Skip metadata
			if err := skipMetadata(resp, &off); err != nil {
				return nil, err
			}

			// Read payload
			pLen, err := readUvarint(resp, &off)
			if err != nil {
				return nil, err
			}
			_ = pLen

			return resp[off:], nil
		}
		// Ignore other message types for now
	}
}

func (tc *TcpConnection) writeFrame(payload []byte) error {
	enc := cobsEncode(payload)
	enc = append(enc, 0x00)
	_, err := tc.conn.Write(enc)
	return err
}

func (tc *TcpConnection) readFrame() ([]byte, error) {
	for {
		if i := bytes.IndexByte(tc.buf, 0x00); i >= 0 {
			frame := tc.buf[:i]
			tc.buf = tc.buf[i+1:]
			if len(frame) == 0 {
				continue
			}
			return cobsDecode(frame)
		}

		tmp := make([]byte, 4096)
		n, err := tc.conn.Read(tmp)
		if n > 0 {
			tc.buf = append(tc.buf, tmp[:n]...)
		}
		if err != nil {
			if errors.Is(err, io.EOF) && len(tc.buf) > 0 {
				continue
			}
			return nil, err
		}
	}
}

// Helper functions

func appendUvarint(dst []byte, v uint64) []byte {
	var tmp [10]byte
	n := binary.PutUvarint(tmp[:], v)
	return append(dst, tmp[:n]...)
}

func appendBytes(dst []byte, b []byte) []byte {
	dst = appendUvarint(dst, uint64(len(b)))
	return append(dst, b...)
}

func readUvarint(buf []byte, off *int) (uint64, error) {
	v, n := binary.Uvarint(buf[*off:])
	if n <= 0 {
		return 0, errors.New("varint decode error")
	}
	*off += n
	return v, nil
}

func skipString(buf []byte, off *int) error {
	n, err := readUvarint(buf, off)
	if err != nil {
		return err
	}
	if n > uint64(len(buf)-*off) {
		return errors.New("string: length out of range")
	}
	*off += int(n)
	return nil
}

func skipBytes(buf []byte, off *int) error {
	return skipString(buf, off)
}

func skipMetadata(buf []byte, off *int) error {
	mdLen, err := readUvarint(buf, off)
	if err != nil {
		return err
	}
	for i := uint64(0); i < mdLen; i++ {
		if err := skipString(buf, off); err != nil {
			return err
		}
		vDisc, err := readUvarint(buf, off)
		if err != nil {
			return err
		}
		switch vDisc {
		case 0: // String
			if err := skipString(buf, off); err != nil {
				return err
			}
		case 1: // Bytes
			if err := skipBytes(buf, off); err != nil {
				return err
			}
		case 2: // U64
			_, err := readUvarint(buf, off)
			if err != nil {
				return err
			}
		default:
			return errors.New("unknown metadata value")
		}
	}
	return nil
}

func cobsEncode(input []byte) []byte {
	out := make([]byte, 0, len(input)+2)
	codeIndex := 0
	code := byte(1)
	out = append(out, 0) // placeholder

	for _, b := range input {
		if b == 0 {
			out[codeIndex] = code
			codeIndex = len(out)
			out = append(out, 0)
			code = 1
			continue
		}
		out = append(out, b)
		code++
		if code == 0xFF {
			out[codeIndex] = code
			codeIndex = len(out)
			out = append(out, 0)
			code = 1
		}
	}
	out[codeIndex] = code
	return out
}

func cobsDecode(input []byte) ([]byte, error) {
	out := make([]byte, 0, len(input))
	for i := 0; i < len(input); {
		code := input[i]
		i++
		if code == 0 {
			return nil, errors.New("cobs: zero code")
		}
		n := int(code) - 1
		if i+n > len(input) {
			return nil, errors.New("cobs: overrun")
		}
		out = append(out, input[i:i+n]...)
		i += n
		if code != 0xFF && i < len(input) {
			out = append(out, 0)
		}
	}
	return out, nil
}

func main() {
	serverAddr := os.Getenv("SERVER_ADDR")
	if serverAddr == "" {
		serverAddr = "127.0.0.1:9001"
	}

	fmt.Fprintf(os.Stderr, "Connecting to %s...\n", serverAddr)

	conn, err := NewTcpConnection(serverAddr)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to connect: %v\n", err)
		os.Exit(1)
	}
	defer conn.Close()

	fmt.Fprintf(os.Stderr, "Connected! Running tests...\n")

	client := roam.NewEchoClient(conn)
	ctx := context.Background()

	// Test Echo
	result, err := client.Echo(ctx, "Hello, World!")
	if err != nil {
		fmt.Fprintf(os.Stderr, "Echo failed: %v\n", err)
		os.Exit(1)
	}
	if result != "Hello, World!" {
		fmt.Fprintf(os.Stderr, "Echo mismatch: got %q, want %q\n", result, "Hello, World!")
		os.Exit(1)
	}
	fmt.Fprintf(os.Stderr, "Echo: PASS\n")

	// Test Reverse
	result, err = client.Reverse(ctx, "Hello")
	if err != nil {
		fmt.Fprintf(os.Stderr, "Reverse failed: %v\n", err)
		os.Exit(1)
	}
	if result != "olleH" {
		fmt.Fprintf(os.Stderr, "Reverse mismatch: got %q, want %q\n", result, "olleH")
		os.Exit(1)
	}
	fmt.Fprintf(os.Stderr, "Reverse: PASS\n")

	// Test with unicode
	result, err = client.Echo(ctx, "Hello, World! 🎉")
	if err != nil {
		fmt.Fprintf(os.Stderr, "Echo unicode failed: %v\n", err)
		os.Exit(1)
	}
	if result != "Hello, World! 🎉" {
		fmt.Fprintf(os.Stderr, "Echo unicode mismatch: got %q, want %q\n", result, "Hello, World! 🎉")
		os.Exit(1)
	}
	fmt.Fprintf(os.Stderr, "Echo unicode: PASS\n")

	// Test Reverse with unicode
	result, err = client.Reverse(ctx, "日本語")
	if err != nil {
		fmt.Fprintf(os.Stderr, "Reverse unicode failed: %v\n", err)
		os.Exit(1)
	}
	if result != "語本日" {
		fmt.Fprintf(os.Stderr, "Reverse unicode mismatch: got %q, want %q\n", result, "語本日")
		os.Exit(1)
	}
	fmt.Fprintf(os.Stderr, "Reverse unicode: PASS\n")

	fmt.Println("All tests passed!")
}
