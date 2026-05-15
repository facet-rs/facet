import type { MetadataEntry } from "@bearcove/vox-wire";
import type { MethodDescriptor } from "./channeling/descriptor.ts";
import { Extensions } from "./middleware.ts";
import { ClientMetadata } from "./metadata.ts";

export class RequestContext {
  readonly serviceName: string;
  readonly method: MethodDescriptor;
  readonly metadata: ClientMetadata;
  readonly extensions: Extensions;

  constructor(
    serviceName: string,
    method: MethodDescriptor,
    entries: MetadataEntry[],
    extensions: Extensions = new Extensions(),
  ) {
    this.serviceName = serviceName;
    this.method = method;
    this.metadata = ClientMetadata.fromWireEntries(entries);
    this.extensions = extensions;
  }
}
