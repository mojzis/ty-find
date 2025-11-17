# ty-find: Project Assessment and Roadmap

**Date**: 2025-11-17
**Status**: Initial Assessment

## Table of Contents

1. [Current State Assessment](#current-state-assessment)
2. [Usability Analysis](#usability-analysis)
3. [Daemon Mode Analysis](#daemon-mode-analysis)
4. [Additional ty LSP Features](#additional-ty-lsp-features)
5. [Claude Code Integration Opportunities](#claude-code-integration-opportunities)
6. [Recommended Roadmap](#recommended-roadmap)

---

## Current State Assessment

### Project Architecture

ty-find is a hybrid Rust/Python CLI tool that provides go-to-definition functionality for Python code by interfacing with ty's LSP server. The architecture consists of:

- **LSP Client** (`src/lsp/client.rs`): JSON-RPC client for ty LSP communication
- **LSP Server Manager** (`src/lsp/server.rs`): Spawns and manages `ty lsp` processes
- **CLI Interface** (`src/cli/`): Command-line argument parsing with clap
- **Workspace Navigation** (`src/workspace/`): Text-based symbol finding
- **Main Application** (`src/main.rs`): Orchestrates three modes: definition, find, interactive

### Build Status

**Critical Issue**: Project currently does not build due to missing tracing-subscriber dependency feature.

```
error[E0599]: no method named `with_env_filter` found for struct `SubscriberBuilder`
```

**Root Cause**: `Cargo.toml` doesn't include the `env-filter` feature for tracing-subscriber.

**Fix Required**:
```toml
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

### Current Functionality

When working, the tool provides:

1. **Definition Command**: Find definition at specific line/column
   ```bash
   ty-find definition myfile.py --line 10 --column 5
   ```

2. **Find Command**: Find all definitions of a symbol
   ```bash
   ty-find find myfile.py function_name
   ```

3. **Interactive Mode**: REPL-style interface
   ```bash
   ty-find interactive
   > myfile.py:10:5
   > find myfile.py function_name
   ```

4. **Multiple Output Formats**: human, JSON, CSV, paths-only

### Integration Tests

The project includes basic integration tests (`tests/integration/test_basic.rs`) that verify:
- Definition command functionality
- Find command functionality
- JSON output format

---

## Usability Analysis

### As-Is Usability: ⚠️ Not Ready

**Blockers**:
1. **Build Failure**: Cannot compile without dependency fix
2. **ty Dependency**: Requires ty to be installed separately (`pip install ty`)
3. **Pre-Alpha Dependency**: ty itself is in pre-alpha and not production-ready
4. **Limited Features**: Only supports go-to-definition (not hover, completion, etc.)
5. **No Error Handling**: Limited error messages for missing ty or invalid files

### What's Needed to Make It Workable

#### Phase 1: Basic Functionality (1-2 days)

1. **Fix Build Issues**
   - Add `env-filter` feature to tracing-subscriber
   - Run `cargo test` to verify all tests pass
   - Test with actual ty installation

2. **Improve Documentation**
   - Add troubleshooting section for common errors
   - Document ty installation requirements
   - Add examples with expected output

3. **Better Error Handling**
   - Clear error when ty is not installed
   - Helpful message when ty LSP server fails to start
   - Better file path validation

#### Phase 2: Production Readiness (1 week)

4. **Installation Improvements**
   - Verify maturin packaging works correctly
   - Test installation via pip from git
   - Add CI/CD for building wheels

5. **Testing**
   - Expand integration tests
   - Add unit tests for LSP protocol handling
   - Test with various Python codebases

6. **Performance Baseline**
   - Benchmark current performance
   - Measure LSP startup time
   - Profile memory usage

---

## Daemon Mode Analysis

### Current Approach: One-Shot Process Per Command

Currently, ty-find spawns a new `ty lsp` process for each command invocation:

```rust
// In TyLspServer::start()
let mut process = Command::new("ty")
    .arg("lsp")
    .spawn()?;
```

**Problems**:
- LSP server startup overhead on every command (~100-500ms)
- No caching of type information between commands
- Repeated workspace initialization
- Inefficient for interactive workflows

### Daemon Mode Benefits

**Performance Gains**:
- **First-time**: 0.5-2s (LSP server startup + indexing)
- **Subsequent**: 50-200ms (warm cache, no startup)
- **Interactive mode**: Already keeps server alive between queries (good!)

### Implementation Strategies

#### Option 1: ty-find Daemon (Recommended)

Create a persistent ty-find daemon that maintains a pool of LSP server connections.

**Architecture**:
```
┌─────────────────┐
│   ty-find CLI   │ (client)
└────────┬────────┘
         │ Unix Socket / TCP
┌────────▼────────┐
│  ty-find Daemon │
│  ┌───────────┐  │
│  │LSP Client │  │
│  │  Pool     │  │
│  └─────┬─────┘  │
└────────┼────────┘
         │
    ┌────▼────┐
    │ ty lsp  │ (one per workspace)
    └─────────┘
```

**Benefits**:
- Persistent connections to ty LSP servers
- Can maintain multiple workspace connections
- Cache symbol lookups across commands
- Lower latency for all commands (not just interactive)

**Implementation**:
```rust
// New components needed:
// 1. Daemon server (tokio TCP or Unix socket)
// 2. Client-server protocol (JSON-RPC or custom)
// 3. LSP connection pool manager
// 4. Workspace detection and routing
```

**Commands**:
```bash
ty-find daemon start          # Start daemon
ty-find daemon stop           # Stop daemon
ty-find daemon status         # Check status
ty-find definition ...        # Auto-connects to daemon
```

#### Option 2: Leverage ty's Native Daemon (If Available)

Check if ty LSP server already supports daemon mode.

**Investigation needed**:
- Does `ty lsp` have a daemon mode?
- Can it persist across multiple client connections?
- Does it support LSP server lifecycle management?

**Current status**: ty documentation doesn't mention daemon mode, but could check source code or ask maintainers.

#### Option 3: Hybrid Approach

- Use daemon mode for interactive sessions
- Keep one-shot mode for single commands
- Auto-start daemon on first interactive use

### Recommendation: Start with Option 3

1. **Short term**: Fix build issues and improve current architecture
2. **Medium term**: Implement daemon for interactive mode
3. **Long term**: Full daemon architecture if usage justifies it

**Reasoning**:
- ty is still pre-alpha; daemon adds complexity that may need frequent updates
- Interactive mode already keeps server alive (main use case)
- One-shot commands are acceptable for CI/CD and scripts
- Can gather usage data to justify full daemon investment

---

## Additional ty LSP Features

### Currently Supported by ty LSP

Based on research, ty LSP currently supports:

1. **Diagnostics** - Type errors and warnings
2. **Go-to-definition** - Navigate to symbol definitions ✅ (used by ty-find)
3. **Hover** - Show type information on hover
4. **Completions** - Code completion suggestions
5. **Inlay Hints** - Inline type annotations
6. **Semantic Tokens** - Enhanced syntax highlighting
7. **Go-to-type-definition** - Navigate to type definitions

### Planned Features (ty roadmap)

- **Auto-import** - Automatic import statement generation
- **Find references** - Find all usages of a symbol
- **Rename** - Safe symbol renaming across codebase
- **Refactorings** - Code transformation actions
- **Advanced code actions** - Quick fixes and improvements

### Features ty-find Could Expose

#### Priority 1: Core Navigation (Next Release)

1. **Hover Information**
   ```bash
   ty-find hover myfile.py --line 10 --column 5
   # Output: Type annotation, docstring, signature
   ```

2. **Go-to-type-definition**
   ```bash
   ty-find type-definition myfile.py --line 10 --column 5
   # Navigate to the class/type definition
   ```

3. **Workspace Symbols**
   ```bash
   ty-find symbols --query "MyClass"
   # Find all symbols matching pattern across workspace
   ```

#### Priority 2: Code Understanding

4. **Document Symbols**
   ```bash
   ty-find outline myfile.py
   # Show all functions, classes, methods in file
   ```

5. **Diagnostics**
   ```bash
   ty-find check myfile.py
   # Show type errors and warnings
   ty-find check --workspace
   # Check entire workspace
   ```

6. **Inlay Hints**
   ```bash
   ty-find hints myfile.py
   # Show inferred types as inline annotations
   ```

#### Priority 3: Code Intelligence (When Available in ty)

7. **Find References**
   ```bash
   ty-find references myfile.py --line 10 --column 5
   # Find all usages of symbol
   ```

8. **Rename**
   ```bash
   ty-find rename myfile.py --line 10 --column 5 --new-name "better_name"
   # Safely rename symbol across workspace
   ```

9. **Code Actions**
   ```bash
   ty-find actions myfile.py --line 10 --column 5
   # List available quick fixes and refactorings
   ```

### Implementation Strategy

**Phase 1** (1-2 weeks):
- Add hover and type-definition commands
- Implement workspace symbols search
- Add document outline/symbols command

**Phase 2** (2-3 weeks):
- Integrate diagnostics (type checking)
- Add inlay hints support
- Improve output formatting for new data types

**Phase 3** (When available in ty):
- Monitor ty releases for new features
- Implement find references
- Add rename support
- Expose code actions

### API Design Example

```rust
// In src/lsp/client.rs

impl TyLspClient {
    // New methods to add

    pub async fn hover(&self, file_path: &str, line: u32, character: u32)
        -> Result<Option<HoverInfo>> { ... }

    pub async fn workspace_symbols(&self, query: &str)
        -> Result<Vec<SymbolInformation>> { ... }

    pub async fn document_symbols(&self, file_path: &str)
        -> Result<Vec<DocumentSymbol>> { ... }

    pub async fn diagnostics(&self, file_path: Option<&str>)
        -> Result<Vec<Diagnostic>> { ... }
}
```

---

## Claude Code Integration Opportunities

### Background: Claude Code's Current Limitations

Research shows that Claude Code:
- **Does NOT index codebases** - relies on grep/search for code understanding
- Uses token-intensive retrieval strategies
- Limited semantic understanding of large codebases
- Can burn through tokens on large codebases

### The Opportunity: MCP + LSP Integration

Several projects are bridging this gap:

1. **cclsp** - Claude Code LSP integration via MCP
2. **claude-context** - Codebase indexing for Claude with vector search
3. **Code-Index-MCP** - Semantic code search for Claude Code

### How ty-find Could Enhance Claude Code

#### Strategy 1: ty-find as an MCP Server

Create an MCP (Model Context Protocol) server wrapper around ty-find.

**Architecture**:
```
┌──────────────┐
│ Claude Code  │
└──────┬───────┘
       │ MCP Protocol
┌──────▼───────┐
│   ty-find    │
│  MCP Server  │
└──────┬───────┘
       │
┌──────▼───────┐
│   ty LSP     │
└──────────────┘
```

**MCP Tools to Expose**:

```json
{
  "tools": [
    {
      "name": "ty_goto_definition",
      "description": "Find where a Python symbol is defined",
      "parameters": {
        "file_path": "string",
        "line": "number",
        "column": "number"
      }
    },
    {
      "name": "ty_find_symbol",
      "description": "Search for Python symbol across workspace",
      "parameters": {
        "symbol_name": "string",
        "workspace_path": "string"
      }
    },
    {
      "name": "ty_get_hover_info",
      "description": "Get type information and documentation for symbol",
      "parameters": {
        "file_path": "string",
        "line": "number",
        "column": "number"
      }
    },
    {
      "name": "ty_find_references",
      "description": "Find all usages of a Python symbol",
      "parameters": {
        "file_path": "string",
        "line": "number",
        "column": "number"
      }
    },
    {
      "name": "ty_get_diagnostics",
      "description": "Get type errors and warnings for file or workspace",
      "parameters": {
        "file_path": "string (optional)"
      }
    },
    {
      "name": "ty_workspace_symbols",
      "description": "Search for symbols across entire workspace",
      "parameters": {
        "query": "string"
      }
    }
  ]
}
```

**Benefits for Claude Code**:

1. **Accurate Symbol Navigation**
   - No more grep false positives
   - Follow imports and references correctly
   - Understand inheritance and method overrides

2. **Type-Aware Understanding**
   - Know actual types, not just syntax
   - Understand function signatures
   - Follow type annotations

3. **Reduced Token Usage**
   - Direct symbol lookup vs. reading entire files
   - Targeted code retrieval
   - Smaller context windows

4. **Better Code Modifications**
   - Know all usages before refactoring
   - Understand dependencies between symbols
   - Suggest type-aware changes

#### Strategy 2: Codebase Indexing + LSP

Combine ty-find with vector-based code search (like claude-context).

**Hybrid Approach**:
```
Claude Code Query
       │
       ▼
┌──────────────┐
│Vector Search │ ← Initial broad search
│(claude-ctx)  │   (semantic similarity)
└──────┬───────┘
       │ Top matches
       ▼
┌──────────────┐
│  ty-find     │ ← Precise navigation
│  (LSP)       │   (type-aware)
└──────────────┘
```

**Example Workflow**:
1. User asks: "Find all places where UserService is instantiated"
2. Vector search finds relevant files mentioning UserService
3. ty LSP finds exact instantiation points with type information
4. Return precise locations + surrounding context

#### Strategy 3: Type-Guided Code Generation

Use ty's type information to guide Claude's code generation.

**Before Code Generation**:
```python
# Claude asks via MCP:
ty_get_hover_info("services/user.py", line=45, column=10)

# Response includes:
{
  "symbol": "UserService",
  "type": "class UserService(BaseService[User])",
  "methods": ["create_user", "get_user", "update_user"],
  "constructor_signature": "__init__(self, db: Database, cache: Cache)"
}
```

**Claude generates better code**:
- Correct constructor arguments
- Proper method calls
- Type-safe code from the start

### Implementation Plan

#### Phase 1: Basic MCP Server (1-2 weeks)

1. Create new crate: `ty-find-mcp`
2. Implement MCP protocol handlers
3. Expose existing commands as MCP tools
4. Write MCP server configuration guide

#### Phase 2: Enhanced Features (2-3 weeks)

5. Add hover, references, diagnostics tools
6. Implement workspace symbol search
7. Add caching layer for frequent queries
8. Performance optimization for MCP context

#### Phase 3: Advanced Integration (1 month)

9. Combine with vector search (optional)
10. Add type-guided suggestions
11. Implement smart context retrieval
12. Create example Claude Code workflows

### Competitive Analysis

**vs. grep/search**:
- ✅ Type-aware, not text-matching
- ✅ Follows imports and inheritance
- ✅ Understands Python semantics

**vs. other LSP MCPs (cclsp)**:
- ✅ Python-specialized (ty is optimized for Python)
- ✅ Faster type checking (ty benchmarks)
- ⚠️ ty is pre-alpha (less stable)

**vs. vector search (claude-context)**:
- ✅ Precise symbol locations
- ✅ Type information included
- ⚠️ No semantic similarity search
- 💡 Best used together!

### Expected Impact

**Token Reduction**: 30-50%
- Direct symbol lookup vs. file reads
- Precise code extraction
- Less trial-and-error

**Accuracy Improvement**: 40-60%
- Type-aware suggestions
- Correct symbol resolution
- Better refactoring safety

**Developer Experience**:
- Faster responses (less token processing)
- More accurate code changes
- Better understanding of codebases

---

## Recommended Roadmap

### Phase 0: Foundation (Week 1)

**Goal**: Make project buildable and testable

- [ ] Fix tracing-subscriber dependency
- [ ] Verify cargo build succeeds
- [ ] Run and fix integration tests
- [ ] Test with real ty installation
- [ ] Update README with current status

**Success Criteria**: `cargo test --release` passes

### Phase 1: Core Improvements (Weeks 2-3)

**Goal**: Production-ready CLI tool

- [ ] Improve error handling and messages
- [ ] Add comprehensive integration tests
- [ ] Verify maturin packaging works
- [ ] Add CI/CD for building and testing
- [ ] Performance benchmarking baseline
- [ ] Documentation improvements

**Success Criteria**: Can install via pip and use reliably

### Phase 2: Feature Expansion (Weeks 4-6)

**Goal**: Expose more LSP capabilities

- [ ] Implement `hover` command
- [ ] Implement `type-definition` command
- [ ] Implement `workspace-symbols` command
- [ ] Implement `document-symbols` command
- [ ] Implement `diagnostics` command
- [ ] Add output format support for new data

**Success Criteria**: 5+ LSP features available via CLI

### Phase 3: Performance Optimization (Weeks 7-8)

**Goal**: Daemon mode for better performance

- [ ] Design daemon architecture
- [ ] Implement daemon server
- [ ] Implement client-daemon protocol
- [ ] Add LSP connection pooling
- [ ] Benchmark performance improvements
- [ ] Documentation for daemon mode

**Success Criteria**: <100ms response time for warm cache

### Phase 4: MCP Integration (Weeks 9-12)

**Goal**: Claude Code integration via MCP

- [ ] Create ty-find-mcp server crate
- [ ] Implement MCP protocol
- [ ] Expose CLI commands as MCP tools
- [ ] Add caching for MCP requests
- [ ] Write Claude Code configuration guide
- [ ] Create example workflows
- [ ] Performance testing with Claude Code

**Success Criteria**: Working MCP server usable by Claude Code

### Phase 5: Advanced Features (Weeks 13-16)

**Goal**: Full LSP feature parity

- [ ] Monitor ty releases for new features
- [ ] Implement `find-references` (when available)
- [ ] Implement `rename` (when available)
- [ ] Implement `code-actions` (when available)
- [ ] Add workspace-wide operations
- [ ] Optimize for large codebases

**Success Criteria**: Feature parity with ty LSP capabilities

---

## Key Decisions Required

### Decision 1: Daemon Mode Priority

**Question**: Implement daemon mode now or later?

**Recommendation**: Later (Phase 3)
- Fix build issues first
- Expand features second
- Optimize performance third
- ty is pre-alpha; avoid premature optimization

### Decision 2: MCP Integration Timing

**Question**: When to build MCP server?

**Recommendation**: After core features (Phase 4)
- Need stable CLI foundation first
- Multiple features make MCP more valuable
- Can iterate on CLI UX before locking MCP API

### Decision 3: ty Stability Risk

**Question**: How to handle ty's pre-alpha status?

**Recommendation**:
- Document ty version compatibility clearly
- Pin to specific ty versions in tests
- Monitor ty releases and breaking changes
- Consider abstracting LSP client for future flexibility

### Decision 4: Scope of LSP Features

**Question**: Expose all LSP features or stay focused?

**Recommendation**: Selective exposure
- Start with navigation features (goto, hover, symbols)
- Add diagnostics (high value for CI/CD)
- Wait for references/rename until ty stabilizes
- Don't duplicate functionality better served by IDEs

---

## Risks and Mitigations

### Risk 1: ty Pre-Alpha Instability

**Impact**: High
**Probability**: Medium-High

**Mitigation**:
- Pin specific ty versions in documentation
- Comprehensive integration tests
- Monitor ty release notes
- Maintain compatibility matrix
- Abstract LSP client interface

### Risk 2: Performance Without Daemon

**Impact**: Medium
**Probability**: High (already observed)

**Mitigation**:
- Start with interactive mode improvements
- Profile and optimize LSP initialization
- Implement caching where possible
- Plan daemon mode for Phase 3

### Risk 3: MCP Adoption

**Impact**: Medium
**Probability**: Low-Medium

**Mitigation**:
- Build CLI first (standalone value)
- MCP as additive feature
- Document both use cases
- Monitor MCP ecosystem growth

### Risk 4: Maintenance Burden

**Impact**: Medium
**Probability**: Medium

**Mitigation**:
- Start small, iterate based on usage
- Comprehensive test coverage
- CI/CD automation
- Clear contribution guidelines

---

## Success Metrics

### Phase 1 (Foundation)
- [ ] Build success rate: 100%
- [ ] Test pass rate: 100%
- [ ] Installation success: >95%

### Phase 2 (Features)
- [ ] Feature count: 5+ LSP features
- [ ] Error rate: <5%
- [ ] Documentation coverage: >80%

### Phase 3 (Performance)
- [ ] Cold start time: <2s
- [ ] Warm cache time: <100ms
- [ ] Memory usage: <100MB

### Phase 4 (MCP)
- [ ] Claude Code integration working
- [ ] Token reduction: >30%
- [ ] User satisfaction: >80%

---

## Conclusion

**Current State**: Not usable due to build issues, but fundamentally sound architecture.

**Near-term Focus**:
1. Fix build (1 day)
2. Improve stability and testing (1 week)
3. Expand LSP features (2-3 weeks)

**Long-term Vision**:
- High-performance Python code navigation tool
- MCP server for Claude Code integration
- Bridge between AI coding assistants and type-aware code understanding

**Key Insight**: The combination of ty's speed + LSP capabilities + MCP integration could significantly improve AI-assisted Python development, especially for Claude Code's current limitation around codebase understanding.

**Is Daemon Mode Necessary?**: Not immediately. Interactive mode already keeps server alive. Daemon mode is an optimization for Phase 3 after core features are solid.

**Best Use of ty Features**: Expose navigation (goto, hover, symbols) and diagnostics first. These provide immediate value and differentiate from simple grep. Advanced features (rename, refactor) can wait for ty to mature.

**Claude Code Integration**: High potential value. ty-find + MCP could reduce token usage and improve accuracy significantly. However, build solid CLI foundation first before adding MCP complexity.
