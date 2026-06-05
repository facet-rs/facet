import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

interface PackageJson {
  name: string;
  dependencies?: Record<string, string>;
}

function readPackage(relative: string): PackageJson {
  const path = fileURLToPath(new URL(relative, import.meta.url));
  return JSON.parse(readFileSync(path, "utf8")) as PackageJson;
}

function deps(pkg: PackageJson): Set<string> {
  return new Set(Object.keys(pkg.dependencies ?? {}));
}

// r[verify crates.concern-separation]
// r[verify crates.engine-is-binding-free]
describe("TypeScript package boundaries", () => {
  const schema = readPackage("../../phon-schema/package.json");
  const engine = readPackage("../package.json");
  const frontDoor = readPackage("../../phon/package.json");

  it("keeps schema, engine, and front door as separate packages", () => {
    expect(schema.name).toBe("@bearcove/phon-schema");
    expect(engine.name).toBe("@bearcove/phon-engine");
    expect(frontDoor.name).toBe("@bearcove/phon");

    expect(deps(schema)).not.toContain("@bearcove/phon-engine");
    expect(deps(schema)).not.toContain("@bearcove/phon");
    expect(deps(engine)).toContain("@bearcove/phon-schema");
    expect(deps(engine)).not.toContain("@bearcove/phon");
    expect(deps(frontDoor)).toEqual(new Set(["@bearcove/phon-engine", "@bearcove/phon-schema"]));
  });
});
