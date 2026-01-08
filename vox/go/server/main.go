// Go TCP server for cross-language testing.
//
// Listens on a TCP port and handles Echo service requests.
// Used to test clients in other languages against a Go server.

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

	roam "github.com/bearcove/roam/go/generated"
)

const (
	localMaxPayload    = uint32(1024 * 1024)
	localInitialCredit = uint32(64 * 1024)
)

// Echo handler implementation
type echoService struct{}

func (e *echoService) Echo(ctx context.Context, message string) (string, error) {
	return message, nil
}

func (e *echoService) Reverse(ctx context.Context, message string) (string, error) {
	runes := []rune(message)
	for i, j := 0, len(runes)-1; i < j; i, j = i+1, j-1 {
		runes[i], runes[j] = runes[j], runes[i]
	}
	return string(runes), nil
}

func main() {
	port := os.Getenv("TCP_PORT")
	if port == "" {
		port = "9010"
	}

	addr := "127.0.0.1:" + port
	listener, err := net.Listen("tcp", addr)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Listen failed: %v\n", err)
		os.Exit(1)
	}
	defer listener.Close()

	fmt.Fprintf(os.Stderr, "Go TCP server listening on %s\n", addr)
	fmt.Println(port) // For test harness

	for {
		conn, err := listener.Accept()
		if err != nil {
			fmt.Fprintf(os.Stderr, "Accept error: %v\n", err)
			continue
		}

		go handleConnection(conn)
	}
}

func handleConnection(conn net.Conn) {
	defer conn.Close()
	peer := conn.RemoteAddr().String()
	fmt.Fprintf(os.Stderr, "New connection from %s\n", peer)

	handler := &echoService{}
	dispatcher := roam.NewEchoDispatcher(handler)

	negotiatedMaxPayload := localMaxPayload
	haveReceivedHello := false

	// Send Hello
	if err := sendHello(conn); err != nil {
		fmt.Fprintf(os.Stderr, "Send hello error: %v\n", err)
		return
	}

	var buf []byte
	tmp := make([]byte, 4096)

	for {
		n, err := conn.Read(tmp)
		if n > 0 {
			buf = append(buf, tmp[:n]...)
			for {
				i := bytes.IndexByte(buf, 0x00)
				if i < 0 {
					break
				}
				frame := buf[:i]
				buf = buf[i+1:]
				if len(frame) == 0 {
					continue
				}

				payload, err := cobsDecode(frame)
				if err != nil {
					_ = sendGoodbye(conn, "message.decode-error")
					return
				}

				if err := processMessage(conn, payload, dispatcher, &negotiatedMaxPayload, &haveReceivedHello); err != nil {
					if errors.Is(err, io.EOF) {
						fmt.Fprintf(os.Stderr, "Connection closed: %s\n", peer)
						return
					}
					_ = sendGoodbye(conn, "message.decode-error")
					return
				}
			}
		}
		if err != nil {
			fmt.Fprintf(os.Stderr, "Connection closed: %s\n", peer)
			return
		}
	}
}

func processMessage(conn net.Conn, payload []byte, dispatcher func(context.Context, uint64, []byte) ([]byte, error), negotiatedMaxPayload *uint32, haveReceivedHello *bool) error {
	off := 0
	msgDisc, err := readUvarint(payload, &off)
	if err != nil {
		return err
	}

	switch msgDisc {
	case 0: // Hello
		helloDisc, err := readUvarint(payload, &off)
		if err != nil {
			return err
		}
		if helloDisc != 0 {
			_ = sendGoodbye(conn, "message.hello.unknown-version")
			return io.EOF
		}
		remoteMax, err := readUvarint(payload, &off)
		if err != nil {
			return err
		}
		_, err = readUvarint(payload, &off) // initial_stream_credit
		if err != nil {
			return err
		}
		rm := uint32(remoteMax)
		if rm < *negotiatedMaxPayload {
			*negotiatedMaxPayload = rm
		}
		*haveReceivedHello = true
		return nil

	case 1: // Goodbye
		return io.EOF

	case 2: // Request
		if !*haveReceivedHello {
			return nil
		}

		requestID, err := readUvarint(payload, &off)
		if err != nil {
			return err
		}
		methodID, err := readUvarint(payload, &off)
		if err != nil {
			return err
		}
		if err := skipMetadata(payload, &off); err != nil {
			return err
		}
		pLen, err := readUvarint(payload, &off)
		if err != nil {
			return err
		}
		if pLen > uint64(*negotiatedMaxPayload) {
			_ = sendGoodbye(conn, "flow.unary.payload-limit")
			return io.EOF
		}

		requestPayload := payload[off:]

		ctx := context.Background()
		responsePayload, err := dispatcher(ctx, methodID, requestPayload)
		if err != nil {
			return err
		}

		// Send Response
		var respMsg []byte
		respMsg = appendUvarint(respMsg, 3) // Message::Response
		respMsg = appendUvarint(respMsg, requestID)
		respMsg = appendUvarint(respMsg, 0) // metadata length = 0
		respMsg = appendBytes(respMsg, responsePayload)

		return writeFrame(conn, respMsg)

	case 5, 6, 7, 8: // Data, Close, Reset, Credit
		if !*haveReceivedHello {
			return nil
		}
		sid, err := readUvarint(payload, &off)
		if err != nil {
			return err
		}
		if sid == 0 {
			_ = sendGoodbye(conn, "streaming.id.zero-reserved")
			return io.EOF
		}
		_ = sendGoodbye(conn, "streaming.unknown")
		return io.EOF

	default:
		return nil
	}
}

func sendHello(w io.Writer) error {
	var payload []byte
	payload = appendUvarint(payload, 0) // Message::Hello
	payload = appendUvarint(payload, 0) // Hello::V1
	payload = appendUvarint(payload, uint64(localMaxPayload))
	payload = appendUvarint(payload, uint64(localInitialCredit))
	return writeFrame(w, payload)
}

func sendGoodbye(w io.Writer, reason string) error {
	var payload []byte
	payload = appendUvarint(payload, 1) // Message::Goodbye
	payload = appendString(payload, reason)
	_ = writeFrame(w, payload)
	return nil
}

func writeFrame(w io.Writer, payload []byte) error {
	enc := cobsEncode(payload)
	enc = append(enc, 0x00)
	_, err := w.Write(enc)
	return err
}

func appendUvarint(dst []byte, v uint64) []byte {
	var tmp [10]byte
	n := binary.PutUvarint(tmp[:], v)
	return append(dst, tmp[:n]...)
}

func readUvarint(buf []byte, off *int) (uint64, error) {
	v, n := binary.Uvarint(buf[*off:])
	if n <= 0 {
		return 0, errors.New("varint decode error")
	}
	*off += n
	return v, nil
}

func appendString(dst []byte, s string) []byte {
	b := []byte(s)
	dst = appendUvarint(dst, uint64(len(b)))
	return append(dst, b...)
}

func appendBytes(dst []byte, b []byte) []byte {
	dst = appendUvarint(dst, uint64(len(b)))
	return append(dst, b...)
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
		case 0:
			if err := skipString(buf, off); err != nil {
				return err
			}
		case 1:
			if err := skipBytes(buf, off); err != nil {
				return err
			}
		case 2:
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
	out = append(out, 0)

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
