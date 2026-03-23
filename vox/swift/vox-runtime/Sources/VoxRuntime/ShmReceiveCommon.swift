import Foundation

struct ShmReceiveErrors<ReceiveError: Error> {
    let malformedFrame: ReceiveError
    let slotError: ReceiveError
    let payloadTooLarge: ReceiveError
}

func receiveShmFrame<ReceiveError: Error>(
    bytes: [UInt8],
    maxPayloadSize: UInt32,
    inbox: ShmBipBuffer,
    slotPool: ShmVarSlotPool,
    doorbell: ShmDoorbell?,
    mmapAttachments: ShmMmapAttachments?,
    errors: ShmReceiveErrors<ReceiveError>
) throws -> ShmGuestFrame {
    let decoded: ShmDecodedFrame
    do {
        decoded = try decodeShmFrame(bytes)
    } catch {
        throw errors.malformedFrame
    }

    switch decoded {
    case .inline(let header, let payload):
        try inbox.release(header.totalLen)
        return ShmGuestFrame(payload: payload)

    case .slotRef(let header, let slotRef):
        let handle = ShmVarSlotHandle(
            classIdx: slotRef.classIdx,
            extentIdx: slotRef.extentIdx,
            slotIdx: slotRef.slotIdx,
            generation: slotRef.slotGeneration
        )

        guard let clsSize = slotPool.slotSize(classIdx: slotRef.classIdx), clsSize >= 4 else {
            throw errors.slotError
        }
        guard let payloadPtr = slotPool.payloadPointer(handle) else {
            throw errors.slotError
        }

        let slotBytes = UnsafeRawBufferPointer(start: UnsafeRawPointer(payloadPtr), count: Int(clsSize))
        let payloadLen = readShmU32LE(Array(slotBytes.prefix(4)), 0)
        if payloadLen > clsSize - 4 {
            throw errors.payloadTooLarge
        }
        let payload = Array(
            UnsafeRawBufferPointer(
                start: UnsafeRawPointer(payloadPtr.advanced(by: 4)),
                count: Int(payloadLen)
            )
        )

        do {
            try slotPool.free(handle)
        } catch {
            throw errors.slotError
        }

        try inbox.release(header.totalLen)
        try doorbell?.signal()
        return ShmGuestFrame(payload: payload)

    case .mmapRef(let header, let mmapRef):
        guard mmapRef.payloadLen <= maxPayloadSize else {
            throw errors.payloadTooLarge
        }
        guard let mmapAttachments,
            mmapAttachments.drainControl(),
            let payload = mmapAttachments.resolve(mmapRef: mmapRef)
        else {
            throw errors.malformedFrame
        }
        try inbox.release(header.totalLen)
        try doorbell?.signal()
        return ShmGuestFrame(payload: payload)
    }
}

@inline(__always)
func readShmU32LE(_ bytes: [UInt8], _ at: Int) -> UInt32 {
    UInt32(bytes[at])
        | (UInt32(bytes[at + 1]) << 8)
        | (UInt32(bytes[at + 2]) << 16)
        | (UInt32(bytes[at + 3]) << 24)
}
