// Evolved subject for schema compatibility testing.
//
// Implements the Testbed service using evolved types (added fields,
// reordered fields, added enum variants, etc.) to test that schema
// exchange and translation plans bridge the differences.

import type {
  TestbedHandler,
  Profile,
  Record,
  Status,
  Tag,
  Measurement,
  Config,
} from "@bearcove/roam-generated/testbed_evolved.generated.ts";
import { TestbedDispatcher } from "@bearcove/roam-generated/testbed_evolved.generated.ts";
import { setRoamLogger } from "@bearcove/roam-core";
import { runSubjectServer } from "./harness.ts";

setRoamLogger({
  debug: (...args) => console.error(...args),
  error: (...args) => console.error(...args),
});

class EvolvedTestbedService implements TestbedHandler {
  echoProfile(profile: Profile): Profile {
    return profile;
  }

  echoRecord(record: Record): Record {
    return record;
  }

  echoStatus(status: Status): Status {
    return status;
  }

  echoTag(tag: Tag): Tag {
    return tag;
  }

  echoMeasurement(m: Measurement): Measurement {
    return m;
  }

  echoConfig(c: Config): Config {
    return c;
  }
}

runSubjectServer(() => new TestbedDispatcher(new EvolvedTestbedService())).catch((e) => {
  console.error(e);
  process.exit(1);
});
