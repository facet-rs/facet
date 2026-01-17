import * as monaco from 'monaco-editor';
import { registerStyxLanguage } from './src/main';

registerStyxLanguage();

function tokenize(text: string): Array<{line: number, tokens: Array<{offset: number, type: string, text: string}>}> {
  const lines = text.split('\n');
  const result = monaco.editor.tokenize(text, 'styx');

  return result.map((lineTokens, lineIndex) => {
    const line = lines[lineIndex];
    const tokens = lineTokens.map((token, i) => {
      const nextOffset = lineTokens[i + 1]?.offset ?? line.length;
      return {
        offset: token.offset,
        type: token.type.replace('.styx', ''),
        text: line.slice(token.offset, nextOffset)
      };
    });
    return { line: lineIndex + 1, tokens };
  });
}

function test(name: string, input: string, expectations: Array<{line: number, text: string, expectedType: string}>) {
  const result = tokenize(input);
  let passed = true;

  for (const exp of expectations) {
    const lineResult = result[exp.line - 1];
    if (!lineResult) {
      console.error(`FAIL [${name}]: Line ${exp.line} not found`);
      passed = false;
      continue;
    }

    const token = lineResult.tokens.find(t => t.text === exp.text);
    if (!token) {
      console.error(`FAIL [${name}]: Token "${exp.text}" not found on line ${exp.line}`);
      console.error(`  Available tokens:`, lineResult.tokens);
      passed = false;
      continue;
    }

    if (token.type !== exp.expectedType) {
      console.error(`FAIL [${name}]: "${exp.text}" on line ${exp.line}`);
      console.error(`  Expected: ${exp.expectedType}`);
      console.error(`  Got: ${token.type}`);
      passed = false;
    }
  }

  if (passed) {
    console.log(`PASS [${name}]`);
  }
  return passed;
}

// Test cases
console.log('=== Styx Monaco Grammar Tests ===\n');

test('simple entry',
  'name hello',
  [
    { line: 1, text: 'name', expectedType: 'key' },
    { line: 1, text: 'hello', expectedType: 'value' },
  ]
);

test('nested object',
  `server {
    host localhost
    port 8080
}`,
  [
    { line: 1, text: 'server', expectedType: 'key' },
    { line: 2, text: 'host', expectedType: 'key' },
    { line: 2, text: 'localhost', expectedType: 'value' },
    { line: 3, text: 'port', expectedType: 'key' },
    { line: 3, text: '8080', expectedType: 'value' },
  ]
);

test('deeply nested',
  'a {b {c value}}',
  [
    { line: 1, text: 'a', expectedType: 'key' },
    { line: 1, text: 'b', expectedType: 'key' },
    { line: 1, text: 'c', expectedType: 'key' },
    { line: 1, text: 'value', expectedType: 'value' },
  ]
);

test('sequence values',
  'numbers (1 2 3)',
  [
    { line: 1, text: 'numbers', expectedType: 'key' },
    { line: 1, text: '1', expectedType: 'value' },
    { line: 1, text: '2', expectedType: 'value' },
    { line: 1, text: '3', expectedType: 'value' },
  ]
);

test('tag as value',
  'timeout @duration(30s)',
  [
    { line: 1, text: 'timeout', expectedType: 'key' },
    { line: 1, text: '@duration', expectedType: 'tag' },
  ]
);

test('multiple entries multiline',
  `host localhost
port 8080
enabled true`,
  [
    { line: 1, text: 'host', expectedType: 'key' },
    { line: 1, text: 'localhost', expectedType: 'value' },
    { line: 2, text: 'port', expectedType: 'key' },
    { line: 2, text: '8080', expectedType: 'value' },
    { line: 3, text: 'enabled', expectedType: 'key' },
    { line: 3, text: 'true', expectedType: 'value' },
  ]
);

console.log('\n=== Debug: tokenize nested object ===');
const nested = tokenize(`server {
    host localhost
    port 8080
}`);
nested.forEach(line => {
  console.log(`Line ${line.line}:`, line.tokens.map(t => `"${t.text}"=${t.type}`).join(', '));
});
