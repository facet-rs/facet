private let peepsMethodNameMetadataKey = "moire.method_name"
private let peepsRequestEntityIdMetadataKey = "moire.request_entity_id"

func responseMetadataFromRequest(_ requestMetadata: [MetadataEntryV7]) -> [MetadataEntryV7] {
    var responseMetadata: [MetadataEntryV7] = []
    for entry in requestMetadata {
        if entry.key == peepsMethodNameMetadataKey || entry.key == peepsRequestEntityIdMetadataKey {
            responseMetadata.append(entry)
        }
    }
    return responseMetadata
}
