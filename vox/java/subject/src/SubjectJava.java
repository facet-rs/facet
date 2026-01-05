import java.io.ByteArrayOutputStream;
import java.io.EOFException;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;

public final class SubjectJava {
    private static final int LOCAL_MAX_PAYLOAD = 1024 * 1024;
    private static final int LOCAL_INITIAL_CREDIT = 64 * 1024;

    private static final String GOODBYE_DECODE_ERROR = "message.decode-error";
    private static final String GOODBYE_HELLO_UNKNOWN = "message.hello.unknown-version";
    private static final String GOODBYE_PAYLOAD_LIMIT = "flow.unary.payload-limit";
    private static final String GOODBYE_STREAM_ID_ZERO = "streaming.id.zero-reserved";

    public static void main(String[] args) throws Exception {
        String peerAddr = System.getenv("PEER_ADDR");
        if (peerAddr == null || peerAddr.isEmpty()) {
            fatal("PEER_ADDR is not set");
            return;
        }

        int colon = peerAddr.lastIndexOf(':');
        if (colon < 0) fatal("Invalid PEER_ADDR " + peerAddr);
        String host = peerAddr.substring(0, colon);
        int port = Integer.parseInt(peerAddr.substring(colon + 1));

        int negotiatedMaxPayload = LOCAL_MAX_PAYLOAD;
        boolean haveReceivedHello = false;

        try (Socket socket = new Socket()) {
            socket.connect(new InetSocketAddress(host, port), 5000);
            socket.setTcpNoDelay(true);

            InputStream in = socket.getInputStream();
            OutputStream out = socket.getOutputStream();

            sendHello(out);

            ByteArrayOutputStream buf = new ByteArrayOutputStream();
            byte[] tmp = new byte[4096];

            while (true) {
                int n = in.read(tmp);
                if (n < 0) return;
                buf.write(tmp, 0, n);

                byte[] b = buf.toByteArray();
                int idx;
                while ((idx = indexOfZero(b)) >= 0) {
                    byte[] frame = Arrays.copyOfRange(b, 0, idx);
                    b = Arrays.copyOfRange(b, idx + 1, b.length);
                    buf.reset();
                    buf.write(b);

                    if (frame.length == 0) continue;

                    byte[] payload;
                    try {
                        payload = cobsDecode(frame);
                    } catch (Exception e) {
                        sendGoodbye(out, GOODBYE_DECODE_ERROR);
                        return;
                    }

                    try {
                        IntRef off = new IntRef(0);
                        long msgDisc = readUVarint(payload, off);

                        if (msgDisc == 0) { // Hello
                            long helloDisc = readUVarint(payload, off);
                            if (helloDisc != 0) {
                                sendGoodbye(out, GOODBYE_HELLO_UNKNOWN);
                                return;
                            }
                            long remoteMax = readUVarint(payload, off);
                            readUVarint(payload, off); // initial_stream_credit
                            if (remoteMax < negotiatedMaxPayload) negotiatedMaxPayload = (int) remoteMax;
                            haveReceivedHello = true;
                            continue;
                        }

                        if (!haveReceivedHello) continue;

                        if (msgDisc == 2) { // Request
                            readUVarint(payload, off); // request_id
                            readUVarint(payload, off); // method_id
                            skipMetadata(payload, off);
                            long pLen = readUVarint(payload, off);
                            if (pLen > negotiatedMaxPayload) {
                                sendGoodbye(out, GOODBYE_PAYLOAD_LIMIT);
                                return;
                            }
                            continue;
                        }

                        if (msgDisc == 3) { // Response
                            readUVarint(payload, off); // request_id
                            skipMetadata(payload, off);
                            long pLen = readUVarint(payload, off);
                            if (pLen > negotiatedMaxPayload) {
                                sendGoodbye(out, GOODBYE_PAYLOAD_LIMIT);
                                return;
                            }
                            continue;
                        }

                        if (msgDisc == 6 || msgDisc == 7) { // Close / Reset
                            long sid = readUVarint(payload, off);
                            if (sid == 0) {
                                sendGoodbye(out, GOODBYE_STREAM_ID_ZERO);
                                return;
                            }
                        }
                    } catch (EOFException e) {
                        sendGoodbye(out, GOODBYE_DECODE_ERROR);
                        return;
                    } catch (Exception e) {
                        sendGoodbye(out, GOODBYE_DECODE_ERROR);
                        return;
                    }
                }
            }
        }
    }

    private static int indexOfZero(byte[] b) {
        for (int i = 0; i < b.length; i++) if (b[i] == 0) return i;
        return -1;
    }

