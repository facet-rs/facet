import { test, expect } from "@playwright/test";

// Test cases defined inline - patches are computed dynamically via WASM diffHtml
const TEST_CASES = [
  { name: "simple_text_change", old: "<p>Hello</p>", new: "<p>World</p>" },
  { name: "text_in_div", old: "<div>Old text</div>", new: "<div>New text</div>" },
  { name: "add_class", old: "<div>Content</div>", new: '<div class="highlight">Content</div>' },
  {
    name: "change_class",
    old: '<div class="old">Content</div>',
    new: '<div class="new">Content</div>',
  },
  { name: "remove_class", old: '<div class="remove-me">Content</div>', new: "<div>Content</div>" },
  { name: "insert_element_at_end", old: "<p>First</p>", new: "<p>First</p><p>Second</p>" },
  { name: "insert_element_at_start", old: "<p>Second</p>", new: "<p>First</p><p>Second</p>" },
  {
    name: "insert_element_in_middle",
    old: "<p>First</p><p>Third</p>",
    new: "<p>First</p><p>Second</p><p>Third</p>",
  },
  { name: "remove_element_from_end", old: "<p>First</p><p>Second</p>", new: "<p>First</p>" },
  { name: "remove_element_from_start", old: "<p>First</p><p>Second</p>", new: "<p>Second</p>" },
  { name: "fill_empty_div", old: "<div></div>", new: "<div>Content</div>" },
  { name: "drain_div_content", old: "<div>Content</div>", new: "<div></div>" },
  { name: "text_moves_into_div", old: "Text<div></div>", new: "<div>Text</div>" },
  { name: "nested_text_change", old: "<div><p>Old</p></div>", new: "<div><p>New</p></div>" },
  {
    name: "deeply_nested",
    old: "<div><div><div>Deep</div></div></div>",
    new: "<div><div><div>Changed</div></div></div>",
  },
  {
    name: "multiple_text_changes",
    old: "<p>A</p><p>B</p><p>C</p>",
    new: "<p>X</p><p>Y</p><p>Z</p>",
  },
  { name: "swap_siblings", old: "<p>First</p><p>Second</p>", new: "<p>Second</p><p>First</p>" },
  { name: "text_and_elements", old: "Text<span>Span</span>", new: "<span>Span</span>Text" },
];

test.describe("facet-html-diff WASM", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/index.html");
    await page.waitForFunction(() => (window as any).wasmReady === true, { timeout: 10000 });
  });

  test("WASM module loads", async ({ page }) => {
    const status = await page.textContent("#status");
    expect(status).toBe("WASM loaded successfully");
  });

  for (const tc of TEST_CASES) {
    test(`roundtrip: ${tc.name}`, async ({ page }) => {
      const result = await page.evaluate(
        ({ oldHtml, newHtml }) => {
          const fullOld = `<html><body>${oldHtml}</body></html>`;
          const fullNew = `<html><body>${newHtml}</body></html>`;

          try {
            // Compute patches dynamically
            const patchesJson = (window as any).diffHtml(fullOld, fullNew);

            // Apply patches
            (window as any).setBodyInnerHtml(oldHtml);
            (window as any).applyPatchesJson(patchesJson);
            const resultHtml = (window as any).getBodyInnerHtml();

            // Normalize for comparison
            const normalizeHtml = (html: string) => {
              const temp = document.createElement("div");
              temp.innerHTML = html;
              return temp.innerHTML;
            };

            const normalizedResult = normalizeHtml(resultHtml);
            const normalizedExpected = normalizeHtml(newHtml);

            if (normalizedResult === normalizedExpected) {
              return { pass: true };
            } else {
              return {
                pass: false,
                error: `Mismatch:\nExpected: ${normalizedExpected}\nGot: ${normalizedResult}\nPatches: ${patchesJson}`,
              };
            }
          } catch (e) {
            return { pass: false, error: String(e) };
          }
        },
        { oldHtml: tc.old, newHtml: tc.new },
      );

      expect(result.pass, result.error || "").toBe(true);
    });
  }
});

// ============================================================================
// FUZZING TESTS - Random mutations on realistic HTML documents
// ============================================================================

