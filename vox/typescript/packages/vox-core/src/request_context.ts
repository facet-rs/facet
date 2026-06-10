import { type Metadata, emptyMetadata } from "@bearcove/vox-wire";
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
    metadata: Metadata = emptyMetadata(),
    extensions: Extensions = new Extensions(),
  ) {
    this.serviceName = serviceName;
    this.method = method;
    this.metadata = ClientMetadata.fromWire(metadata);
    this.extensions = extensions;
  }
}
