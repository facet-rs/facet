private let peepsMethodNameMetadataKey = "moire.method_name"
private let peepsRequestEntityIdMetadataKey = "moire.request_entity_id"

/// Carry the moire routing keys from a request's metadata into its response.
func responseMetadataFromRequest(_ requestMetadata: Metadata) -> Metadata {
    var responseMetadata: Metadata = .null
    for (key, value) in requestMetadata.metaEntries() {
        if key == peepsMethodNameMetadataKey || key == peepsRequestEntityIdMetadataKey {
            responseMetadata.metaSet(key, value)
        }
    }
    return responseMetadata
}
