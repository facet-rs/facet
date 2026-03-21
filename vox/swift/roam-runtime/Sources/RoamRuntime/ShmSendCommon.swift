import Foundation

struct ShmSendErrors<SendError: Error> {
    let payloadTooLarge: SendError
    let ringFull: SendError
    let slotExhausted: SendError
    let slotError: SendError
    let mmapUnavailable: SendError
}

enum ShmSelectedSendPath {
    case inline
    case slot(payloadLen: UInt32, slotPayloadLen: UInt32)
    case mmap(payloadLen: UInt32)
}

func selectShmSendPath<SendError: Error>(
    payloadLen: UInt32,
    maxPayloadSize: UInt32,
    inlineThreshold: UInt32,
    maxVarSlotPayload: UInt32,
    role: String,
    errors: ShmSendErrors<SendError>
) throws -> ShmSelectedSendPath {
    if payloadLen > maxPayloadSize {
        throw errors.payloadTooLarge
    }

    let threshold = inlineThreshold == 0 ? shmDefaultInlineThreshold : inlineThreshold
    if shmShouldInline(payloadLen: payloadLen, threshold: threshold) {
        traceLog(
            .shm,
            "\(role) send payload=\(payloadLen) threshold=\(threshold) max_slot=\(maxVarSlotPayload) path=inline"
        )
        return .inline
    }

    let slotPayloadLen = payloadLen &+ 4
    guard slotPayloadLen >= payloadLen else {
        throw errors.payloadTooLarge
    }
    if payloadLen <= maxVarSlotPayload {
        traceLog(
            .shm,
            "\(role) send payload=\(payloadLen) threshold=\(threshold) max_slot=\(maxVarSlotPayload) path=slot"
        )
        return .slot(payloadLen: payloadLen, slotPayloadLen: slotPayloadLen)
    }

    traceLog(
        .shm,
        "\(role) send payload=\(payloadLen) threshold=\(threshold) max_slot=\(maxVarSlotPayload) path=mmap"
    )
    return .mmap(payloadLen: payloadLen)
}

func sendShmFrame<SendError: Error>(
    role: String,
    frame: ShmGuestFrame,
    header: ShmSegmentHeader,
    outbox: ShmBipBuffer,
    slotPool: ShmVarSlotPool,
    slotOwner: UInt8,
    doorbell: ShmDoorbell?,
    maxVarSlotPayload: UInt32,
    mmapControlFd: Int32,
    errors: ShmSendErrors<SendError>,
    allocateMmapRef: (_ payload: [UInt8], _ payloadLen: UInt32) throws -> ShmMmapRef
) throws {
    let payloadLen = UInt32(frame.payload.count)
    let path = try selectShmSendPath(
        payloadLen: payloadLen,
        maxPayloadSize: header.maxPayloadSize,
        inlineThreshold: header.inlineThreshold,
        maxVarSlotPayload: maxVarSlotPayload,
        role: role,
        errors: errors
    )

    switch path {
    case .inline:
        let bytes = encodeShmInlineFrame(payload: frame.payload)
        if let grant = try outbox.tryGrant(UInt32(bytes.count)) {
            grant.copyBytes(from: bytes)
            try outbox.commit(UInt32(bytes.count))
            try doorbell?.signal()
            return
        }
        throw errors.ringFull

    case .slot(let payloadLen, let slotPayloadLen):
        guard let handle = slotPool.alloc(size: slotPayloadLen, owner: slotOwner) else {
            throw errors.slotExhausted
        }

        guard let payloadPtr = slotPool.payloadPointer(handle) else {
            try? slotPool.freeAllocated(handle)
            throw errors.slotError
        }

        frame.payload.withUnsafeBytes { raw in
            if let base = raw.baseAddress {
                payloadPtr.storeBytes(of: payloadLen.littleEndian, as: UInt32.self)
                memcpy(payloadPtr.advanced(by: 4), base, raw.count)
            }
        }

        do {
            try slotPool.markInFlight(handle)
        } catch {
            try? slotPool.freeAllocated(handle)
            throw errors.slotError
        }

        let slotFrame = encodeShmSlotRefFrame(
            slotRef: ShmSlotRef(
                classIdx: handle.classIdx,
                extentIdx: handle.extentIdx,
                slotIdx: handle.slotIdx,
                slotGeneration: handle.generation
            )
        )

        if let grant = try outbox.tryGrant(UInt32(slotFrame.count)) {
            grant.copyBytes(from: slotFrame)
            try outbox.commit(UInt32(slotFrame.count))
            try doorbell?.signal()
            return
        }

        try? slotPool.free(handle)
        throw errors.ringFull

    case .mmap(let payloadLen):
        guard mmapControlFd >= 0 else {
            throw errors.mmapUnavailable
        }

        let frameSize = UInt32(shmFrameHeaderSize + shmMmapRefSize)
        guard let grant = try outbox.tryGrant(frameSize) else {
            throw errors.ringFull
        }

        let mmapRef = try allocateMmapRef(frame.payload, payloadLen)
        let mmapFrame = encodeShmMmapRefFrame(mmapRef: mmapRef)
        grant.copyBytes(from: mmapFrame)
        try outbox.commit(UInt32(mmapFrame.count))
        try doorbell?.signal()
    }
}
