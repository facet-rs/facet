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

	rapace "github.com/bearcove/rapace/go/generated"
)

const (
	localMaxPayload     = uint32(1024 * 1024)
	localInitialCredit  = uint32(64 * 1024)
	goodbyeDecodeError  = "message.decode-error"
	goodbyeHelloUnknown = "message.hello.unknown-version"
	goodbyePayloadLimit = "flow.unary.payload-limit"
	goodbyeStreamIDZero = "streaming.id.zero-reserved"
)

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
	peerAddr := os.Getenv("PEER_ADDR")
	if peerAddr == "" {
		fatal("PEER_ADDR is not set")
	}

	conn, err := net.Dial("tcp", peerAddr)
	if err != nil {
		fatal(fmt.Sprintf("dial %s: %v", peerAddr, err))
	}
	defer conn.Close()

	// Create dispatcher
	handler := &echoService{}
	dispatcher := rapace.NewEchoDispatcher(handler)

	negotiatedMaxPayload := localMaxPayload
	haveReceivedHello := false

	if err := sendHello(conn); err != nil {
		fatal(fmt.Sprintf("send hello: %v", err))
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
					_ = sendGoodbye(conn, goodbyeDecodeError)
					return
				}

				if err := func() error {
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
							_ = sendGoodbye(conn, goodbyeHelloUnknown)
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
						if remoteMax > uint64(^uint32(0)) {
							return errors.New("remote max_payload_size overflow")
						}
						rm := uint32(remoteMax)
						if rm < negotiatedMaxPayload {
							negotiatedMaxPayload = rm
						}
						haveReceivedHello = true
						return nil

					default:
						if !haveReceivedHello {
							return nil
						}
					}

					switch msgDisc {
					case 2: // Request
						requestID, err := readUvarint(payload, &off) // request_id
						if err != nil {
							return err
						}
						methodID, err := readUvarint(payload, &off) // method_id
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
						if pLen > uint64(negotiatedMaxPayload) {
							_ = sendGoodbye(conn, goodbyePayloadLimit)
							return io.EOF
						}

						// Extract request payload
						requestPayload := payload[off:]

						// Call dispatcher
						ctx := context.Background()
						responsePayload, err := dispatcher(ctx, methodID, requestPayload)
						if err != nil {
							return err
						}

						// Send Response message
						var respMsg []byte
						respMsg = appendUvarint(respMsg, 3) // Message::Response
						respMsg = appendUvarint(respMsg, requestID)
						respMsg = appendUvarint(respMsg, 0) // metadata length = 0
						respMsg = appendBytes(respMsg, responsePayload)

						if err := writeFrame(conn, respMsg); err != nil {
							return err
						}
						return nil

					case 3: // Response
						_, err := readUvarint(payload, &off) // request_id
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
						if pLen > uint64(negotiatedMaxPayload) {
							_ = sendGoodbye(conn, goodbyePayloadLimit)
							return io.EOF
						}
						return nil

					case 6, 7: // Close / Reset
						sid, err := readUvarint(payload, &off)
						if err != nil {
							return err
						}
						if sid == 0 {
							_ = sendGoodbye(conn, goodbyeStreamIDZero)
							return io.EOF
						}
						return nil
					default:
						// Ignore.
						return nil
					}
				}(); err != nil {
					if errors.Is(err, io.EOF) {
						return
					}
					_ = sendGoodbye(conn, goodbyeDecodeError)
					return
				}
			}
		}
		if err != nil {
			return
		}
	}
}

func fatal(msg string) {
	fmt.Fprintln(os.Stderr, msg)
	os.Exit(1)
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

func readString(buf []byte, off *int) (string, error) {
	n, err := readUvarint(buf, off)
	if err != nil {
		return "", err
	}
	if n > uint64(len(buf)-*off) {
		return "", errors.New("string: length out of range")
	}
	s := string(buf[*off : *off+int(n)])
	*off += int(n)
	return s, nil
}

func encodeString(s string) []byte {
	var out []byte
	return appendString(out, s)
}

// Spec: `[impl unary.response.encoding]`
func encodeResultOk(value string, encoder func(string) []byte) []byte {
	var out []byte
	out = appendUvarint(out, 0) // Result::Ok
	out = append(out, encoder(value)...)
	return out
}

// Spec: `[impl unary.response.encoding]`
func encodeResultErr(err error) []byte {
	var out []byte
	out = appendUvarint(out, 1) // Result::Err
	out = appendUvarint(out, 0) // RapaceError::User
	out = appendString(out, err.Error())
	return out
}

// Spec: `[impl unary.response.encoding]`
func encodeUnknownMethodError() []byte {
	var out []byte
	out = appendUvarint(out, 1) // Result::Err
	out = appendUvarint(out, 1) // RapaceError::UnknownMethod
	return out
}

// Spec: `[impl unary.response.encoding]`
func encodeInvalidPayloadError() []byte {
	var out []byte
	out = appendUvarint(out, 1) // Result::Err
	out = appendUvarint(out, 2) // RapaceError::InvalidPayload
	return out
}

func skipBytes(buf []byte, off *int) error {
	n, err := readUvarint(buf, off)
	if err != nil {
		return err
	}
	if n > uint64(len(buf)-*off) {
		return errors.New("bytes: length out of range")
	}
	*off += int(n)
	return nil
}

func skipString(buf []byte, off *int) error {
	return skipBytes(buf, off)
}

func skipMetadata(buf []byte, off *int) error {
	// metadata: Vec<(String, MetadataValue)>
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
