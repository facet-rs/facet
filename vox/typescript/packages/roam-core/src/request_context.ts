import type { MetadataEntry } from "@bearcove/roam-wire";
import type { MethodDescriptor } from "./channeling/descriptor.ts";
import { Extensions } from "./middleware.ts";
import { ClientMetadata } from "./metadata.ts";

export class RequestContext {
  readonly metadata: ClientMetadata;
  readonly extensions: Extensions;

  constructor(
    readonly serviceName: string,
    readonly method: MethodDescriptor,
    entries: MetadataEntry[],
    extensions: Extensions = new Extensions(),
  ) {
    this.metadata = ClientMetadata.fromWireEntries(entries);
    this.extensions = extensions;
  }
}