    private static void sendHello(OutputStream out) throws IOException {
        ByteArrayOutputStream payload = new ByteArrayOutputStream();
        writeUVarint(payload, 0); // Message::Hello
        writeUVarint(payload, 0); // Hello::V1
        writeUVarint(payload, LOCAL_MAX_PAYLOAD);
        writeUVarint(payload, LOCAL_INITIAL_CREDIT);
        writeFrame(out, payload.toByteArray());
    }

    private static void sendGoodbye(OutputStream out, String reason) {
        try {
            ByteArrayOutputStream payload = new ByteArrayOutputStream();
            writeUVarint(payload, 1); // Message::Goodbye
            writeBytes(payload, reason.getBytes(StandardCharsets.UTF_8));
            writeFrame(out, payload.toByteArray());
            out.flush();
        } catch (Exception ignored) {
        }
    }

    private static void writeFrame(OutputStream out, byte[] payload) throws IOException {
        byte[] enc = cobsEncode(payload);
        out.write(enc);
        out.write(0);
        out.flush();
    }

    private static void writeUVarint(ByteArrayOutputStream out, long value) {
        long v = value;
        while (v >= 0x80) {
            out.write((int) (v & 0x7F) | 0x80);
            v >>>= 7;
        }
        out.write((int) v);
    }

    private static long readUVarint(byte[] buf, IntRef off) throws EOFException {
        long result = 0;
        int shift = 0;
        while (true) {
            if (off.v >= buf.length) throw new EOFException("varint eof");
            int b = buf[off.v++] & 0xFF;
            if (shift >= 64) throw new EOFException("varint overflow");
            result |= (long) (b & 0x7F) << shift;
            if ((b & 0x80) == 0) return result;
            shift += 7;
        }
    }

    private static void writeBytes(ByteArrayOutputStream out, byte[] bytes) {
        writeUVarint(out, bytes.length);
        out.writeBytes(bytes);
    }

    private static void skipBytes(byte[] buf, IntRef off) throws EOFException {
        long len = readUVarint(buf, off);
        if (len < 0 || len > (buf.length - off.v)) throw new EOFException("len out of range");
        off.v += (int) len;
    }

    private static void skipMetadata(byte[] buf, IntRef off) throws EOFException {
        long mdLen = readUVarint(buf, off);
        for (long i = 0; i < mdLen; i++) {
            skipBytes(buf, off); // key string
            long disc = readUVarint(buf, off);
            if (disc == 0) {
                skipBytes(buf, off); // string
            } else if (disc == 1) {
                skipBytes(buf, off); // bytes
            } else if (disc == 2) {
                readUVarint(buf, off); // u64
            } else {
                throw new EOFException("unknown metadata value");
            }
        }
    }

    private static byte[] cobsEncode(byte[] input) {
        ByteArrayOutputStream out = new ByteArrayOutputStream(input.length + 2);
        int codeIndex = 0;
        int code = 1;
        out.write(0); // placeholder

        for (byte b : input) {
            int ub = b & 0xFF;
            if (ub == 0) {
                byte[] data = out.toByteArray();
                data[codeIndex] = (byte) code;
                out.reset();
                out.writeBytes(data);

                codeIndex = out.size();
                out.write(0);
                code = 1;
            } else {
                out.write(ub);
                code++;
                if (code == 0xFF) {
                    byte[] data = out.toByteArray();
                    data[codeIndex] = (byte) code;
                    out.reset();
                    out.writeBytes(data);

                    codeIndex = out.size();
                    out.write(0);
                    code = 1;
                }
            }
        }

        byte[] data = out.toByteArray();
        data[codeIndex] = (byte) code;
        return data;
    }

    private static byte[] cobsDecode(byte[] input) throws EOFException {
        ByteArrayOutputStream out = new ByteArrayOutputStream(input.length);
        int i = 0;
        while (i < input.length) {
            int code = input[i++] & 0xFF;
            if (code == 0) throw new EOFException("cobs zero code");
            int n = code - 1;
            if (i + n > input.length) throw new EOFException("cobs overrun");
            out.writeBytes(Arrays.copyOfRange(input, i, i + n));
            i += n;
            if (code != 0xFF && i < input.length) out.write(0);
        }
        return out.toByteArray();
    }

    private static void fatal(String msg) {
        System.err.println(msg);
        System.exit(1);
    }

    private static final class IntRef {
        int v;

        IntRef(int v) {
            this.v = v;
        }
    }
}

