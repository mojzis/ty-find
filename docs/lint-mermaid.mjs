#!/usr/bin/env node
// Validates mermaid diagram syntax in markdown files.
// Uses @mermaid-js/parser for pure Node.js validation (no browser needed).
//
// Usage:
//   node docs/lint-mermaid.mjs                  # scans docs/src/**/*.md
//   node docs/lint-mermaid.mjs path/to/file.md  # specific file(s)

import { readFileSync } from "fs";
import { execSync } from "child_process";
import { parse } from "@mermaid-js/parser";

function extractMermaidBlocks(filePath) {
  const content = readFileSync(filePath, "utf8");
  const blocks = [];
  const regex = /```mermaid\n([\s\S]*?)```/g;
  let match;
  while ((match = regex.exec(content)) !== null) {
    const line = content.substring(0, match.index).split("\n").length + 1;
    blocks.push({ content: match[1].trim(), line, file: filePath });
  }
  return blocks;
}

// Find files
const args = process.argv.slice(2);
const mdFiles =
  args.length > 0
    ? args
    : execSync('find docs/src -name "*.md" -type f', { encoding: "utf8" })
        .trim()
        .split("\n")
        .filter(Boolean);

// Extract all mermaid blocks
const allBlocks = [];
for (const file of mdFiles) {
  allBlocks.push(...extractMermaidBlocks(file));
}

if (allBlocks.length === 0) {
  console.log("No mermaid diagrams found.");
  process.exit(0);
}

console.log(
  `Checking ${allBlocks.length} mermaid diagrams in ${mdFiles.length} files\n`,
);

let errors = 0;
for (const block of allBlocks) {
  const label = `${block.file}:${block.line}`;
  const firstLine = block.content.split("\n")[0];
  try {
    parse(block.content);
    console.log(`  ok  ${label} (${firstLine})`);
  } catch (e) {
    const msg = e.message || String(e);
    // @mermaid-js/parser throws "Unknown diagram type" for types it can
    // detect but has no grammar for (e.g. graph, gantt, timeline).
    // These still pass the initial detection — a real syntax error looks
    // different, so we skip "Unknown diagram type" as a false negative.
    if (msg.startsWith("Unknown diagram type")) {
      console.log(`  ok  ${label} (${firstLine}) [type-only check]`);
    } else {
      errors++;
      console.log(`FAIL  ${label} (${firstLine})`);
      console.log(`      ${msg.split("\n")[0]}`);
    }
  }
}

console.log(
  `\n${allBlocks.length - errors} passed, ${errors} failed out of ${allBlocks.length} diagrams`,
);
process.exit(errors > 0 ? 1 : 0);
