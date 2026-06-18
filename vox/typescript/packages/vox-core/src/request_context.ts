import { type Metadata, emptyMetadata } from "@bearcove/vox-wire";
import type { MethodDescriptor } from "./channeling/descriptor.ts";
import {
  anonymousRequestAuthorizationContext,
  type RequestAuthorizationContext,
} from "./handshake.ts";
import { Extensions } from "./middleware.ts";
import { ClientMetadata } from "./metadata.ts";

export interface RequestContextOptions {
  readonly requestId?: bigint;
  readonly laneId?: bigint;
  readonly authorization?: RequestAuthorizationContext;
  readonly extensions?: Extensions;
}

// r[impl request.authorization]
export class RequestContext {
  readonly serviceName: string;
  readonly method: MethodDescriptor;
  readonly metadata: ClientMetadata;
  readonly extensions: Extensions;
  readonly requestId?: bigint;
  readonly laneId?: bigint;
  readonly authorization: RequestAuthorizationContext;

  constructor(
    serviceName: string,
    method: MethodDescriptor,
    metadata: Metadata = emptyMetadata(),
    optionsOrExtensions: RequestContextOptions | Extensions = new Extensions(),
  ) {
    const options = optionsOrExtensions instanceof Extensions
      ? { extensions: optionsOrExtensions }
      : optionsOrExtensions;
    this.serviceName = serviceName;
    this.method = method;
    this.metadata = ClientMetadata.fromWire(metadata);
    this.extensions = options.extensions ?? new Extensions();
    this.requestId = options.requestId;
    this.laneId = options.laneId;
    this.authorization = options.authorization ?? anonymousRequestAuthorizationContext();
  }
}
