import PhonJIT
import VoxRuntime

public func enableVoxJIT() {
    VoxTypedCodec.configure(engine: JITEngine())
}