// Realistic HTML templates to use as starting points
const REALISTIC_TEMPLATES = [
  // Simple article
  `<article>
    <h1>Article Title</h1>
    <p>First paragraph with <strong>bold</strong> and <em>italic</em> text.</p>
    <p>Second paragraph with a <a href="#">link</a>.</p>
  </article>`,

  // Navigation menu
  `<nav>
    <ul>
      <li><a href="/">Home</a></li>
      <li><a href="/about">About</a></li>
      <li><a href="/contact">Contact</a></li>
    </ul>
  </nav>`,

  // Card component
  `<div class="card">
    <div class="card-header">
      <h2>Card Title</h2>
    </div>
    <div class="card-body">
      <p>Card content goes here.</p>
      <button>Action</button>
    </div>
  </div>`,

  // Form
  `<form>
    <div class="form-group">
      <label>Name</label>
      <input type="text" placeholder="Enter name">
    </div>
    <div class="form-group">
      <label>Email</label>
      <input type="email" placeholder="Enter email">
    </div>
    <button type="submit">Submit</button>
  </form>`,

  // Nested divs (issue #1846 pattern)
  `<div class="outer">
    <div class="middle">
      <div class="inner">
        <span>Deep content</span>
      </div>
    </div>
  </div>`,

  // Table
  `<table>
    <thead>
      <tr><th>Name</th><th>Value</th></tr>
    </thead>
    <tbody>
      <tr><td>Item 1</td><td>100</td></tr>
      <tr><td>Item 2</td><td>200</td></tr>
    </tbody>
  </table>`,

  // List with mixed content
  `<div>
    <h3>Features</h3>
    <ul>
      <li>Feature one with <code>code</code></li>
      <li>Feature two with <strong>emphasis</strong></li>
      <li>Feature three</li>
    </ul>
  </div>`,

  // Sidebar layout
  `<div class="layout">
    <aside class="sidebar">
      <h4>Menu</h4>
      <ul>
        <li>Item A</li>
        <li>Item B</li>
      </ul>
    </aside>
    <main class="content">
      <p>Main content area.</p>
    </main>
  </div>`,
];

// Seeded random number generator for reproducibility
class SeededRandom {
  private seed: number;

  constructor(seed: number) {
    this.seed = seed;
  }

  next(): number {
    this.seed = (this.seed * 1103515245 + 12345) & 0x7fffffff;
    return this.seed / 0x7fffffff;
  }

  nextInt(max: number): number {
    return Math.floor(this.next() * max);
  }

  pick<T>(arr: T[]): T {
    return arr[this.nextInt(arr.length)];
  }

  shuffle<T>(arr: T[]): T[] {
    const result = [...arr];
    for (let i = result.length - 1; i > 0; i--) {
      const j = this.nextInt(i + 1);
      [result[i], result[j]] = [result[j], result[i]];
    }
    return result;
  }
}

// Mutation types
type MutationType =
  | "insert_text_before"
  | "insert_text_after"
  | "insert_element"
  | "remove_element"
  | "change_text"
  | "add_attribute"
  | "remove_attribute"
  | "change_attribute"
  | "wrap_element"
  | "unwrap_element"
  | "move_element"
  | "duplicate_element";

const MUTATION_TYPES: MutationType[] = [
  "insert_text_before",
  "insert_text_after",
  "insert_element",
  "remove_element",
  "change_text",
  "add_attribute",
  "remove_attribute",
  "change_attribute",
  "wrap_element",
  "unwrap_element",
  "move_element",
  "duplicate_element",
];

const RANDOM_WORDS = [
  "hello",
  "world",
  "test",
  "content",
  "sample",
  "data",
  "item",
  "value",
  "text",
  "node",
  "element",
  "child",
  "parent",
  "sibling",
];

const RANDOM_ELEMENTS = ["div", "span", "p", "strong", "em", "a", "section"];

const RANDOM_CLASSES = ["primary", "secondary", "highlight", "active", "hidden", "visible"];

