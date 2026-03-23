private let peepsMethodNameMetadataKey = "moire.method_name"
private let peepsRequestEntityIdMetadataKey = "moire.request_entity_id"

func responseMetadataFromRequest(_ requestMetadata: [MetadataEntry]) -> [MetadataEntry] {
    var responseMetadata: [MetadataEntry] = []
    for entry in requestMetadata {
        if entry.key == peepsMethodNameMetadataKey || entry.key == peepsRequestEntityIdMetadataKey {
            responseMetadata.append(entry)
        }
    }
    return responseMetadata
}
