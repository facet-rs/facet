import Foundation
import PhonSchema

// r[impl connection.evidence]
// r[impl connection.identity.use-cases]
// r[impl connection.identity.redaction]
public struct PeerEvidence: Sendable {
    public let items: [PeerEvidenceItem]

    public static let none = PeerEvidence(items: [])

    init(items: [PeerEvidenceItem] = []) {
        self.items = items
    }

    static func synthetic(_ label: String) -> PeerEvidence {
        PeerEvidence(items: [.synthetic(label: label)])
    }
}

// r[impl connection.identity.use-cases]
// r[impl connection.identity.redaction]
public enum PeerEvidenceItem: Sendable {
    case synthetic(label: String)
    case tls(verifiedSubject: String?, alpn: String?)
    case unixPeerCredentials(uid: UInt32?, gid: UInt32?, pid: UInt32?)
    case platformProcess(description: String)
    case xpc(codeSigningIdentity: String)
    case inProcess(component: String)
}

public enum PeerIdentityForm: Sendable, Equatable {
    case anonymous
    case synthetic
    case localProcess
    case certificateBacked
    case applicationUser
    case composite
}

public enum IdentityBasisProvenance: Sendable, Equatable {
    case evidenceBacked
    case verifiedClaimBacked
    case synthetic
}

// r[impl connection.identity.forms]
public struct IdentityBasis: Sendable, Equatable {
    public let form: PeerIdentityForm
    public let provenance: IdentityBasisProvenance
    public let redacted: String

    public init(
        form: PeerIdentityForm,
        provenance: IdentityBasisProvenance,
        redacted: String
    ) {
        self.form = form
        self.provenance = provenance
        self.redacted = redacted
    }
}

// r[impl connection.identity]
// r[impl connection.identity.late-claims]
// r[impl connection.identity.scope]
public struct PeerIdentity: Sendable, Equatable {
    public let epoch: UInt64
    public let form: PeerIdentityForm
    public let bases: [IdentityBasis]

    public static let anonymous = PeerIdentity(epoch: 0, form: .anonymous, bases: [])

    public init(epoch: UInt64 = 0, form: PeerIdentityForm, bases: [IdentityBasis]) {
        self.epoch = epoch
        self.form = form
        self.bases = bases
    }

    public static func fromBasis(_ basis: IdentityBasis) -> PeerIdentity {
        PeerIdentity(form: basis.form, bases: [basis])
    }

    public static func composite(_ bases: [IdentityBasis]) -> PeerIdentity {
        PeerIdentity(
            form: bases.count <= 1 ? (bases.first?.form ?? .anonymous) : .composite,
            bases: bases
        )
    }
}

// r[impl lane.authorization.context]
// r[impl request.authorization]
public struct LaneGrant: Sendable {
    public let metadata: Metadata

    public static let empty = LaneGrant(metadata: .null)

    public init(metadata: Metadata = .null) {
        self.metadata = metadata
    }
}

// r[impl request.authorization]
public struct RequestAuthorizationContext: Sendable {
    public let peerIdentity: PeerIdentity
    public let peerEvidence: PeerEvidence
    public let laneGrant: LaneGrant

    public static let anonymous = RequestAuthorizationContext(
        peerIdentity: .anonymous,
        peerEvidence: .none,
        laneGrant: .empty
    )

    public init(
        peerIdentity: PeerIdentity,
        peerEvidence: PeerEvidence,
        laneGrant: LaneGrant
    ) {
        self.peerIdentity = peerIdentity
        self.peerEvidence = peerEvidence
        self.laneGrant = laneGrant
    }
}

// r[impl request.authorization]
public struct RequestContext: Sendable {
    public let methodId: UInt64
    public let requestId: UInt64
    public let laneId: UInt64
    public let metadata: Metadata
    public let authorization: RequestAuthorizationContext

    public init(
        methodId: UInt64,
        requestId: UInt64,
        laneId: UInt64,
        metadata: Metadata,
        authorization: RequestAuthorizationContext
    ) {
        self.methodId = methodId
        self.requestId = requestId
        self.laneId = laneId
        self.metadata = metadata
        self.authorization = authorization
    }
}

// r[impl connection.identity.inputs]
// r[impl connection.identity.local]
public struct IdentityResolutionContext: Sendable {
    public let role: VoxEstablishmentRole
    public let evidence: PeerEvidence
    public let claims: Metadata
}

public typealias IdentityResolver =
    @Sendable (IdentityResolutionContext) async throws -> PeerIdentity

public struct ConnectionDeclinedError: Error, Sendable {
    public let decline: Decline
    public let receivedFromPeer: Bool

    public init(decline: Decline, receivedFromPeer: Bool = false) {
        self.decline = decline
        self.receivedFromPeer = receivedFromPeer
    }

    public init(
        reason: EstablishmentRejectReason,
        metadata: Metadata = .null,
        receivedFromPeer: Bool = false
    ) {
        self.init(
            decline: Decline(reason: reason, metadata: metadata),
            receivedFromPeer: receivedFromPeer
        )
    }
}