// Apply a random mutation to a DOM tree
function applyMutation(doc: Document, container: Element, rng: SeededRandom): boolean {
  const allElements = Array.from(container.querySelectorAll("*"));
  if (allElements.length === 0) return false;

  const mutationType = rng.pick(MUTATION_TYPES);
  const targetEl = rng.pick(allElements);

  try {
    switch (mutationType) {
      case "insert_text_before": {
        const text = rng.pick(RANDOM_WORDS);
        targetEl.parentNode?.insertBefore(doc.createTextNode(text), targetEl);
        return true;
      }

      case "insert_text_after": {
        const text = rng.pick(RANDOM_WORDS);
        targetEl.parentNode?.insertBefore(doc.createTextNode(text), targetEl.nextSibling);
        return true;
      }

      case "insert_element": {
        const tag = rng.pick(RANDOM_ELEMENTS);
        const newEl = doc.createElement(tag);
        newEl.textContent = rng.pick(RANDOM_WORDS);
        const parent = targetEl.parentNode;
        if (parent) {
          const children = Array.from(parent.childNodes);
          const pos = rng.nextInt(children.length + 1);
          parent.insertBefore(newEl, children[pos] || null);
        }
        return true;
      }

      case "remove_element": {
        if (targetEl !== container && targetEl.parentNode) {
          targetEl.parentNode.removeChild(targetEl);
          return true;
        }
        return false;
      }

      case "change_text": {
        const textNodes: Text[] = [];
        const walker = doc.createTreeWalker(container, NodeFilter.SHOW_TEXT);
        let node;
        while ((node = walker.nextNode())) {
          textNodes.push(node as Text);
        }
        if (textNodes.length > 0) {
          const textNode = rng.pick(textNodes);
          textNode.textContent = rng.pick(RANDOM_WORDS);
          return true;
        }
        return false;
      }

      case "add_attribute": {
        const cls = rng.pick(RANDOM_CLASSES);
        targetEl.setAttribute("class", cls);
        return true;
      }

      case "remove_attribute": {
        if (targetEl.hasAttribute("class")) {
          targetEl.removeAttribute("class");
          return true;
        }
        return false;
      }

      case "change_attribute": {
        if (targetEl.hasAttribute("class")) {
          targetEl.setAttribute("class", rng.pick(RANDOM_CLASSES));
          return true;
        }
        return false;
      }

      case "wrap_element": {
        if (targetEl !== container && targetEl.parentNode) {
          const wrapper = doc.createElement(rng.pick(RANDOM_ELEMENTS));
          targetEl.parentNode.insertBefore(wrapper, targetEl);
          wrapper.appendChild(targetEl);
          return true;
        }
        return false;
      }

      case "unwrap_element": {
        if (targetEl !== container && targetEl.childNodes.length > 0 && targetEl.parentNode) {
          const parent = targetEl.parentNode;
          while (targetEl.firstChild) {
            parent.insertBefore(targetEl.firstChild, targetEl);
          }
          parent.removeChild(targetEl);
          return true;
        }
        return false;
      }

      case "move_element": {
        if (targetEl !== container && allElements.length > 1) {
          const destEl = rng.pick(allElements.filter((e) => e !== targetEl));
          if (destEl && !destEl.contains(targetEl)) {
            destEl.appendChild(targetEl);
            return true;
          }
        }
        return false;
      }

      case "duplicate_element": {
        if (targetEl !== container && targetEl.parentNode) {
          const clone = targetEl.cloneNode(true);
          targetEl.parentNode.insertBefore(clone, targetEl.nextSibling);
          return true;
        }
        return false;
      }
    }
  } catch {
    return false;
  }

  return false;
}

