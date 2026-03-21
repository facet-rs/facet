import {
  buildPlan,
  resolveTypeRef,
  schemaSetFromSchemas,
  type TranslationPlan,
  type Schema,
  type SchemaKind,
  type SchemaRegistry,
} from "@bearcove/roam-postcard";
import {
  decodeMessage,
  decodeMessageWithPlan,
  encodeMessage,
  type Message,
  messageRootRef,
  messageSchemaRegistry,
} from "@bearcove/roam-wire";
import type { Link } from "./link.ts";
import { roamLogger } from "./logger.ts";

export interface Conduit<T> {
  send(item: T): Promise<void>;
  recv(): Promise<T | null>;
  close(): void;
  isClosed(): boolean;
}

export interface MessageDecodePlan {
  plan: TranslationPlan;
  remoteRootKind: SchemaKind;
  remoteRegistry: SchemaRegistry;
}

export function buildMessageDecodePlan(peerSchemas: Schema[]): MessageDecodePlan | null {
  if (peerSchemas.length === 0) {
    return null;
  }
  const localRootKind = resolveTypeRef(messageRootRef, messageSchemaRegistry);
  if (!localRootKind) {
    throw new Error("local message root schema not found");
  }
  const remoteSchemaSet = schemaSetFromSchemas(peerSchemas);
  const localSchemaSet = {
    root: {
      id: messageRootRef.tag === "concrete" ? messageRootRef.type_id : 0n,
      type_params: [],
      kind: localRootKind,
    },
    registry: messageSchemaRegistry,
  };
  return {
    plan: buildPlan(remoteSchemaSet, localSchemaSet),
    remoteRootKind: remoteSchemaSet.root.kind,
    remoteRegistry: remoteSchemaSet.registry,
  };
}

export class BareConduit implements Conduit<Message> {
  constructor(
    private readonly link: Link,
    private readonly messagePlan: MessageDecodePlan | null = null,
  ) {}

  async send(item: Message): Promise<void> {
    await this.link.send(encodeMessage(item));
  }

  async recv(): Promise<Message | null> {
    const payload = await this.link.recv();
    if (!payload) {
      return null;
    }
    try {
      return this.messagePlan
        ? decodeMessageWithPlan(
            payload,
            0,
            this.messagePlan.plan,
            this.messagePlan.remoteRootKind,
            this.messagePlan.remoteRegistry,
          ).value
        : decodeMessage(payload).value;
    } catch (e) {
      roamLogger()?.error(`[roam:conduit] decode failed (${payload.length} bytes):`, e);
      throw e;
    }
  }

  close(): void {
    this.link.close();
  }

  isClosed(): boolean {
    return this.link.isClosed();
  }
}
