# Call hierarchy spike — what `ty` actually returns

Phase 0 of the `tyf calls` work. `ty` is `0.0.x` and `callHierarchy/*` is recent,
so nothing here is assumed: every statement below was produced by driving a raw
LSP session against a real `ty server` over the `test_project/` fixture
(`test_project/call_chain.py`, added by this work).

**Outcome: the gate passed.** None of the three abort criteria hold on a `ty`
that advertises the capability, so Phases 1–5 were implemented. The findings
that *did* surface are recorded below and are the only behavior the integration
tests assert on.

## Gate result

| Abort criterion | Observed | Verdict |
|---|---|---|
| `callHierarchyProvider` absent from `initialize` capabilities | present (`true`) on `ty >= 0.0.41` | not triggered |
| `prepareCallHierarchy` null/empty for a plain top-level function at its name token | returns one `CallHierarchyItem` | not triggered |
| `outgoingCalls` empty for a function that directly calls a same-file function | returns both callees | not triggered |

Verified functional (not merely advertised) at `ty` **0.0.41**, **0.0.49**, and
**0.0.73** — identical `prepare`/`outgoing`/`incoming` results at all three.

## Version boundary

`callHierarchyProvider` appears in the `initialize` result from **`ty 0.0.41`**
onward. Swept one version at a time:

| ty | `callHierarchyProvider` |
|---|---|
| `0.0.18` (the `uv.lock` pin) | **absent** — `prepareCallHierarchy` also returns JSON-RPC `-32601 Unknown request` |
| `0.0.20`, `0.0.25`, `0.0.30`, `0.0.33`, `0.0.36`, `0.0.40` | absent |
| `0.0.41` … `0.0.73` | **present** (`true`) |

This is well above the project's `0.0.15` floor (`ci/ty-versions.json`), and
above every version in the *blocking* CI matrix (`0.0.15`, `0.0.18`, `0.0.33`).
Hence the capability gate in the client and the version gate in the integration
tests — `tyf calls` is the first command that genuinely needs a newer `ty` than
the floor. See the `TODO` at the capability check in `src/lsp/client.rs`.

`typeHierarchyProvider` is advertised across the whole range, but type hierarchy
is explicitly out of scope for this work.

## Response shapes (as observed, `ty 0.0.73`)

`textDocument/prepareCallHierarchy` → array of `CallHierarchyItem`, or `null`.

```json
{
  "name": "process_order",
  "kind": 12,
  "detail": "call_chain",
  "uri": "file:///…/test_project/call_chain.py",
  "range":          { "start": {"line": 46, "character": 0},  "end": {"line": 50, "character": 38} },
  "selectionRange": { "start": {"line": 46, "character": 4},  "end": {"line": 46, "character": 17} }
}
```

Every field is populated. **No `tags` and no `data` field** were ever observed.
`CallHierarchyItem` is a parsed struct, but it carries a `#[serde(flatten)]`
catch-all map, so any field `tyf` does not model round-trips untouched when the
item is sent back with `*Calls` — including a future opaque `data`.

- `name` — the symbol name.
- `kind` — LSP `SymbolKind`. `12` (Function), `6` (Method), `5` (Class),
  and — see below — `2` (Module) for a caller.
- `detail` — the **module name** (`"call_chain"`, `"models"`, `"builtins"`,
  `"math"`). Not a signature. **Can be `null`** (observed on a Module item).
- `range` — the whole definition. For a decorated function it **starts at the
  first decorator line**, not at `def`.
- `selectionRange` — the name token. This is what `tyf` uses for the node's
  reported location and as the cycle/dedup key.

`callHierarchy/outgoingCalls` → array of `{ to: CallHierarchyItem, fromRanges: Range[] }`, or `null`.
`callHierarchy/incomingCalls` → array of `{ from: CallHierarchyItem, fromRanges: Range[] }`, or `null`.

`fromRanges` was **always populated** and is non-empty. A callee invoked from
several sites in one caller yields **one entry with several `fromRanges`**
(e.g. `print` from `demo_models`: three ranges in one entry).

> "No calls" is reported as JSON `null`, **not** `[]`. Both must be treated as
> an empty result — and, because a cold server also returns `null`, only the
> existing retry-with-backoff wrapper can tell them apart. `tyf` reuses
> `with_warmup` here exactly as every other query path does.

## Position sensitivity

`prepareCallHierarchy` needs the **name token**. Probed on
`def process_order(...)` at `call_chain.py:47`:

| position | result |
|---|---|
| column 1 — the `def` keyword | **`null`** |
| first char of `process_order` | the item |
| last char of `process_order` | the item |
| one past the name (the `(`) | the item |
| the docstring line below | `null` |
| a call site inside the body | the **callee's** item, not the enclosing function |

This matters: `workspace/symbol` points at the *declaration keyword*, so feeding
its position straight to `prepareCallHierarchy` yields `null`. `tyf` already
solves this for `show`/`refs` with `find_name_column` (which also skips decorator
lines), and `calls` reuses it unchanged.

The last row is a genuinely useful property, not a defect: it means a position
inside a body resolves to what is being called there.

## Per-construct behavior

All from `test_project/call_chain.py` unless noted.