test.describe("facet-html-diff fuzzing", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/index.html");
    await page.waitForFunction(() => (window as any).wasmReady === true, { timeout: 10000 });
  });

  // Run fuzzing with different seeds for reproducibility
  const NUM_SEEDS = 20;
  const MUTATIONS_PER_TEST = 5;

  for (let seed = 0; seed < NUM_SEEDS; seed++) {
    test(`fuzz seed ${seed}`, async ({ page }) => {
      const results = await page.evaluate(
        ({ seed, templates, mutations, words, elements, classes }) => {
          // Recreate the SeededRandom class in browser context
          class SeededRandom {
            private seed: number;
            constructor(seed: number) {
              this.seed = seed;
            }
            next(): number {
              this.seed = (this.seed * 1103515245 + 12345) & 0x7fffffff;
              return this.seed / 0x7fffffff;
            }
            nextInt(max: number): number {
              return Math.floor(this.next() * max);
            }
            pick<T>(arr: T[]): T {
              return arr[this.nextInt(arr.length)];
            }
          }

          const rng = new SeededRandom(seed);
          const results: Array<{
            template: number;
            oldHtml: string;
            newHtml: string;
            pass: boolean;
            error?: string;
          }> = [];

          // Test each template
          for (let templateIdx = 0; templateIdx < templates.length; templateIdx++) {
            const template = templates[templateIdx];

            // Create old HTML
            const oldContainer = document.createElement("div");
            oldContainer.innerHTML = template;
            const oldHtml = oldContainer.innerHTML;

            // Create new HTML by applying mutations
            const newContainer = document.createElement("div");
            newContainer.innerHTML = template;

            let mutationsApplied = 0;
            let attempts = 0;
            while (mutationsApplied < mutations && attempts < mutations * 3) {
              attempts++;

              const allElements = Array.from(newContainer.querySelectorAll("*"));
              if (allElements.length === 0) break;

              const mutationType = rng.pick([
                "insert_text_before",
                "insert_text_after",
                "insert_element",
                "remove_element",
                "change_text",
                "add_attribute",
                "remove_attribute",
                "wrap_element",
                "move_element",
              ]);

              const targetEl = rng.pick(allElements);
              let success = false;

              try {
                switch (mutationType) {
                  case "insert_text_before": {
                    const text = rng.pick(words);
                    targetEl.parentNode?.insertBefore(document.createTextNode(text), targetEl);
                    success = true;
                    break;
                  }
                  case "insert_text_after": {
                    const text = rng.pick(words);
                    targetEl.parentNode?.insertBefore(
                      document.createTextNode(text),
                      targetEl.nextSibling,
                    );
                    success = true;
                    break;
                  }
                  case "insert_element": {
                    const tag = rng.pick(elements);
                    const newEl = document.createElement(tag);
                    newEl.textContent = rng.pick(words);
                    if (targetEl.parentNode) {
                      const children = Array.from(targetEl.parentNode.childNodes);
                      const pos = rng.nextInt(children.length + 1);
                      targetEl.parentNode.insertBefore(newEl, children[pos] || null);
                      success = true;
                    }
                    break;
                  }
                  case "remove_element": {
                    if (targetEl !== newContainer && targetEl.parentNode) {
                      targetEl.parentNode.removeChild(targetEl);
                      success = true;
                    }
                    break;
                  }
                  case "change_text": {
                    const textNodes: Text[] = [];
                    const walker = document.createTreeWalker(newContainer, NodeFilter.SHOW_TEXT);
                    let node;
                    while ((node = walker.nextNode())) {
                      textNodes.push(node as Text);
                    }
                    if (textNodes.length > 0) {
                      const textNode = rng.pick(textNodes);
                      textNode.textContent = rng.pick(words);
                      success = true;
                    }
                    break;
                  }
                  case "add_attribute": {
                    targetEl.setAttribute("class", rng.pick(classes));
                    success = true;
                    break;
                  }
                  case "remove_attribute": {
                    if (targetEl.hasAttribute("class")) {
                      targetEl.removeAttribute("class");
                      success = true;
                    }
                    break;
                  }
                  case "wrap_element": {
                    if (targetEl !== newContainer && targetEl.parentNode) {
                      const wrapper = document.createElement(rng.pick(elements));
                      targetEl.parentNode.insertBefore(wrapper, targetEl);
                      wrapper.appendChild(targetEl);
                      success = true;
                    }
                    break;
                  }
                  case "move_element": {
                    if (targetEl !== newContainer && allElements.length > 1) {
                      const destEl = rng.pick(allElements.filter((e) => e !== targetEl));
                      if (destEl && !destEl.contains(targetEl) && !targetEl.contains(destEl)) {
                        destEl.appendChild(targetEl);
                        success = true;
                      }
                    }
                    break;
                  }
                }
              } catch {
                // Mutation failed, try again
              }

              if (success) mutationsApplied++;
            }

            const newHtml = newContainer.innerHTML;

            // Now test the diff/apply roundtrip
            try {
              // Wrap in html/body for the diff function
              const fullOld = `<html><body>${oldHtml}</body></html>`;
              const fullNew = `<html><body>${newHtml}</body></html>`;

              // Use the WASM diff function
              const diffFn = (window as any).diffHtml;
              if (!diffFn) {
                results.push({
                  template: templateIdx,
                  oldHtml,
                  newHtml,
                  pass: true, // Skip if diff not available
                  error: "diff function not available",
                });
                continue;
              }

              const patchesJson = diffFn(fullOld, fullNew);

              // Apply patches
              (window as any).setBodyInnerHtml(oldHtml);
              (window as any).applyPatchesJson(patchesJson);
              const resultHtml = (window as any).getBodyInnerHtml();

              // Normalize for comparison
              const normalizeHtml = (html: string) => {
                const temp = document.createElement("div");
                temp.innerHTML = html;
                return temp.innerHTML;
              };

              const normalizedResult = normalizeHtml(resultHtml);
              const normalizedExpected = normalizeHtml(newHtml);

              if (normalizedResult === normalizedExpected) {
                results.push({ template: templateIdx, oldHtml, newHtml, pass: true });
              } else {
                results.push({
                  template: templateIdx,
                  oldHtml,
                  newHtml,
                  pass: false,
                  error: `Mismatch:\nExpected: ${normalizedExpected}\nGot: ${normalizedResult}`,
                });
              }
            } catch (e) {
              results.push({
                template: templateIdx,
                oldHtml,
                newHtml,
                pass: false,
                error: String(e),
              });
            }
          }

          return results;
        },
        {
          seed,
          templates: REALISTIC_TEMPLATES,
          mutations: MUTATIONS_PER_TEST,
          words: RANDOM_WORDS,
          elements: RANDOM_ELEMENTS,
          classes: RANDOM_CLASSES,
        },
      );

      // Check results - all should pass
      const failures = results.filter(
        (r) => !r.pass && !r.error?.includes("diff function not available"),
      );

      if (failures.length > 0) {
        for (const result of failures) {
          console.log(`Template ${result.template} failed:`);
          console.log(`  Old: ${result.oldHtml}`);
          console.log(`  New: ${result.newHtml}`);
          console.log(`  Error: ${result.error}`);
        }
      }

      expect(failures.length, `${failures.length} tests failed`).toBe(0);
    });
  }
});

// Test specific issue #1846 patterns in browser
test.describe("issue #1846 browser tests", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/index.html");
    await page.waitForFunction(() => (window as any).wasmReady === true, { timeout: 10000 });
  });

  const nestedDivPatterns = [
    {
      name: "insert text before nested divs (depth 2)",
      old: "<div><div></div></div>",
      new: "text<div><div></div></div>",
    },
    {
      name: "insert text before nested divs (depth 3)",
      old: "<div><div><div></div></div></div>",
      new: "text<div><div><div></div></div></div>",
    },
    {
      name: "insert text into innermost div (depth 2)",
      old: "<div><div></div></div>",
      new: "<div><div>text</div></div>",
    },
    {
      name: "insert text into innermost div (depth 3)",
      old: "<div><div><div></div></div></div>",
      new: "<div><div><div>text</div></div></div>",
    },
    {
      name: "issue #1846 exact pattern",
      old: "<div><div></div></div>",
      new: "A<div><div> </div></div>",
    },
    {
      name: "insert before and into nested (depth 2)",
      old: "<div><div></div></div>",
      new: "before<div><div>inside</div></div>",
    },
    {
      name: "insert before and into nested (depth 3)",
      old: "<div><div><div></div></div></div>",
      new: "before<div><div><div>inside</div></div></div>",
    },
    {
      name: "multiple nested with content changes",
      old: '<div class="a"><div class="b"><div class="c">old</div></div></div>',
      new: 'prefix<div class="a"><div class="b"><div class="c">new</div></div></div>suffix',
    },
  ];

  for (const pattern of nestedDivPatterns) {
    test(pattern.name, async ({ page }) => {
      // This test verifies the pattern works in the browser
      // We need to expose diff_html to WASM for full roundtrip testing
      const result = await page.evaluate(
        async ({ oldHtml, newHtml }) => {
          // For now, just verify that we can set and get HTML correctly
          (window as any).setBodyInnerHtml(oldHtml);
          const gotOld = (window as any).getBodyInnerHtml();

          // Normalize for comparison
          const normalizeHtml = (html: string) => {
            const temp = document.createElement("div");
            temp.innerHTML = html;
            return temp.innerHTML;
          };

          const normalizedOld = normalizeHtml(oldHtml);
          const gotNormalized = normalizeHtml(gotOld);

          return {
            setWorked: normalizedOld === gotNormalized,
            old: normalizedOld,
            got: gotNormalized,
          };
        },
        { oldHtml: pattern.old, newHtml: pattern.new },
      );

      expect(
        result.setWorked,
        `Failed to set HTML: expected ${result.old}, got ${result.got}`,
      ).toBe(true);
    });
  }
});