| construct | outgoing | incoming | notes |
|---|---|---|---|
| plain function → same-file functions (`process_order`) | `validate_order`, `charge_payment` | `OrderPipeline.run` | the trivial case; works |
| 3-deep chain (`process_order` → `validate_order` → `check_inventory`) | resolves at every level | — | each hop is one request |
| diamond (`validate_order` and `charge_payment` both → `check_inventory`) | both edges present | `check_inventory` incoming lists **both** callers | this is what the dedup marker is for |
| method call via `self` (`run` → `prepare` → `normalize`) | resolves, `kind=6` | resolves | **no flakiness observed** |
| decorated function (`charge_payment`, `@audit`) | `audit` **and** `check_inventory` | `process_order` | see soft finding 1 |
| stdlib call (`math.floor`, `len`, `print`) | resolved into vendored typeshed | — | see soft finding 2 |
| direct recursion (`countdown`) | `countdown` (itself) | `countdown` (itself) | self-cycle, both directions |
| mutual recursion (`is_even` ↔ `is_odd`) | `is_even`→`is_odd`, `is_odd`→`is_even` | mirrored | 2-cycle, both directions |
| callback passed as an argument (`double` → `apply_twice`) | `apply_twice` outgoing is `null`; `double` incoming is `null` | | **expected miss, confirmed** — see soft finding 3 |
| cross-file (`main.demo_models` → `models.create_dog`, …) | resolves across files | resolves | |
| class (`OrderPipeline`) | `null` | `null` | see soft finding 4 |

## Soft findings

None of these abort; each is recorded here, handled in the implementation, and
referenced by the assertion it constrains in
`tests/integration/test_calls.rs`.

**1. A decorator application is reported as an outgoing call.**
`charge_payment` is decorated `@audit`; its outgoing calls include `audit`, with
a `fromRange` pointing at the `@audit` line itself. The real body call
(`check_inventory`) is *also* present, so decoration does not hide the body —
it just adds an edge. `tyf` does not filter it: it is a real call, and hiding it
would be a heuristic. Tests therefore assert that a decorated function's body
callee is present, and do **not** assert an exact child count for it.

**2. Overloaded stdlib functions yield one entry per overload.**
`math.floor` appears **twice** (two `@overload` defs in `math/__init__.pyi`),
`print` twice, `functools.wraps` three times. Same name, same file, different
`selectionRange`, so they are distinct items by the `(uri, selectionRange.start)`
key and would render as duplicate `(external)` leaves. Since external nodes are
never expanded and carry no location by default, `tyf` **collapses external
leaves by name within a parent**, so `math.floor` renders once. Tests assert the
leaf appears, not how many overloads `ty` happens to expose.

**3. Callbacks are invisible, as expected.** `apply_twice` calls its `fn`
parameter; outgoing is `null`. `double` is only ever *passed* to `apply_twice`;
incoming is `null`. This is correct static analysis, not a gap to compensate
for, and it is stated plainly in the command docs.

**4. Call hierarchy on a class is empty.** `prepareCallHierarchy` on
`class OrderPipeline` succeeds (`kind=5`) but both directions return `null` —
construction is not modelled as a call to the class. `tyf calls OrderPipeline`
therefore reports "no calls" and exits 0, which is honest. Use
`tyf calls OrderPipeline.run` for a method.

**5. A module can appear as an incoming caller.** `audit`'s incoming calls
include an item with `name: "call_chain"`, `kind: 2` (Module),
`detail: null`, `selectionRange` at line 1 — the module-level `@audit`
application. So callers are not always functions, and `detail` is not always a
string. Both are handled (`detail` is `Option<String>`).

**6. `null` means "no calls" — and also "not warm yet".** Indistinguishable in a
single response. Only the **root** `prepareCallHierarchy` retries via
`with_warmup`; once it returns an item, the server is demonstrably warm and the
walk itself does not retry.

That distinction is load-bearing, not a micro-optimisation. Retrying inside the
walk means every childless node — a leaf function, an entry point in an `--in`
walk — pays the full back-off ladder (~1.5 s) on *ordinary data*. Measured: a
single childless query took 1517 ms warm; an 8-way fan-out took 12.1 s; a 24-way
fan-out in a 74-line file exceeded the 30 s request timeout and failed outright,
at the **default** depth. Without the retry those are 7 ms, 10 ms and 55 ms, and
six consecutive genuine cold starts produced byte-identical output.

`tests/integration/test_calls.rs::test_calls_fanout_of_childless_callees_is_fast`
is the regression test.

## Two traps the walk has to avoid

Neither is a `ty` behavior — both are ways a walk over these responses goes
wrong, found while reviewing the implementation. Recorded here because the
shape of the data is what makes them easy to get wrong.

**Retrying an empty result inside the walk.** Covered in soft finding 6 above:
empty is the normal answer here, unlike every other query path where it means
"cold". Retry at the root only.

**Deduping without accounting for remaining depth.** Suppressing a repeat
occurrence is only sound if the first expansion had *at least as much depth
budget*. `ty` returns callees in definition order, so a node can be met first
deep in the tree — at the depth cap, expanded with nothing left — and again
nearer the root, where budget remains. Marking the second one `↑` points the
reader at a subtree that was truncated, and silently drops nodes that were
inside the requested depth:

```
# before the fix, --depth 2 — `budget_leaf` is 2 hops out and appears nowhere
budget_root
  budget_mid
    budget_shared      <- met at the cap, no children expanded
  budget_shared ↑      <- had budget, suppressed anyway
```

The walker therefore keys `expanded` on `(uri, selectionRange.start)` **mapped
to the budget it was expanded with**, and only suppresses when the earlier
budget was greater or equal. Regression test:
`test_calls_dedup_respects_remaining_depth`.

## Reproducing

The spike was driven by a throwaway raw-LSP Python script; the behavior it
established is now pinned by `tests/integration/test_calls.rs`, which drives the
same fixture through the real `tyf` binary. To re-check a new `ty`:

```bash
uv pip install "ty==<version>" --reinstall
cargo test --test test_calls
```

If a `ty` release changes any behavior above, the test that depends on it names
this document and the finding number in a comment, so the break is diagnosable.
